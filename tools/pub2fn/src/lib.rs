use anyhow::Result;
use lsp_client::client::Client as LspClient;
use lsp_types::{
    notification::Initialized, request::Initialize, InitializeParams, InitializedParams,
};
use std::{
    fs::DirEntry,
    path::{Path, PathBuf},
};
use tree_sitter::{Language, Node, Query, QueryCursor};

pub async fn get_steps(
    root_dir: &Path,
    lsp_client: &LspClient,
    language: Language,
    pub_query: (Query, u32),
    hacky_query: (Query, u32),
) -> Result<Vec<Vec<(PathBuf, u32)>>> {
    let init_resp = lsp_client
        .request::<Initialize>(InitializeParams::default())
        .await?
        .result
        .as_result()
        .map_err(anyhow::Error::msg)?;

    if init_resp.capabilities.references_provider.is_none() {
        anyhow::bail!("lsp has no reference provider");
    }

    lsp_client
        .notify::<Initialized>(InitializedParams {})
        .unwrap();

    let pub_locations = get_query_locations(root_dir, language, &pub_query)?;
    let hacky_locations = get_query_locations(root_dir, language, &hacky_query)?;

    let mut paths = vec![];
    for pub_location in &pub_locations {
        for hacky_location in &hacky_locations {
            if let Some(path) =
                get_path(root_dir, lsp_client, language, hacky_location, pub_location)
            {
                paths.push(path);
            }
        }
    }

    Ok(paths)
}

fn get_path(
    root_dir: &Path,
    lsp_client: &LspClient,
    language: Language,
    hacky_location: &(PathBuf, u32, u32),
    pub_location: &(PathBuf, u32, u32),
) -> Option<Vec<(PathBuf, u32)>> {
    todo!()
}

fn get_query_locations(
    root_dir: &Path,
    language: Language,
    query: &(Query, u32),
) -> Result<Vec<(PathBuf, u32, u32)>> {
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
            locations.push((
                dir.path(),
                result.start_position().row as u32,
                result.start_position().column as u32,
            ));
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
                dbg!(&text[capture.node.byte_range()]);
                nodes.push(capture.node);
            }
        }
    }

    nodes
}
