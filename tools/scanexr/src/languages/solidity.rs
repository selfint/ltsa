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
    converter::{Convert, Converter},
    language_provider::{self, LanguageAutomata, LspProvider, SupportedLanguage},
    utils::{
        get_breadcrumbs, get_named_child_index, get_node_location, get_query_results,
        get_uri_content, parse_file,
    },
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
    Resolve {
        /// Use this when a call expression needs to be resolved.
        ///
        /// The Location anchor is here so that if the resolved return value
        /// is a parameter, we can return to the correct call expression
        anchor: Option<Location>,
        index: Option<usize>,
    },
}

impl LanguageAutomata for Solidity {
    type Stack = StepMeta;
    type LspProvider = SolidityLs;

    fn get_language(&self) -> tree_sitter::Language {
        tree_sitter_solidity::language()
    }

    fn initial_state(&self) -> Vec<Self::Stack> {
        vec![StepMeta::Start]
    }

    fn transition(
        &self,
        location: Location,
        state: Self::Stack,
        definitions: Result<Vec<Location>>,
        references: Result<Vec<Location>>,
    ) -> Result<Vec<(Location, Vec<Self::Stack>)>> {
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
            "Got location:\nbreadcrumbs: {:?}\n\n{}\n",
            breadcrumbs.iter().map(|(k, _)| k).collect::<Vec<_>>(),
            crate::test_utils::display_location(&location, &state, Some(5))
        );

        Ok(
            match (state, breadcrumbs.as_slice(), definitions, references) {
                (_, [("number_literal", _), ..], _, _) => vec![],
                (
                    state @ StepMeta::Resolve { .. },
                    [("identifier", _), ("variable_declaration", decl), ("variable_declaration_tuple", tuple), ..],
                    _,
                    _,
                ) => {
                    vec![(
                        get_node_location(location.uri, tuple),
                        vec![
                            state,
                            StepMeta::Resolve {
                                anchor: None,
                                index: Some(get_named_child_index(tuple, decl).unwrap()),
                            },
                        ],
                    )]
                }
                (
                    state @ StepMeta::Resolve { .. },
                    [("identifier", _), ("variable_declaration", variable_declaration), ..],
                    _,
                    _,
                ) => {
                    vec![(
                        get_node_location(location.uri, variable_declaration),
                        vec![state],
                    )]
                }
                (
                    state @ StepMeta::Resolve { .. },
                    [("variable_declaration" | "variable_declaration_tuple", _), ("variable_declaration_statement", variable_declaration_statement), ..],
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
                        vec![state],
                    )]
                }
                (
                    StepMeta::GotoDefinition,
                    [("identifier", _), (
                        "member_expression" | "call_argument" | "call_expression"
                        | "return_statement" | "tuple_expression",
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
                    StepMeta::Resolve {
                        anchor: Some(anchor),
                        index: None,
                    },
                    [_, ("parameter", parameter), ("function_definition", function_definition), ..],
                    _,
                    _,
                ) => vec![(
                    anchor,
                    vec![
                        StepMeta::Resolve {
                            anchor: None,
                            index: None,
                        },
                        StepMeta::GotoArgument(
                            get_named_child_index(function_definition, parameter).unwrap(),
                        ),
                    ],
                )],
                (
                    StepMeta::Resolve {
                        anchor: None,
                        index: None,
                    },
                    [_, ("parameter", parameter), ("function_definition", function_definition), ..],
                    _,
                    _,
                ) => vec![(
                    get_node_location(
                        location.uri,
                        &function_definition.child_by_field_name("name").unwrap(),
                    ),
                    vec![
                        StepMeta::Resolve {
                            anchor: None,
                            index: None,
                        },
                        StepMeta::GotoArgument(
                            get_named_child_index(function_definition, parameter).unwrap(),
                        ),
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
                (
                    StepMeta::Resolve { anchor, index },
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
                            StepMeta::Resolve { anchor, index },
                            StepMeta::Resolve {
                                anchor: Some(function_call),
                                index,
                            },
                            StepMeta::GotoDefinition,
                        ],
                    )]
                }
                (
                    state @ StepMeta::Resolve { .. },
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
                        vec![state.clone()],
                    )
                })
                .collect::<Vec<_>>(),
                (
                    StepMeta::Resolve {
                        anchor,
                        index: Some(index),
                    },
                    [("tuple_expression", tuple_expression), ..],
                    _,
                    _,
                ) => {
                    let mut cursor = tuple_expression.walk();
                    let item = tuple_expression
                        .named_children(&mut cursor)
                        .nth(index)
                        .expect("failed to go to argument");
                    let item_location = get_node_location(location.uri, &item);

                    vec![(
                        item_location,
                        vec![StepMeta::Resolve {
                            anchor,
                            index: None,
                        }],
                    )]
                }
                (
                    state @ StepMeta::Resolve { .. },
                    [("identifier", _), (
                        "member_expression" | "call_argument" | "return_statement"
                        | "tuple_expression",
                        _,
                    ), ..],
                    _,
                    _,
                ) => {
                    vec![(location, vec![state, StepMeta::GotoDefinition])]
                }
                (StepMeta::Start, _, _, _) => {
                    vec![(
                        location,
                        vec![
                            StepMeta::Start,
                            StepMeta::Resolve {
                                anchor: None,
                                index: None,
                            },
                        ],
                    )]
                }
                _ => vec![],
            },
        )
    }
}

