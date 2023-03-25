use lsp_types::{
    notification::Initialized, request::Initialize, InitializeParams, InitializedParams, Url,
};
use pub2fn::{get_query_results, LanguageProvider, LspMethod, Step, StepContext};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use tempfile::tempdir;
use tokio::process::{Child, Command};

use anyhow::Result;
use tree_sitter::{Node, Point, Query, Tree};

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
foo(1, a) # step:1
###end###
util.py @@@
def foo(dummy, val):
    if val != "token": # step:2
        return eval(val) # step:3
###end###
        "#;

    let (root_dir, expected_steps) = build_src(src).expect("failed to build src");
    dbg!(&root_dir);

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
                    let mut new_lines = vec![];
                    for (i, line) in source.lines().enumerate() {
                        new_lines.push(line.to_string());
                        if i == s.start.0 as usize {
                            let mut pointer = " ".repeat(s.start.1 as usize) + "^";
                            if let Some(context) = &s.context {
                                pointer += &format!(" context: {:?}", context);
                            }
                            new_lines.push(pointer);
                        }
                    }

                    (path, new_lines)
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
                [
                    "",
                    "def foo(dummy, val):",
                    "    if val != \"token\": # step:2",
                    "        return eval(val) # step:3",
                    "                    ^",
                ],
            ),
            (
                "util.py",
                [
                    "",
                    "def foo(dummy, val):",
                    "               ^",
                    "    if val != \"token\": # step:2",
                    "        return eval(val) # step:3",
                ],
            ),
            (
                "util.py",
                [
                    "",
                    "def foo(dummy, val):",
                    "    ^ context: ParameterIndex(1)",
                    "    if val != \"token\": # step:2",
                    "        return eval(val) # step:3",
                ],
            ),
            (
                "main.py",
                [
                    "",
                    "from util import foo",
                    "",
                    "a = input() # step:0",
                    "foo(1, a) # step:1",
                    "       ^",
                ],
            ),
            (
                "main.py",
                [
                    "",
                    "from util import foo",
                    "",
                    "a = input() # step:0",
                    "^",
                    "foo(1, a) # step:1",
                ],
            ),
            (
                "main.py",
                [
                    "",
                    "from util import foo",
                    "",
                    "a = input() # step:0",
                    "    ^",
                    "foo(1, a) # step:1",
                ],
            ),
        ],
    ]
    "###
    );

    for handle in handles {
        handle.abort();
    }

    // fs::remove_dir_all(root_dir).expect("failed to delete src");

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
    fn get_next_step(
        &self,
        step: &Step,
        previous_step: Option<&Step>,
    ) -> Option<(pub2fn::LspMethod, Step, Vec<Step>)> {
        let tree = get_tree(step);
        let node = get_node(step, tree.root_node());

        dbg!("-------------------------------");

        dbg!(
            (
                &previous_step.map(get_step_line),
                node.kind(),
                node.parent().unwrap().kind()
            ),
            (get_step_line(step), " ".repeat(step.start.1 as usize) + "^")
        );

        match (node.kind(), node.parent().map(|p| p.kind())) {
            ("identifier", Some("parameters")) => {
                let arg_list = node.parent().unwrap();
                let fn_def = arg_list.parent().unwrap();
                let fn_name = fn_def.child_by_field_name("name").unwrap();

                let mut cursor = tree.walk();
                let index = arg_list
                    .named_children(&mut cursor)
                    .position(|arg| arg == node)
                    .expect("failed to find parameter index");

                let param_step = step.clone();
                let mut fn_step = step_from_node(step.path.clone(), fn_name);
                fn_step.context = Some(StepContext::ParameterIndex(index));

                dbg!("got parameter, finding references to containing method (added parameter index context)");

                Some((
                    LspMethod::References,
                    fn_step.clone(),
                    vec![param_step, fn_step],
                ))
            }
            ("identifier", Some("argument_list")) => {
                dbg!("got argument, finding where it is defined");
                Some((LspMethod::Definition, step.clone(), vec![step.clone()]))
            }
            ("identifier", Some("function_definition")) => {
                let mut next_step = step.clone();
                next_step.context = previous_step
                    .expect("got fn def without previous step")
                    .context
                    .clone();

                dbg!("got function definition, finding where it is referenced");
                Some((LspMethod::References, next_step.clone(), vec![next_step]))
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

                let Some(StepContext::ParameterIndex(index)) = previous_step
                    .expect("got call without previous step")
                    .context else {
                        panic!("invalid context");
                    };

                let arg = args_list.named_children(&mut cursor).nth(index).unwrap();
                let next_step = step_from_node(step.path.clone(), arg);

                dbg!("got call, finding where parameter passed to call is defined");
                Some((LspMethod::Definition, next_step.clone(), vec![next_step]))
            }
            ("identifier", Some("assignment")) => {
                let parent = node.parent().unwrap();

                // this step is being assigned a value
                if parent.child_by_field_name("left").unwrap() == node {
                    let next_node = parent.child_by_field_name("right").unwrap();
                    let next_step = step_from_node(step.path.clone(), next_node);

                    dbg!("got assignment, next step is the assigned value");
                    Some((
                        LspMethod::Nop,
                        next_step.clone(),
                        vec![step.clone(), next_step],
                    ))
                }
                // a value is being assigned as this step
                else {
                    let next_node = parent.child_by_field_name("left").unwrap();
                    let next_step = step_from_node(step.path.clone(), next_node);

                    dbg!("got aliased, finding references of new alias");
                    Some((
                        LspMethod::References,
                        next_step.clone(),
                        vec![step.clone(), next_step],
                    ))
                }
            }
            _ => {
                eprintln!(
                    "unexpected node kind: {:?} / parent: {:?}, line:\n\n{}\n\n",
                    node.kind(),
                    node.parent(),
                    get_step_line(step)
                );
                dbg!("-------------------------------");

                None
            }
        }
    }
}

fn get_step_line(step: &Step) -> String {
    let content = String::from_utf8(std::fs::read(&step.path).unwrap()).unwrap();
    let line = step.start.0;
    content.lines().nth(line as usize).unwrap().to_string()
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
