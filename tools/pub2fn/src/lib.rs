use anyhow::Result;
use jsonrpc::types::JsonRpcResult;
use lsp_types::{
    notification::Initialized, request::Initialize, InitializeParams, InitializedParams,
};
use std::{
    convert::Into,
    path::{Path, PathBuf},
};

pub async fn get_steps(
    root_dir: &Path,
    fns: &[&str],
    lsp_client: lsp_client::client::Client,
) -> Result<Vec<(PathBuf, usize)>> {
    let init_resp = lsp_client
        .request::<Initialize>(InitializeParams::default())
        .await?
        .result
        .as_result()
        .map_err(anyhow::Error::msg)?;

    lsp_client
        .notify::<Initialized>(InitializedParams {})
        .unwrap();

    todo!()
}
