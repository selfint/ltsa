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
    let contract = include_str!("contract.sol");

    let temp_dir = tempdir().expect("failed to create tempdir");
    std::fs::write(temp_dir.path().join("contract.sol"), contract)
        .expect("failed to copy contract");

    temp_dir
}

#[test]
fn test_queries() {
    let temp_dir = get_temp_dir();
    let path = temp_dir.path().join("contract.sol");
    let text = String::from_utf8(std::fs::read(path).unwrap()).unwrap();

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(tree_sitter_solidity::language())
        .unwrap();

    let tree = parser.parse(&text, None).unwrap();

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

    let results =
        scanexr::utils::get_query_results(&text, tree.root_node(), &pub_query.0, pub_query.1);
    let node_text = results
        .iter()
        .map(|node| {
            (
                node.parent().unwrap().utf8_text(text.as_bytes()).unwrap(),
                node.utf8_text(text.as_bytes()).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    insta::assert_debug_snapshot!(node_text,
        @r###"
    [
        (
            "balances[msg.sender]",
            "msg.sender",
        ),
        (
            "return msg.sender;",
            "msg.sender",
        ),
    ]
    "###
    );

    let results =
        scanexr::utils::get_query_results(&text, tree.root_node(), &hacky_query.0, hacky_query.1);
    let node_text = results
        .iter()
        .map(|node| {
            (
                node.parent().unwrap().utf8_text(text.as_bytes()).unwrap(),
                node.utf8_text(text.as_bytes()).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    insta::assert_debug_snapshot!(node_text,
        @r###"
    [
        (
            "bar.call",
            "call",
        ),
    ]
    "###
    );
}

#[tokio::test]
async fn test_solidity() {
    _test_solidity().await;
}

async fn _test_solidity() {
    let root_dir = get_temp_dir();
    let (lsp_client, handles) = lsp_client::clients::child_client(start_solidity_ls());

    lsp_client
        .request::<Initialize>(InitializeParams {
            root_uri: Some(Url::from_file_path(root_dir.path()).unwrap()),
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

    let tracer = scanexr::tracers::solidity::SolidityTracer;

    let stacktraces = scanexr::get_all_stacktraces(
        &tracer,
        &lsp_client,
        root_dir.path(),
        &[pub_query],
        &hacky_query,
    )
    .await
    .unwrap();

    for handle in handles {
        handle.abort()
    }
}
