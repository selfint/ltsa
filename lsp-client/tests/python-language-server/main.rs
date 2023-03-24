use lsp_client::clients;
use lsp_types::{
    notification::Initialized, request::Initialize, InitializeParams, InitializedParams,
};
use std::process::Stdio;
use tokio::process::{Child, Command};

fn start_server() -> Child {
    Command::new("pyls")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start rust analyzer")
}

#[tokio::test]
async fn test_python_language_server() {
    let mut child = start_server();

    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (client, handles) = clients::stdio_client(stdin, stdout, stderr);

    let init_resp = client
        .request::<Initialize>(InitializeParams::default())
        .await;

    insta::assert_debug_snapshot!(init_resp);

    client.notify::<Initialized>(InitializedParams {}).unwrap();

    for handle in handles {
        handle.abort();
    }
}
