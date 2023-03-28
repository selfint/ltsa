use lsp_types::{notification::*, request::*, *};
use scanexr::{
    tracers::solidity::StepContext,
    utils::{get_node, get_step_line, get_tree},
};
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

#[test]
fn test_queries() {
    let temp_dir = get_temp_dir();

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(tree_sitter_solidity::language())
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

    let results = scanexr::utils::get_query_steps::<()>(
        temp_dir.path(),
        tree_sitter_solidity::language(),
        &pub_query,
    );
    let node_text = results
        .unwrap()
        .iter()
        .map(|step| {
            let tree = get_tree(step);
            let node = get_node(step, tree.root_node());
            get_step_line(step)
                + "\n"
                + &" ".repeat(node.start_position().column)
                + &"^".repeat(node.end_position().column - node.start_position().column)
        })
        .collect::<Vec<_>>()
        .join(",\n");

    insta::assert_snapshot!(node_text);

    let results = scanexr::utils::get_query_steps::<()>(
        temp_dir.path(),
        tree_sitter_solidity::language(),
        &hacky_query,
    );
    let node_text = results
        .unwrap()
        .iter()
        .map(|step| {
            let tree = get_tree(step);
            let node = get_node(step, tree.root_node());
            get_step_line(step)
                + "\n"
                + &" ".repeat(node.start_position().column)
                + &"^".repeat(node.end_position().column - node.start_position().column)
        })
        .collect::<Vec<_>>()
        .join(",\n");

    insta::assert_snapshot!(node_text);
}

#[tokio::test]
async fn test_solidity() {
    _test_solidity().await;
}

async fn _test_solidity() {
    let temp_dir = get_temp_dir();
    let root_dir = temp_dir.path().join("contract");
    let (lsp_client, handles) = lsp_client::clients::child_client(start_solidity_ls());

    lsp_client
        .request::<Initialize>(InitializeParams {
            root_uri: Some(Url::from_file_path(&root_dir.canonicalize().unwrap()).unwrap()),
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

    let stacktraces =
        scanexr::get_all_stacktraces(&tracer, &lsp_client, &root_dir, &[pub_query], &hacky_query)
            .await
            .unwrap();

    fn format_context(ctx: StepContext) -> StepContext {
        match ctx {
            StepContext::GetReturnValue(mut anchor) => {
                anchor.path = anchor.path.file_name().unwrap().into();
                anchor.context = format_context(anchor.context);
                StepContext::GetReturnValue(anchor)
            }
            StepContext::GetReturnTupleValue(mut anchor, index) => {
                anchor.path = anchor.path.file_name().unwrap().into();
                anchor.context = format_context(anchor.context);
                StepContext::GetReturnTupleValue(anchor, index)
            }
            _ => ctx,
        }
    }

    let debug_stacktraces = stacktraces
        .into_iter()
        .map(|steps| {
            steps
                .into_iter()
                .map(|s| {
                    let path = s.path.file_name().unwrap().to_str().unwrap().to_string();
                    let source = String::from_utf8(std::fs::read(&s.path).unwrap()).unwrap();
                    let mut snippet = vec![];
                    let scroll = 5;
                    let start_line = s.start.line - scroll.min(s.start.line);
                    let end_line = s.end.line + scroll;
                    for (i, line) in source.lines().enumerate() {
                        if i < start_line || i > end_line {
                            continue;
                        }

                        snippet.push(line.to_string());
                        if i == s.start.line {
                            let mut pointer = " ".repeat(s.start.character - 3)
                                + "// "
                                + &"^".repeat(s.end.character - s.start.character);
                            pointer +=
                                &format!(" context: {:?}", format_context(s.context.clone()));
                            snippet.push(pointer);
                        }
                    }

                    let snippet = snippet.join("\n");
                    format!("# {path} #\n\n{snippet}")
                })
                .enumerate()
                .map(|(i, step_snippet)| format!("Step: {i}\n{step_snippet}\n"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .enumerate()
        .map(|(i, path_snippets)| format!("Stacktrace: {i}\n{path_snippets}\n"))
        .collect::<Vec<_>>()
        .join("\n");

    insta::assert_snapshot!(debug_stacktraces);

    for handle in handles {
        handle.abort()
    }
}
