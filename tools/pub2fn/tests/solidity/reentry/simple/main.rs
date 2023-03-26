use anyhow::Result;
use lsp_types::{notification::*, request::*, *};
use pub2fn::{LanguageProvider, LspMethod, Step};
use std::process::Stdio;
use std::{env, path::PathBuf};
use tokio::process::{Child, Command};
use tree_sitter::{Node, Point, Query, Tree};

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

fn get_tree<C>(step: &Step<C>) -> Tree {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(tree_sitter_solidity::language())
        .expect("failed to set language");

    let content = String::from_utf8(std::fs::read(&step.path).unwrap()).unwrap();

    parser
        .parse(&content, None)
        .expect("failed to parse content")
}

fn get_node<'a, C>(step: &Step<C>, root: Node<'a>) -> Node<'a> {
    root.descendant_for_point_range(
        Point {
            row: step.start.0 as usize,
            column: step.start.1 as usize,
        },
        Point {
            row: step.end.0 as usize,
            column: step.end.1 as usize,
        },
    )
    .expect("failed to get node at location range")
}

struct SolidityLanguageProvider;
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum StepContext {}

impl LanguageProvider for SolidityLanguageProvider {
    type Context = StepContext;

    fn get_previous_step(
        &self,
        step: &pub2fn::Step<Self::Context>,
        previous_step: Option<&pub2fn::Step<Self::Context>>,
    ) -> Option<
        Vec<(
            pub2fn::LspMethod,
            pub2fn::Step<Self::Context>,
            Vec<pub2fn::Step<Self::Context>>,
        )>,
    > {
        let tree = get_tree(step);
        let node = get_node(step, tree.root_node());
        let parent = node.parent().unwrap();

        eprintln!(
            "got step with node kind: {:?} / parent: {:?} / context: {:?}, line:\n\n{}\n{}\n",
            node.kind(),
            parent.kind(),
            step.context,
            get_step_line(step),
            " ".repeat(node.start_position().column)
                + &"^".repeat(node.end_position().column - node.start_position().column)
        );

        match (
            node.kind(),
            parent.kind(),
            previous_step.and_then(|p| p.context.as_ref()),
        ) {
            ("identifier", "member_expression", None) => {
                dbg!(parent.to_sexp());
                // if we are a property
                if parent.child_by_field_name("property") == Some(node) {
                    dbg!("got property, next step is object");

                    // get object definition
                    let object = parent
                        .child_by_field_name("object")
                        .expect("got member expression with property but without object");
                    let next_step = step_from_node(step.path.clone(), object);

                    Some(vec![(
                        LspMethod::Nop,
                        next_step.clone(),
                        vec![step.clone(), next_step],
                    )])
                } else {
                    todo!("we are object")
                }
            }
            _ => {
                // todo!()
                None
            }
        }
    }
}

fn get_step_line<C>(step: &Step<C>) -> String {
    let content = String::from_utf8(std::fs::read(&step.path).unwrap()).unwrap();
    let line = step.start.0;
    content.lines().nth(line as usize).unwrap().to_string()
}

#[tokio::test]
async fn test_solidity() {
    _test_solidity().await.expect("solidity test failed");
}

async fn _test_solidity() -> Result<()> {
    let mut child = start_solidity_ls();
    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (lsp_client, handles) = lsp_client::clients::stdio_client(stdin, stdout, stderr);

    let root_dir = get_root_dir();

    lsp_client
        .request::<Initialize>(InitializeParams {
            root_uri: Some(Url::from_file_path(&root_dir).unwrap()),
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
        &root_dir,
        &lsp_client,
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

            uint bal = balances[msg.sender];
            require(bal > 0);

            // address sender = getSender();

            (bool sent, ) = msg.sender.call{value: bal}("");
                                       ^^^^
            require(sent, "Failed to send Ether");

            balances[msg.sender] = 0;
        }

    Step: 1
    # contract.sol #

            uint bal = balances[msg.sender];
            require(bal > 0);

            // address sender = getSender();

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
    let text =
        String::from_utf8(std::fs::read(get_root_dir().join("contract.sol")).unwrap()).unwrap();

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
}

fn step_from_node<C>(path: PathBuf, node: Node) -> Step<C> {
    let start = node.start_position();
    let end = node.end_position();

    let start = (start.row as u32, start.column as u32);
    let end = (end.row as u32, end.column as u32);

    Step::new(path, start, end)
}
