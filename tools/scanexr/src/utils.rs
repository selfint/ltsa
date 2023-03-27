use std::fmt::Debug;
use std::fs::DirEntry;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Result;
use lsp_types::Location;
use tree_sitter;
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

pub fn get_query_steps<C>(
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

pub fn location_to_step<C>(location: Location) -> Step<C> {
    let path = location
        .uri
        .to_file_path()
        .expect("failed to get uri file path");
    let start = location.range.start;
    let end = location.range.end;

    Step::new(path, start, end)
}

pub fn get_tree<C>(step: &Step<C>) -> Tree {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(tree_sitter_solidity::language())
        .expect("failed to set language");

    let content = String::from_utf8(std::fs::read(&step.path).unwrap()).unwrap();

    parser
        .parse(&content, None)
        .expect("failed to parse content")
}

pub fn get_node<'a, C>(step: &Step<C>, root: Node<'a>) -> Node<'a> {
    root.descendant_for_point_range(step.start.into(), step.end.into())
        .expect("failed to get node at location range")
}

pub fn get_step_line<C>(step: &Step<C>) -> String {
    let content = String::from_utf8(std::fs::read(&step.path).unwrap()).unwrap();
    content.lines().nth(step.start.line).unwrap().to_string()
}

pub fn step_from_node<C>(path: PathBuf, node: Node) -> Step<C> {
    Step::new(path, node.start_position(), node.end_position())
}

pub fn debug_node_step<C: Debug>(node: &Node, parent: &Node, step: &Step<C>) {
    eprintln!(
        "\ngot step with:\nnode kind: {:?}\nparent: {:?}\ncontext: {:?}\nline:\n\n{}\n{}\n\n",
        node.kind(),
        parent.kind(),
        step.context,
        get_step_line(step),
        " ".repeat(node.start_position().column)
            + &"^".repeat(node.end_position().column - node.start_position().column)
    );
}
