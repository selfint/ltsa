use std::fs::DirEntry;
use std::path::Path;

use lsp_types::Location;
use lsp_types::Url;
use tree_sitter::Node;
use tree_sitter::Point;
use tree_sitter::Query;
use tree_sitter::QueryCursor;

use crate::converter::{Convert, Converter};

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

pub fn visit_dirs(dir: &Path, cb: &mut impl FnMut(&DirEntry)) -> std::io::Result<()> {
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

pub fn get_node_location(uri: Url, node: &Node) -> Location {
    Location {
        uri,
        range: lsp_types::Range {
            start: Converter::convert(node.start_position()),
            end: Converter::convert(node.end_position()),
        },
    }
}

pub fn get_location_node<'a>(root: Node<'a>, location: &Location) -> Option<Node<'a>> {
    let start = Point {
        row: location.range.start.line as usize,
        column: location.range.start.character as usize,
    };
    let end = Point {
        row: location.range.end.line as usize,
        column: location.range.end.character as usize,
    };

    root.named_descendant_for_point_range(start, end)
}

pub fn get_breadcrumbs<'a>(root: Node<'a>, location: &Location) -> Option<Vec<Node<'a>>> {
    let mut node = get_location_node(root, location)?;

    let mut breadcrumbs = vec![];
    while let Some(parent_node) = node.parent() {
        breadcrumbs.push(node);
        node = parent_node;
    }

    Some(breadcrumbs)
}
