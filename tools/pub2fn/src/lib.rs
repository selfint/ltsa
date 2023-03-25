use anyhow::Result;
use async_recursion::async_recursion;
use lsp_client::client::Client as LspClient;
use lsp_types::{notification::*, request::*, *};
use std::{
    fs::DirEntry,
    path::{Path, PathBuf},
};
use tree_sitter::{Language, Node, Query, QueryCursor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspMethod {
    Nop,
    Definition,
    References,
}

pub trait LanguageProvider: Send + Sync {
    fn get_next_step(&self, step: &Step) -> Option<(LspMethod, Step)>;
    fn get_definition_parents(&self, response: GotoDefinitionResponse) -> Vec<Step>;
    fn get_references_parents(&self, response: Vec<Location>) -> Vec<Step>;
}

pub async fn get_all_paths(
    root_dir: &Path,
    lsp_client: &LspClient,
    language: Language,
    pub_query: (Query, u32),
    hacky_query: (Query, u32),
    language_provider: impl LanguageProvider,
) -> Result<Vec<Vec<Step>>> {
    let pub_locations = get_query_locations(root_dir, language, &pub_query)?;
    let hacky_locations = get_query_locations(root_dir, language, &hacky_query)?;

    let mut step_paths = vec![];
    for pub_location in &pub_locations {
        for hacky_location in &hacky_locations {
            if let Some(steps) = get_steps(
                lsp_client,
                &language_provider,
                pub_location,
                hacky_location,
                vec![hacky_location.clone()],
            )
            .await?
            {
                step_paths.push(steps);
            }
        }
    }

    Ok(step_paths)
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Step {
    pub path: PathBuf,
    pub start: (u32, u32),
    pub end: (u32, u32),
}

impl Step {
    pub fn new(path: PathBuf, start: (u32, u32), end: (u32, u32)) -> Self {
        Self { path, start, end }
    }
}

#[async_recursion]
async fn get_steps(
    lsp_client: &LspClient,
    language_provider: &impl LanguageProvider,
    src: &Step,
    dst: &Step,
    steps: Vec<Step>,
) -> Result<Option<Vec<Step>>> {
    if src == dst {
        return Ok(Some(steps));
    }

    let Some((method, next_step)) = language_provider.get_next_step(dst) else {
        return Ok(None);
    };

    let parents: Vec<Step> = match method {
        LspMethod::Nop => vec![next_step],
        LspMethod::Definition => {
            let response = lsp_client
                .request::<GotoDefinition>(GotoDeclarationParams {
                    text_document_position_params: TextDocumentPositionParams::from(&next_step),
                    work_done_progress_params: WorkDoneProgressParams {
                        work_done_token: None,
                    },
                    partial_result_params: PartialResultParams {
                        partial_result_token: None,
                    },
                })
                .await?
                .result
                .as_result()
                .map_err(anyhow::Error::msg)?
                .expect("failed to get definition");

            language_provider.get_definition_parents(response)
        }
        LspMethod::References => {
            let response = lsp_client
                .request::<References>(ReferenceParams {
                    text_document_position: TextDocumentPositionParams::from(&next_step),
                    work_done_progress_params: WorkDoneProgressParams {
                        work_done_token: None,
                    },
                    partial_result_params: PartialResultParams {
                        partial_result_token: None,
                    },
                    context: ReferenceContext {
                        include_declaration: false,
                    },
                })
                .await?
                .result
                .as_result()
                .map_err(anyhow::Error::msg)?
                .expect("failed to get references");

            language_provider.get_references_parents(response)
        }
    };

    for new_dst in parents {
        if !steps.contains(&new_dst) {
            let mut new_steps = steps.clone();
            new_steps.push(new_dst.clone());

            if let Some(next_step) =
                get_steps(lsp_client, language_provider, src, &new_dst, new_steps).await?
            {
                return Ok(Some(next_step));
            }
        }
    }

    Ok(None)
}

fn get_query_locations(
    root_dir: &Path,
    language: Language,
    query: &(Query, u32),
) -> Result<Vec<Step>> {
    fn visit_dirs(dir: &Path, cb: &mut impl FnMut(&DirEntry)) -> std::io::Result<()> {
        if dir.is_dir() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    visit_dirs(&path, cb)?;
                } else {
                    cb(&entry);
                }
            }
        }
        Ok(())
    }

    let mut locations = vec![];
    visit_dirs(root_dir, &mut |dir| {
        let content = String::from_utf8(std::fs::read(dir.path()).expect("failed to read file"))
            .expect("got non-utf8 file");

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(language)
            .expect("failed to set language on parser");
        let tree = parser
            .parse(&content, None)
            .expect("failed to parse content");

        let results = get_query_results(&content, tree.root_node(), &query.0, query.1);

        for result in results {
            let start = result.start_position();
            let start = (start.row as u32, start.column as u32);
            let end = result.end_position();
            let end = (end.row as u32, end.column as u32);
            locations.push(Step::new(dir.path(), start, end));
        }
    })?;

    Ok(locations)
}

pub fn get_query_results<'a>(
    text: &str,
    root: tree_sitter::Node<'a>,
    query: &Query,
    capture_index: u32,
) -> Vec<Node<'a>> {
    let mut query_cursor = QueryCursor::new();
    let captures = query_cursor.captures(query, root, text.as_bytes());

    let mut nodes = vec![];

    for (q_match, index) in captures {
        if index != 0 {
            continue;
        }

        for capture in q_match.captures {
            if capture.index == capture_index {
                nodes.push(capture.node);
            }
        }
    }

    nodes
}

impl From<&Step> for TextDocumentPositionParams {
    fn from(step: &Step) -> Self {
        Self {
            text_document: TextDocumentIdentifier {
                uri: Url::from_file_path(&step.path).unwrap(),
            },
            position: Position {
                line: step.start.0,
                character: step.start.1,
            },
        }
    }
}
