use lsp_types::{notification::*, request::*, *};
use std::process::Stdio;
use tempfile::{tempdir, TempDir};
use tokio::process::{Child, Command};
use tree_sitter::Query;

fn start_solidity_ls() -> Child {
    Command::new("solc")
        .arg("--lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start solidity ls")
}

fn get_temp_dir() -> TempDir {
    let contract = include_str!("contract/contract.sol");
    let other_file = include_str!("contract/other_file.sol");

    let temp_dir = tempdir().expect("failed to create tempdir");
    std::fs::create_dir(temp_dir.path().join("contract")).expect("failed to create dir");

    std::fs::write(
        temp_dir.path().join("contract").join("contract.sol"),
        contract,
    )
    .expect("failed to copy contract");
    std::fs::write(
        temp_dir.path().join("contract").join("other_file.sol"),
        other_file,
    )
    .expect("failed to copy contract");

    temp_dir
}

#[tokio::test]
async fn test_solidity() {
    _test_solidity().await;
}

async fn _test_solidity() {
    let temp_dir = get_temp_dir();
    let root_dir = temp_dir.path().join("contract").canonicalize().unwrap();
    let (lsp_client, handles) = lsp_client::clients::child_client(start_solidity_ls());

    lsp_client
        .request::<Initialize>(InitializeParams {
            root_uri: Some(Url::from_file_path(&root_dir).unwrap()),
            ..Default::default()
        })
        .await
        .unwrap()
        .unwrap();

    lsp_client
        .notify::<Initialized>(InitializedParams {})
        .unwrap();

    let pub_query = (
        Query::new(
            tree_sitter_solidity::language(),
            r#"
            (member_expression
                object: (identifier) @obj
                (#match? @obj "msg")
                property: (identifier) @prop
                (#match? @prop "sender")
            ) @pub
            "#,
        )
        .unwrap(),
        2,
    );

    let hacky_query = (
        Query::new(
            tree_sitter_solidity::language(),
            r#"
        (call_expression
            function: (struct_expression
                type: (member_expression
                    property: (identifier) @hacky
                    (#match? @hacky "call")
                )
            )
        )
        "#,
        )
        .unwrap(),
        0,
    );

    for handle in handles {
        handle.abort()
    }
}
