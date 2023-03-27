pub mod language_provider;

use anyhow::Result;
use async_recursion::async_recursion;
use lsp_client::client::Client as LspClient;
use lsp_types::{request::*, *};
use std::{
    fmt::Debug,
    fs::DirEntry,
    path::{Path, PathBuf},
};
use tree_sitter::{Language, Node, Query, QueryCursor};

#[derive(Debug, PartialEq)]
pub enum LspMethod {
    Nop,
    Definition,
    References,
    ReverseDefinition {
        query: (Query, u32),
        language: Language,
    },
}

pub trait LanguageProvider: Send + Sync {
    type Context: Send + Sync + Clone + Debug + PartialEq;

    fn get_previous_step(
        &self,
        step: &Step<Self::Context>,
        previous_step: Option<&Step<Self::Context>>,
    ) -> Option<Vec<(LspMethod, Step<Self::Context>, Vec<Step<Self::Context>>)>>;
}

pub async fn get_all_paths<P: LanguageProvider>(
    root_dir: &Path,
    lsp_client: &LspClient,
    language: Language,
    pub_query: (Query, u32),
    hacky_query: (Query, u32),
    language_provider: P,
) -> Result<Vec<Vec<Step<P::Context>>>> {
    let pub_locations = get_query_locations(root_dir, language, &pub_query)?;
    let hacky_locations = get_query_locations(root_dir, language, &hacky_query)?;

    let mut step_paths = vec![];
    for hacky_location in &hacky_locations {
        if let Some(mut steps) = get_steps(
            root_dir,
            lsp_client,
            &language_provider,
            &pub_locations,
            hacky_location,
            vec![],
        )
        .await?
        {
            steps.reverse();
            step_paths.push(steps);
        }
    }

    Ok(step_paths)
}

#[derive(Eq, Debug, Clone)]
pub struct Step<C> {
    pub path: PathBuf,
    pub start: (u32, u32),
    pub end: (u32, u32),
    pub context: Option<C>,
}

impl<C: PartialEq> PartialEq for Step<C> {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
            // && self.context == other.context
            && self.start == other.start
            && self.end == other.end
    }
}

impl<C> Step<C> {
    pub fn new(path: PathBuf, start: (u32, u32), end: (u32, u32)) -> Self {
        Self {
            path,
            start,
            end,
            context: None,
        }
    }
}

#[async_recursion]
async fn get_steps<P: LanguageProvider>(
    root_dir: &Path,
    lsp_client: &LspClient,
    language_provider: &P,
    sources: &[Step<P::Context>],
    dst: &Step<P::Context>,
    steps: Vec<Step<P::Context>>,
) -> Result<Option<Vec<Step<P::Context>>>> {
    if sources.contains(dst) {
        return Ok(Some(steps));
    }

    let Some(next_steps) = language_provider.get_previous_step(dst, steps.last()) else {
        return Ok(None);
    };

    for (method, next_step, steps_from_dst_to_next) in next_steps {
        let mut next_targets = vec![];
        match method {
            LspMethod::Nop => next_targets.push(next_step),
            LspMethod::ReverseDefinition { query, language } => {
                next_targets.extend(reverse_definition(root_dir, lsp_client, next_step));
            }
            LspMethod::Definition => {
                let definitions = lsp_client
                    .request::<GotoDefinition>(GotoDeclarationParams {
                        text_document_position_params: TextDocumentPositionParams::from(&next_step),
                        work_done_progress_params: WorkDoneProgressParams {
                            work_done_token: None,
                        },
                        partial_result_params: PartialResultParams {
                            partial_result_token: None,
                        },
                    })
                    .await??
                    .expect("failed to get definition");

                match definitions {
                    lsp_types::GotoDefinitionResponse::Scalar(location) => {
                        next_targets.push(location_to_step(location));
                    }
                    lsp_types::GotoDefinitionResponse::Array(locations) => {
                        next_targets.extend(locations.into_iter().map(location_to_step))
                    }
                    lsp_types::GotoDefinitionResponse::Link(_) => todo!("what is link?"),
                };
            }
            LspMethod::References => {
                let references = lsp_client
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
                    .await??
                    .expect("failed to get references");

                next_targets.extend(references.into_iter().map(location_to_step))
            }
        };
        dbg!(next_targets.len());

        let mut new_steps = steps.clone();
        new_steps.extend(steps_from_dst_to_next);

        for next_target in &next_targets {
            if !steps.contains(next_target) && next_target != dst {
                if let Some(next_step) = get_steps(
                    root_dir,
                    lsp_client,
                    language_provider,
                    sources,
                    next_target,
                    new_steps.clone(),
                )
                .await?
                {
                    return Ok(Some(next_step));
                }
            }
        }
    }

    Ok(None)
}

fn reverse_definition<C>(
    root_dir: &Path,
    lsp_client: &LspClient,
    next_step: Step<C>,
) -> Vec<Step<C>> {
    todo!()
}

fn get_query_locations<C>(
    root_dir: &Path,
    language: Language,
    query: &(Query, u32),
) -> Result<Vec<Step<C>>> {
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

impl<C> From<&Step<C>> for TextDocumentPositionParams {
    fn from(step: &Step<C>) -> Self {
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

pub fn location_to_step<C>(location: Location) -> Step<C> {
    let path = location
        .uri
        .to_file_path()
        .expect("failed to get uri file path");
    let start = location.range.start;
    let start = (start.line, start.character);
    let end = location.range.end;
    let end = (end.line, end.character);

    Step::new(path, start, end)
}
