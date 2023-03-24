use anyhow::Result;
use lsp_types::{
    notification::Initialized, request::HoverRequest, request::Initialize, HoverParams,
    InitializeParams, InitializedParams, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, Url, WorkDoneProgressParams,
};
use std::{
    fs::DirEntry,
    path::{Path, PathBuf},
};

pub async fn get_steps(
    root_dir: &Path,
    fn_name: &str,
    lsp_client: lsp_client::client::Client,
) -> Result<Vec<(PathBuf, usize)>> {
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

    let possible_call_locations = get_string_locations(root_dir, fn_name)?;

    dbg!(&possible_call_locations);

    for (path, line, character) in possible_call_locations {
        todo!("detect using tree sitter if location is a function call, and extract param identifiers");
    }

    todo!()
}

fn get_string_locations(root_dir: &Path, string: &str) -> Result<Vec<(PathBuf, u32, u32)>> {
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

        for (line_number, line) in content.lines().enumerate() {
            for (col_number, _) in line.match_indices(string) {
                locations.push((dir.path(), line_number as u32, col_number as u32));
            }
        }
    })?;

    Ok(locations)
}
