use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use lsp_client::client::Client;
use lsp_types::{
    notification::Initialized,
    request::{GotoDeclarationParams, GotoDefinition, Initialize},
    GotoDefinitionResponse, InitializeParams, InitializedParams, Location, PartialResultParams,
    Range, TextDocumentPositionParams, Url, WorkDoneProgressParams,
};
use tokio::{
    process::{Child, Command},
    task::JoinHandle,
};
use tree_sitter::Query;

use crate::{
    get_uri_content,
    language_provider::{get_breadcrumbs, get_node_location, LanguageProvider, LspProvider},
    utils::get_query_results,
    Convert, Converter,
};

pub struct SolidityLs {
    client: Client,
    project_files: Vec<PathBuf>,
    handles: Vec<JoinHandle<()>>,
}

impl SolidityLs {
    fn start_solidity_ls() -> Child {
        Command::new("solc")
            .arg("--lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to start solidity ls")
    }

    pub async fn new(root_dir: &Path, project_files: Vec<PathBuf>) -> Result<Self> {
        let (client, handles) = lsp_client::clients::child_client(SolidityLs::start_solidity_ls());
        client
            .request::<Initialize>(InitializeParams {
                root_uri: Some(
                    Url::from_file_path(root_dir)
                        .map_err(|_| anyhow!("failed to convert root dir to url"))?,
                ),
                ..Default::default()
            })
            .await??;

        client.notify::<Initialized>(InitializedParams {})?;

        Ok(Self {
            client,
            project_files,
            handles,
        })
    }
}

impl Drop for SolidityLs {
    fn drop(&mut self) {
        for handle in &self.handles {
            handle.abort();
        }
    }
}

#[async_trait]
impl LspProvider for SolidityLs {
    async fn find_definitions(&self, location: &Location) -> Result<Vec<Location>> {
        let definitions = self
            .client
            .request::<GotoDefinition>(GotoDeclarationParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: lsp_types::TextDocumentIdentifier {
                        uri: location.uri.clone(),
                    },
                    position: location.range.start,
                },
                work_done_progress_params: WorkDoneProgressParams {
                    work_done_token: None,
                },
                partial_result_params: PartialResultParams {
                    partial_result_token: None,
                },
            })
            .await
            .context("awaiting goto definition response")?
            .context("getting goto definition result")?;

        if let Some(definitions) = definitions {
            Ok(match definitions {
                GotoDefinitionResponse::Scalar(definition) => vec![definition],
                GotoDefinitionResponse::Array(definitions) => definitions,
                GotoDefinitionResponse::Link(_) => todo!("what is link?"),
            })
        } else {
            Ok(vec![])
        }
    }

    async fn find_references(&self, location: &Location) -> Result<Vec<Location>> {
        let query = (
            Query::new(
                tree_sitter_solidity::language(),
                "(call_expression function: [
                    (identifier) @ident
                    (member_expression
                        property: (identifier) @ident
                    )
                ])",
            )
            .unwrap(),
            0,
        );

        let call_locations = {
            let language = tree_sitter_solidity::language();

            let mut locations = vec![];
            for project_file in &self.project_files {
                let content =
                    String::from_utf8(std::fs::read(project_file).context("failed to read file")?)
                        .context("got non-utf8 file")?;

                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(language)
                    .expect("failed to set language on parser");
                let tree = parser
                    .parse(&content, None)
                    .expect("failed to parse content");

                let results = get_query_results(&content, tree.root_node(), &query.0, query.1);

                for result in results {
                    let start = Converter::convert(result.start_position());
                    let end = Converter::convert(result.end_position());
                    let url = Converter::convert(project_file.as_path());

                    locations.push(Location::new(url, Range::new(start, end)));
                }
            }

            locations
        };

        let mut references = vec![];
        for call_location in call_locations {
            let Ok(definitions) = self.find_definitions(&call_location).await
            else {
                continue;
            };

            for definition in definitions {
                if &definition == location {
                    references.push(call_location.clone());
                }
            }
        }

        Ok(references)
    }
}

