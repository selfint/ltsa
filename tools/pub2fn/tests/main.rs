use lsp_types::{
    notification::Initialized, request::Initialize, InitializeParams, InitializedParams, Location,
    Position, TextDocumentIdentifier, TextDocumentPositionParams, Url,
};
use pub2fn::{get_query_results, LanguageProvider, LspMethod, Step};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use tempfile::tempdir;
use tokio::process::{Child, Command};

use anyhow::Result;
use tree_sitter::{Node, Point, Query, QueryCursor, Tree, TreeCursor};

fn build_src(src: &str) -> Result<(PathBuf, Vec<(PathBuf, usize)>)> {
    struct File {
        path: PathBuf,
        content: String,
        steps: HashMap<usize, usize>,
    }

    let files = src
        .trim()
        .split("###end###")
        .filter(|&f| !f.is_empty())
        .map(|f| {
            let binding = f.split("@@@").collect::<Vec<_>>();
            let [path, content] = binding.as_slice() else {
                panic!("failed to parse file:\n{:?}\n", f);
            };

            let steps = content
                .lines()
                .enumerate()
                .filter_map(|(line_number, line)| {
                    let parts = line.split("# step:").collect::<Vec<_>>();
                    if parts.len() == 2 {
                        let step_n = parts[1].parse().unwrap_or_else(|_| {
                            panic!("failed to parse step number: {:?}", parts[1])
                        });

                        Some((step_n, line_number))
                    } else {
                        None
                    }
                })
                .collect();

            File {
                path: path.trim().into(),
                content: content.to_string(),
                steps,
            }
        });

    let root_dir = tempdir().expect("failed to create tempdir").into_path();

    let mut steps = HashMap::new();
    for src_file in files {
        if let Some(src_parent) = src_file.path.parent() {
            fs::create_dir_all(root_dir.join(src_parent)).expect("failed to create parent dir")
        }

        steps.extend(
            src_file
                .steps
                .into_iter()
                .map(|(step_number, line_number)| {
                    (step_number, (src_file.path.clone(), line_number))
                }),
        );
        fs::write(root_dir.join(src_file.path), src_file.content)
            .expect("failed to write src file");
    }

    let mut new_steps = vec![None; steps.len()];
    for (step_n, step) in steps {
        new_steps[step_n] = Some(step);
    }

    let steps = new_steps.into_iter().map(|s| s.unwrap()).collect();

    Ok((root_dir, steps))
}

fn start_python_language_server() -> Child {
    Command::new("pyls")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start rust analyzer")
}

#[tokio::test]
async fn test_python() {
    _test_python().await.unwrap();
}

