use std::fmt::Debug;
use std::fs::DirEntry;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use lsp_client::client::Client;
use lsp_types::request::GotoDeclarationParams;
use lsp_types::request::GotoDefinition;
use lsp_types::GotoDefinitionResponse;
use lsp_types::Location;
use lsp_types::PartialResultParams;
use lsp_types::TextDocumentPositionParams;
use lsp_types::Url;
use lsp_types::WorkDoneProgressParams;
use tree_sitter::Language;
use tree_sitter::Node;
use tree_sitter::Query;
use tree_sitter::QueryCursor;
use tree_sitter::Tree;

use crate::Step;

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

pub fn get_query_steps<C: Default>(
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
            let end = result.end_position();
            locations.push(Step::new(dir.path(), start, end));
        }
    })?;

    Ok(locations)
}

pub fn location_to_step<C: Default>(location: Location) -> Result<Step<C>> {
    let path = location
        .uri
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("failed to convert location uri to file path"))?;
    let start = location.range.start;
    let end = location.range.end;

    Ok(Step::new(path, start, end))
}

pub fn get_tree<C: Default>(step: &Step<C>) -> Tree {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(tree_sitter_solidity::language())
        .expect("failed to set language");

    let content = String::from_utf8(std::fs::read(&step.path).unwrap()).unwrap();

    parser
        .parse(&content, None)
        .expect("failed to parse content")
}

pub fn get_node<'a, C: Default>(step: &Step<C>, root: Node<'a>) -> Node<'a> {
    root.descendant_for_point_range(step.start.into(), step.end.into())
        .expect("failed to get node at location range")
}

pub fn get_step_line<C: Default>(step: &Step<C>) -> String {
    let content = String::from_utf8(std::fs::read(&step.path).unwrap()).unwrap();
    content.lines().nth(step.start.line).unwrap().to_string()
}

pub fn step_from_node<C: Default>(path: PathBuf, node: Node) -> Step<C> {
    Step::from((path, node))
}

pub fn debug_node_step<C: Debug + Default>(node: &Node, parent: &Node, step: &Step<C>) {
    eprintln!("{}", format_node_step(node, parent, step));
}

pub fn format_node_step<C: Debug + Default>(node: &Node, parent: &Node, step: &Step<C>) -> String {
    format!(
        r#"
got step with:
path: {:?}
node kind: {:?}
parent: {:?}
context: {:?}
line:{}
content:

{}
{}

"#,
        step.path.as_path(),
        node.kind(),
        parent.kind(),
        step.context,
        step.start.line,
        get_step_line(step),
        " ".repeat(node.start_position().column)
            + &"^".repeat(node.end_position().column - node.start_position().column)
    )
}

pub async fn get_step_definitions<C: Default>(
    lsp_client: &Client,
    step: &Step<C>,
) -> Result<Vec<Step<C>>> {
    let Some(definitions) = lsp_client
        .request::<GotoDefinition>(GotoDeclarationParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier {
                    uri: Url::from_file_path(&step.path)
                        .map_err(|_| anyhow::Error::msg("failed to convert step path to url"))?,
                },
                position: step.start.into(),
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
        .context("getting goto definition result")?
        else { return Ok(vec![]) };

    Ok(match definitions {
        GotoDefinitionResponse::Scalar(definition) => vec![location_to_step(definition)?],
        GotoDefinitionResponse::Array(definitions) => definitions
            .into_iter()
            .map(location_to_step)
            .collect::<Result<Vec<_>>>()?,
        GotoDefinitionResponse::Link(_) => todo!("what is link?"),
    })
}
