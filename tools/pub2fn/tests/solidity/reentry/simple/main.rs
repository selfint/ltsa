use lsp_types::{notification::*, request::*, *};
use std::process::Stdio;
use std::{env, path::PathBuf};
use tokio::process::{Child, Command};

const ROOT_DIR: &str = "tests/solidity/reentry/simple";

fn start_solidity_ls() -> Child {
    Command::new("solidity-ls")
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start rust analyzer")
}

fn get_root_dir() -> PathBuf {
    env::current_dir().unwrap().join(ROOT_DIR)
}

#[tokio::test]
async fn test_load() {
    let mut child = start_solidity_ls();
    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (lsp_client, handles) = lsp_client::clients::stdio_client(stdin, stdout, stderr);

    let root_dir = get_root_dir();

    let init_resp = lsp_client
        .request::<Initialize>(InitializeParams {
            root_uri: Some(Url::from_file_path(&root_dir).unwrap()),
            ..Default::default()
        })
        .await?
        .result
        .as_result()
        .map_err(anyhow::Error::msg)?;
}
