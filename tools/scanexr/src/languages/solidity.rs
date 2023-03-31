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

#[derive(Debug, Clone, Copy)]
pub enum StepMeta {
    Start,
    GotoDefinition,
    GotoArgument(usize),
    GotoReference,
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

        let breadcrumbs_ = breadcrumbs.iter().map(|n| n.kind()).collect::<Vec<_>>();

        eprintln!(
            "Got location:\nbreadcrumbs: {:?}\n\n{}\n",
            &breadcrumbs_,
            crate::test_utils::display_location(&location, &state, Some(5))
        );

        match (
            state,
            breadcrumbs_.as_slice(),
            breadcrumbs.as_slice(),
            definitions,
            references,
        ) {
            (
                StepMeta::GotoDefinition,
                ["identifier", "variable_declaration", "variable_declaration_statement", ..],
                [_, _, variable_declaration_statement, ..],
                _,
                _,
            ) => Ok(vec![(
                get_node_location(
                    location.uri,
                    &variable_declaration_statement
                        .child_by_field_name("value")
                        .unwrap(),
                ),
                vec![],
            )]),
            (
                StepMeta::GotoDefinition,
                ["identifier", "member_expression" | "call_argument", ..],
                _,
                Ok(definitions),
                _,
            ) => Ok(definitions.into_iter().map(|d| (d, vec![])).collect()),
            (
                StepMeta::GotoReference,
                ["identifier", "function_definition", ..],
                _,
                _,
                Ok(references),
            ) => Ok(references.into_iter().map(|d| (d, vec![])).collect()),
            (
                StepMeta::Start,
                [_, "parameter", "function_definition"],
                [_, parameter, function_definition],
                _,
                _,
            ) => Ok(vec![(
                get_node_location(
                    location.uri,
                    &function_definition.child_by_field_name("name").unwrap(),
                ),
                vec![
                    StepMeta::Start,
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
            )]),
            (
                StepMeta::GotoArgument(index),
                ["identifier", "call_expression", ..],
                [_, call_expression, ..],
                _,
                _,
            ) => {
                let mut cursor = call_expression.walk();
                let arg = call_expression
                    .named_children(&mut cursor)
                    .filter(|a| a.kind() == "call_argument")
                    .nth(index)
                    .expect("failed to go to argument");
                Ok(vec![(get_node_location(location.uri, &arg), vec![])])
            }
            (StepMeta::Start, ["identifier", ..], _, _, _) => Ok(vec![(
                location,
                vec![StepMeta::Start, StepMeta::GotoDefinition],
            )]),
            _ => todo!(),
        }
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