#[async_trait]
impl SupportedLanguage for Solidity {
    fn get_start_end(&self, project_files: &[PathBuf]) -> Result<(Vec<Location>, Vec<Location>)> {
        let mut start_locations = vec![];
        let mut end_locations = vec![];

        for project_file in project_files {
            let (text, tree) = parse_file(project_file)?;
            let root = tree.root_node();

            end_locations.extend(
                get_query_results(
                    &text,
                    root,
                    &Query::new(
                        tree_sitter_solidity::language(),
                        r#"
            (member_expression
                object: (identifier) @obj (#match? @obj "msg")
                property: (identifier) @prop (#match? @prop "sender")
            ) @pub
            "#,
                    )
                    .unwrap(),
                    2,
                )
                .iter()
                .map(|n| get_node_location(Converter::convert(project_file.as_path()), n)),
            );

            start_locations.extend(
                get_query_results(
                    &text,
                    root,
                    &Query::new(
                        tree_sitter_solidity::language(),
                        r#"
        (call_expression
            function: (struct_expression
                type: (member_expression
                    object: (identifier) @hacky
                    property: (identifier) @method
                    (#match? @method "call")
                )
            )
        )
        "#,
                    )
                    .unwrap(),
                    0,
                )
                .iter()
                .map(|n| get_node_location(Converter::convert(project_file.as_path()), n)),
            );
        }

        Ok((start_locations, end_locations))
    }
    async fn find_paths(
        &self,
        root_dir: &Path,
        project_files: Vec<PathBuf>,
        start_locations: Vec<Location>,
        stop_at: &[Location],
    ) -> Result<Vec<Vec<Location>>> {
        let lsp = SolidityLs::new(root_dir, project_files)
            .await
            .context("failed to start solidity ls")?;

        let mut all_paths = vec![];
        for start_location in start_locations {
            let paths = language_provider::find_paths(
                &Solidity,
                &lsp,
                start_location,
                Solidity.initial_state(),
                stop_at,
            )
            .await?;

            all_paths.extend(paths);
        }

        Ok(all_paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::display_locations;
    use crate::test_utils::setup_test_dir;

    macro_rules! snapshot {
        ($name:tt, $state:expr, $input:literal) => {
            #[test]
            fn $name() {
                let (_root_dir, location, definitions, references) = setup_test_dir($input);

                let solidity = Solidity;

                let next_steps = solidity
                    .transition(location, $state, Ok(definitions), Ok(references))
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
            }
        };
    }

    snapshot!(
        test_start,
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
        goto_definition,
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