pub struct Solidity;

#[derive(Debug, Clone)]
pub enum StepMeta {
    Start,
    GotoDefinition,
    GotoArgument(usize),
    GotoReference,
    /// Use this when a call expression needs to be resolved.
    ///
    /// The Location anchor is here so that if the resolved return value
    /// is a parameter, we can return to the correct call expression
    ResolveReturnValue(Location),
    Resolve,
}

impl LanguageProvider for Solidity {
    type State = StepMeta;
    type LspProvider = SolidityLs;

    fn get_language(&self) -> tree_sitter::Language {
        tree_sitter_solidity::language()
    }

    fn initial_state(&self) -> Vec<Self::State> {
        vec![StepMeta::Start]
    }

    fn get_next_steps(
        &self,
        location: Location,
        state: Self::State,
        definitions: Result<Vec<Location>>,
        references: Result<Vec<Location>>,
    ) -> Result<Vec<(Location, Vec<Self::State>)>> {
        let tree = self.get_tree(&location)?;
        let root = tree.root_node();

        let Some(breadcrumbs) = get_breadcrumbs(root, &location) else {
            todo!("when does this happen?")
        };

        let breadcrumbs = breadcrumbs
            .into_iter()
            .map(|n| (n.kind(), n))
            .collect::<Vec<_>>();

        eprintln!(
            "Got location:\nbreadcrumbs: {:?}\nstate: {:?}\n\n{}\n",
            breadcrumbs.iter().map(|(k, _)| k).collect::<Vec<_>>(),
            &state,
            crate::test_utils::display_location(&location, &state, Some(5))
        );

        Ok(
            match (state, breadcrumbs.as_slice(), definitions, references) {
                (_, [("number_literal", _), ..], _, _) => vec![],
                (
                    StepMeta::Resolve,
                    [("identifier", _), ("variable_declaration", _), ("variable_declaration_statement", variable_declaration_statement), ..],
                    _,
                    _,
                ) => {
                    vec![(
                        get_node_location(
                            location.uri,
                            &variable_declaration_statement
                                .child_by_field_name("value")
                                .unwrap(),
                        ),
                        vec![StepMeta::Resolve],
                    )]
                }
                (
                    StepMeta::GotoDefinition,
                    [("identifier", _), (
                        "member_expression" | "call_argument" | "call_expression"
                        | "return_statement",
                        _,
                    ), ..],
                    Ok(definitions),
                    _,
                ) => definitions.into_iter().map(|d| (d, vec![])).collect(),
                (
                    StepMeta::GotoReference,
                    [("identifier", _), ("function_definition", _), ..],
                    _,
                    Ok(references),
                ) => references.into_iter().map(|d| (d, vec![])).collect(),
                (
                    StepMeta::ResolveReturnValue(anchor),
                    [_, ("parameter", parameter), ("function_definition", function_definition), ..],
                    _,
                    _,
                ) => vec![(
                    anchor,
                    vec![
                        StepMeta::Resolve,
                        StepMeta::GotoArgument({
                            let mut cursor = function_definition.walk();
                            let index = function_definition
                                .named_children(&mut cursor)
                                .filter(|p| p.kind() == "parameter")
                                .position(|p| &p == parameter)
                                .unwrap();

                            index
                        }),
                    ],
                )],
                (
                    StepMeta::Resolve,
                    [_, ("parameter", parameter), ("function_definition", function_definition), ..],
                    _,
                    _,
                ) => vec![(
                    get_node_location(
                        location.uri,
                        &function_definition.child_by_field_name("name").unwrap(),
                    ),
                    vec![
                        StepMeta::Resolve,
                        StepMeta::GotoArgument({
                            let mut cursor = function_definition.walk();
                            let index = function_definition
                                .named_children(&mut cursor)
                                .filter(|p| p.kind() == "parameter")
                                .position(|p| &p == parameter)
                                .unwrap();

                            index
                        }),
                        StepMeta::GotoReference,
                    ],
                )],
                (
                    StepMeta::GotoArgument(index),
                    [("identifier", _), ("call_expression", call_expression), ..]
                    | [("call_expression", call_expression), ..],
                    _,
                    _,
                ) => {
                    let mut cursor = call_expression.walk();
                    let arg = call_expression
                        .named_children(&mut cursor)
                        .filter(|a| a.kind() == "call_argument")
                        .nth(index)
                        .expect("failed to go to argument");
                    vec![(get_node_location(location.uri, &arg), vec![])]
                }
                (StepMeta::Resolve, [("call_expression", _), ..], _, _) => {
                    vec![(
                        location.clone(),
                        vec![StepMeta::Resolve, StepMeta::ResolveReturnValue(location)],
                    )]
                }
                (
                    StepMeta::ResolveReturnValue(anchor),
                    [("call_expression", call_expression), ..],
                    _,
                    _,
                ) => {
                    let function_call = get_node_location(
                        location.uri,
                        &call_expression.child_by_field_name("function").unwrap(),
                    );
                    vec![(
                        function_call.clone(),
                        vec![
                            StepMeta::ResolveReturnValue(anchor),
                            StepMeta::ResolveReturnValue(function_call),
                            StepMeta::GotoDefinition,
                        ],
                    )]
                }
                (
                    StepMeta::ResolveReturnValue(anchor),
                    [("identifier", _), ("function_definition", function_definition), ..],
                    _,
                    _,
                ) => get_query_results(
                    &get_uri_content(&location.uri)?,
                    *function_definition,
                    &Query::new(self.get_language(), "(return_statement (_) @return)").unwrap(),
                    0,
                )
                .iter()
                .map(|node| {
                    (
                        get_node_location(location.uri.clone(), node),
                        vec![StepMeta::ResolveReturnValue(anchor.clone())],
                    )
                })
                .collect::<Vec<_>>(),
                (
                    state @ (StepMeta::Resolve | StepMeta::ResolveReturnValue(..)),
                    [("identifier", _), ("member_expression" | "call_argument" | "return_statement", _), ..],
                    _,
                    _,
                ) => {
                    vec![(location, vec![state, StepMeta::GotoDefinition])]
                }
                (StepMeta::Start, _, _, _) => {
                    vec![(location, vec![StepMeta::Start, StepMeta::Resolve])]
                }
                _ => todo!(),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::display_locations;
    use crate::test_utils::setup_test_dir;

    macro_rules! snapshot {
        ($state:expr, $input:literal) => {
            let (_root_dir, location, definitions, references) = setup_test_dir($input);

            let solidity = Solidity;

            let next_steps = solidity
                .get_next_steps(location, $state, Ok(definitions), Ok(references))
                .expect("failed");

            let next_steps = display_locations(next_steps, None);
            let snapshot = format!(
                r#"
--- input ---
{}

--- output ---
{}
            "#,
                $input, next_steps
            );

            insta::assert_snapshot!(snapshot);
        };
    }

    #[test]
    fn test_solidity() {
        snapshot!(
            StepMeta::Start,
            r#"
contract.sol
#@#
contract Contract {
    function withdraw() public {
        uint bal = balances[msg.sender];
        require(bal > 0);

        address target = msg.sender;
             // ^^^^^^ definition

        (bool sent, ) = target.call{value: bal}("");
                     // ^^^^^^ start
        balances[msg.sender] = 0;
    }
}
        "#
        );

        snapshot!(
            StepMeta::GotoDefinition,
            r#"
contract.sol
#@#
contract Contract {
    function withdraw() public {
        uint bal = balances[msg.sender];
        require(bal > 0);

        address target = msg.sender;
             // ^^^^^^ definition

        (bool sent, ) = target.call{value: bal}("");
                     // ^^^^^^ start
        balances[msg.sender] = 0;
    }
}
        "#
        );
    }
}
