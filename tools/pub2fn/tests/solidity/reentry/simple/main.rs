use anyhow::Result;
use lsp_types::{notification::*, request::*, *};
use pub2fn::language_provider::solidity::SolidityLanguageProvider;
use std::env;
use std::path::Path;
use std::process::Stdio;
use tempfile::{tempdir, TempDir};
use tokio::process::{Child, Command};
use tree_sitter::Query;

fn start_solidity_ls() -> Child {
    Command::new("solidity-ls")
        .arg("--stdio")
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

#[tokio::test]
async fn test_solidity() {
    let root_dir = get_temp_dir();
    _test_solidity(root_dir.path())
        .await
        .expect("solidity test failed");
}

async fn _test_solidity(root_dir: &Path) -> Result<()> {
    let mut child = start_solidity_ls();
    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (lsp_client, handles) = lsp_client::clients::stdio_client(stdin, stdout, stderr);

    lsp_client
        .request::<Initialize>(InitializeParams {
            root_uri: Some(Url::from_file_path(root_dir).unwrap()),
            ..Default::default()
        })
        .await?
        .result
        .as_result()
        .map_err(anyhow::Error::msg)?;

    lsp_client.notify::<Initialized>(InitializedParams {})?;

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

    let steps = pub2fn::get_all_paths(
        root_dir,
        &[&lsp_client],
        tree_sitter_solidity::language(),
        pub_query,
        hacky_query,
        SolidityLanguageProvider,
    )
    .await?;

    let debug_steps = steps
        .into_iter()
        .map(|steps| {
            steps
                .into_iter()
                .rev()
                .map(|s| {
                    let path = s.path.file_name().unwrap().to_str().unwrap().to_string();
                    let source = String::from_utf8(std::fs::read(&s.path).unwrap()).unwrap();
                    let mut snippet = vec![];
                    let scroll = 5;
                    let start_line = s.start.0 - scroll.min(s.start.0);
                    let end_line = s.end.0 + scroll;
                    for (i, line) in source.lines().enumerate() {
                        if i < start_line as usize || i > end_line as usize {
                            continue;
                        }

                        snippet.push(line.to_string());
                        if i == s.start.0 as usize {
                            let mut pointer = " ".repeat(s.start.1 as usize)
                                + &"^".repeat(s.end.1 as usize - s.start.1 as usize);
                            if let Some(context) = &s.context {
                                pointer += &format!(" context: {:?}", context);
                            }
                            snippet.push(pointer);
                        }
                    }

                    let snippet = snippet.join("\n");
                    format!("# {path} #\n\n{snippet}")
                })
                .enumerate()
                .map(|(i, step_snippet)| format!("Step: {i}\n{step_snippet}"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .enumerate()
        .map(|(i, path_snippets)| format!("Path: {i}\n{path_snippets}"))
        .collect::<Vec<_>>()
        .join("\n");

    insta::assert_display_snapshot!(debug_steps,
        @r###"
    Path: 0
    Step: 0
    # contract.sol #


        function withdraw() public {
            uint bal = balances[msg.sender];
            require(bal > 0);

            (bool sent, ) = msg.sender.call{value: bal}("");
                                       ^^^^
            require(sent, "Failed to send Ether");

            balances[msg.sender] = 0;
        }

    Step: 1
    # contract.sol #


        function withdraw() public {
            uint bal = balances[msg.sender];
            require(bal > 0);

            (bool sent, ) = msg.sender.call{value: bal}("");
                            ^^^^^^^^^^
            require(sent, "Failed to send Ether");

            balances[msg.sender] = 0;
        }
    "###
    );

    for handle in handles {
        handle.abort();
    }

    Ok(())
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

    let results = pub2fn::get_query_results(&text, tree.root_node(), &pub_query.0, pub_query.1);
    let node = results[0];
    let node_text = node.utf8_text(text.as_bytes()).unwrap();
    insta::assert_snapshot!(node_text,
        @"msg.sender"
    );

    let results = pub2fn::get_query_results(&text, tree.root_node(), &hacky_query.0, hacky_query.1);
    let node = results[0];
    let node_text = node.utf8_text(text.as_bytes()).unwrap();
    insta::assert_snapshot!(node_text,
        @"call"
    );

    std::fs::remove_dir_all(temp_dir).unwrap();
}