async fn _test_python() -> Result<()> {
    let src = r#"
main.py @@@
from util import foo

a = input() # step:0
foo(a) # step:1
###end###
util.py @@@
def foo(val):
    if val != "token": # step:2
        return eval(val) # step:3
###end###
        "#;

    let (root_dir, expected_steps) = build_src(src).expect("failed to build src");

    let mut child = start_python_language_server();
    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (lsp_client, handles) = lsp_client::clients::stdio_client(stdin, stdout, stderr);

    let pub_query = (
        Query::new(
            tree_sitter_python::language(),
            r#"
        (call
            function: (identifier) @ident
            (#match? @ident "input")
        ) @pub"#,
        )
        .unwrap(),
        1,
    );

    let hacky_query = (
        Query::new(
            tree_sitter_python::language(),
            r#"
        (call
            function: (identifier) @fn
            (#match? @fn "eval")
            arguments: (argument_list (identifier) @hacky)
        )"#,
        )
        .unwrap(),
        1,
    );

    let init_resp = lsp_client
        .request::<Initialize>(InitializeParams {
            root_uri: Some(Url::from_file_path(&root_dir).unwrap()),
            ..Default::default()
        })
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

    let actual_steps = pub2fn::get_all_paths(
        root_dir.as_path(),
        &lsp_client,
        tree_sitter_python::language(),
        pub_query,
        hacky_query,
        PythonLanguageProvider,
    )
    .await
    .expect("failed to get steps");

    let debug_steps = actual_steps
        .into_iter()
        .map(|steps| {
            steps
                .into_iter()
                .map(|s| {
                    let path = s.path.file_name().unwrap().to_str().unwrap().to_string();
                    let source = String::from_utf8(std::fs::read(&s.path).unwrap()).unwrap();
                    let line = source.lines().nth(s.start.0 as usize).unwrap().to_string();
                    let pointer = " ".repeat(s.start.1 as usize) + "^";

                    (path, line, pointer)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    insta::assert_debug_snapshot!(debug_steps,
        @r###"
    [
        [
            (
                "util.py",
                "        return eval(val) # step:3",
                "                    ^",
            ),
            (
                "util.py",
                "def foo(val):",
                "        ^",
            ),
            (
                "main.py",
                "foo(a) # step:1",
                "^",
            ),
            (
                "main.py",
                "a = input() # step:0",
                "^",
            ),
            (
                "main.py",
                "a = input() # step:0",
                "    ^",
            ),
        ],
    ]
    "###
    );

    for handle in handles {
        handle.abort();
    }

    fs::remove_dir_all(root_dir).expect("failed to delete src");

    Ok(())
}

fn step_from_node(path: PathBuf, node: Node) -> Step {
    let start = node.start_position();
    let end = node.end_position();

    let start = (start.row as u32, start.column as u32);
    let end = (end.row as u32, end.column as u32);

    Step::new(path, start, end)
}

struct PythonLanguageProvider;
impl LanguageProvider for PythonLanguageProvider {
    fn get_next_step(&self, step: &Step) -> Option<(pub2fn::LspMethod, Step)> {
        let tree = get_tree(step);
        let node = get_node(step, tree.root_node());

        match (node.kind(), node.parent().map(|p| p.kind())) {
            ("call", _) => todo!("get references"),
            ("identifier", None) => todo!("got identifier without parent, global?"),
            ("identifier", Some("parameters")) => {
                let arg_list = node.parent().unwrap();
                let fn_def = arg_list.parent().unwrap();
                let fn_name = fn_def.child_by_field_name("name").unwrap();
                let next_step = step_from_node(step.path.clone(), fn_name);

                Some((LspMethod::References, next_step))
            }
            ("identifier", Some("argument_list")) => Some((LspMethod::Definition, step.clone())),
            ("identifier", Some("function_definition")) => {
                Some((LspMethod::References, step.clone()))
            }
            ("identifier", Some("call")) => {
                let parent = node.parent().unwrap();

                let source = std::fs::read(&step.path).unwrap();
                let text = parent.utf8_text(&source).unwrap();

                let query = Query::new(
                    tree_sitter_python::language(),
                    "(call arguments: (argument_list) @args)",
                )
                .unwrap();

                let args_list = get_query_results(text, parent, &query, 0)[0];

                let mut cursor = tree.walk();

                // todo keep track of arg correctly
                let arg = args_list.named_children(&mut cursor).next().unwrap();

                let next_step = step_from_node(step.path.clone(), arg);

                Some((LspMethod::Definition, next_step))
            }
            ("identifier", Some("assignment")) => {
                let parent = node.parent().unwrap();
                // this step is being assigned a value
                if parent.child_by_field_name("left").unwrap() == node {
                    let next_node = parent.child_by_field_name("right").unwrap();
                    let next_step = step_from_node(step.path.clone(), next_node);

                    Some((LspMethod::Nop, next_step))
                }
                // a value is being assigned as this step
                else {
                    let next_node = parent.child_by_field_name("left").unwrap();
                    let next_step = step_from_node(step.path.clone(), next_node);

                    Some((LspMethod::References, next_step))
                }
            }
            _ => {
                eprintln!(
                    "unexpected node kind: {:?} / parent: {:?}, content:\n\n{}\n\n",
                    node.kind(),
                    node.parent(),
                    {
                        let content =
                            String::from_utf8(std::fs::read(&step.path).unwrap()).unwrap();
                        let line = step.start.0;
                        content.lines().nth(line as usize).unwrap().to_string()
                    }
                );

                None
            }
        }
    }

    fn get_definition_parents(&self, response: lsp_types::GotoDefinitionResponse) -> Vec<Step> {
        match response {
            lsp_types::GotoDefinitionResponse::Scalar(location) => {
                vec![location_to_step(location)]
            }
            lsp_types::GotoDefinitionResponse::Array(locations) => {
                locations.into_iter().map(location_to_step).collect()
            }
            lsp_types::GotoDefinitionResponse::Link(_) => todo!("what is link?"),
        }
    }

    fn get_references_parents(&self, response: Vec<lsp_types::Location>) -> Vec<Step> {
        response.into_iter().map(location_to_step).collect()
    }
}

fn location_to_step(location: Location) -> Step {
    let path = location
        .uri
        .to_file_path()
        .expect("failed to get uri file path");
    let start = location.range.start;
    let start = (start.line, start.character);
    let end = location.range.end;
    let end = (end.line, end.character);

    Step::new(path, start, end)
}

fn get_tree(step: &Step) -> Tree {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(tree_sitter_python::language())
        .expect("failed to set language");

    let content = String::from_utf8(std::fs::read(&step.path).unwrap()).unwrap();

    parser
        .parse(&content, None)
        .expect("failed to parse content")
}

fn get_node<'a>(step: &Step, root: Node<'a>) -> Node<'a> {
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

// let dst_parents = match dst.kind {
//     StepKind::Variable => {
//         let parent = lsp_client
//             .request::<GotoDefinition>(GotoDefinitionParams {
//                 text_document_position_params: dst.text_document_position_params.clone(),
//                 work_done_progress_params: WorkDoneProgressParams {
//                     work_done_token: None,
//                 },
//                 partial_result_params: PartialResultParams {
//                     partial_result_token: None,
//                 },
//             })
//             .await?
//             .result
//             .as_result()
//             .map_err(anyhow::Error::msg)?;

//         match parent {
//             Some(parent) => match parent {
//                 GotoDefinitionResponse::Scalar(location) => {
//                     let step = Step::from_location(location, kind_resolver_fn);
//                     vec![step]
//                 }
//                 GotoDefinitionResponse::Array(locations) => locations
//                     .into_iter()
//                     .map(|location| Step::from_location(location, kind_resolver_fn))
//                     .collect(),
//                 GotoDefinitionResponse::Link(_) => todo!("what is link?"),
//             },
//             None => return Ok(None),
//         }
//     }
//     StepKind::Parameter => todo!("got parameter"),
//     StepKind::Function => todo!(),
// };
