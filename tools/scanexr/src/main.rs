use std::{path::PathBuf, process::Stdio};

use lsp_types::{notification::*, request::*, *};
use serde_json::{json, Value};
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

#[tokio::main]
async fn main() {
    let mut args = std::env::args();
    let _binary = args.next();
    let root_dir: PathBuf = args.next().unwrap().trim().into();
    let root_dir = root_dir.canonicalize().unwrap();
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
            [(member_expression
                object: (identifier) @obj
                (#match? @obj "msg")
                property: (identifier) @prop
                (#match? @prop "sender")
            ) @pub
            (contract_declaration
                body: (contract_body
                    (state_variable_declaration
                        name: (identifier) @pub
                    )
                )
            )
            ]
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

    let mut json_stacktraces: Vec<Value> = vec![];
    for stacktrace in stacktraces {
        let mut json_steps = vec![];
        for step in stacktrace {
            let json_step = json!({
                "path": step.path,
                "start": {
                    "line": step.start.line,
                    "character": step.start.character
                },
                "end": {
                    "line": step.end.line,
                    "character": step.end.character
                },
            });
            json_steps.push(json_step);
        }

        json_stacktraces.push(json! {
            {
                "steps": json_steps
            }
        });
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({ "stacktraces": json_stacktraces })).unwrap()
    );

    for handle in handles {
        handle.abort();
    }
}
