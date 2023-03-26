use lsp_types::{
    notification::Initialized, request::Initialize, InitializeParams, InitializedParams, Url,
};
use pub2fn::{get_query_results, LanguageProvider, LspMethod, Step};
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

def bar(v):
    return v

b = bar(a)
foo(1, b) # step:1
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
                .rev()
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
                    "",
                    "def bar(v):",
                    "    return v",
                    "",
                    "b = bar(a)",
                    "foo(1, b) # step:1",
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
                    "",
                    "def bar(v):",
                    "    return v",
                    "",
                    "b = bar(a)",
                    "^",
                    "foo(1, b) # step:1",
                ],
            ),
            (
                "main.py",
                [
                    "",
                    "from util import foo",
                    "",
                    "a = input() # step:0",
                    "",
                    "def bar(v):",
                    "    return v",
                    "",
                    "b = bar(a)",
                    "    ^ context: GetReturnValues",
                    "foo(1, b) # step:1",
                ],
            ),
            (
                "main.py",
                [
                    "",
                    "from util import foo",
                    "",
                    "a = input() # step:0",
                    "",
                    "def bar(v):",
                    "    return v",
                    "    ^ context: GetReturnValues",
                    "",
                    "b = bar(a)",
                    "foo(1, b) # step:1",
                ],
            ),
            (
                "main.py",
                [
                    "",
                    "from util import foo",
                    "",
                    "a = input() # step:0",
                    "",
                    "def bar(v):",
                    "    return v",
                    "           ^",
                    "",
                    "b = bar(a)",
                    "foo(1, b) # step:1",
                ],
            ),
            (
                "main.py",
                [
                    "",
                    "from util import foo",
                    "",
                    "a = input() # step:0",
                    "",
                    "def bar(v):",
                    "    return v",
                    "           ^",
                    "",
                    "b = bar(a)",
                    "foo(1, b) # step:1",
                ],
            ),
            (
                "main.py",
                [
                    "",
                    "from util import foo",
                    "",
                    "a = input() # step:0",
                    "",
                    "def bar(v):",
                    "        ^",
                    "    return v",
                    "",
                    "b = bar(a)",
                    "foo(1, b) # step:1",
                ],
            ),
            (
                "main.py",
                [
                    "",
                    "from util import foo",
                    "",
                    "a = input() # step:0",
                    "",
                    "def bar(v):",
                    "    ^ context: ParameterIndex(0)",
                    "    return v",
                    "",
                    "b = bar(a)",
                    "foo(1, b) # step:1",
                ],
            ),
            (
                "main.py",
                [
                    "",
                    "from util import foo",
                    "",
                    "a = input() # step:0",
                    "",
                    "def bar(v):",
                    "    ^ context: ParameterIndex(0)",
                    "    return v",
                    "",
                    "b = bar(a)",
                    "foo(1, b) # step:1",
                ],
            ),
            (
                "main.py",
                [
                    "",
                    "from util import foo",
                    "",
                    "a = input() # step:0",
                    "",
                    "def bar(v):",
                    "    return v",
                    "",
                    "b = bar(a)",
                    "        ^",
                    "foo(1, b) # step:1",
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
                    "",
                    "def bar(v):",
                    "    return v",
                    "",
                    "b = bar(a)",
                    "foo(1, b) # step:1",
                ],
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

fn step_from_node<C>(path: PathBuf, node: Node) -> Step<C> {
    let start = node.start_position();
    let end = node.end_position();

    let start = (start.row as u32, start.column as u32);
    let end = (end.row as u32, end.column as u32);

    Step::new(path, start, end)
}

struct PythonLanguageProvider;
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum StepContext {
    ParameterIndex(usize),
    GetReturnValues,
}

impl LanguageProvider for PythonLanguageProvider {
    type Context = StepContext;
    fn get_previous_step(
        &self,
        step: &Step<StepContext>,
        previous_step: Option<&Step<StepContext>>,
    ) -> Option<Vec<(pub2fn::LspMethod, Step<StepContext>, Vec<Step<StepContext>>)>> {
        let tree = get_tree(step);
        let node = get_node(step, tree.root_node());

        dbg!("-------------------------------");

        dbg!(
            (
                &previous_step.map(get_step_line),
                node.kind(),
                node.parent().unwrap().kind(),
                previous_step.and_then(|p| p.context.as_ref())
            ),
            (get_step_line(step), " ".repeat(step.start.1 as usize) + "^")
        );

        match (
            node.kind(),
            node.parent().map(|p| p.kind()),
            previous_step.and_then(|p| p.context.as_ref()),
        ) {
            ("identifier", Some("parameters"), _) => {
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

                Some(vec![(
                    LspMethod::References,
                    fn_step.clone(),
                    vec![param_step, fn_step],
                )])
            }
            ("identifier", Some("argument_list"), _) => {
                dbg!("got argument, finding where it is defined");
                Some(vec![(
                    LspMethod::Definition,
                    step.clone(),
                    vec![step.clone()],
                )])
            }
            ("identifier", Some("function_definition"), Some(StepContext::ParameterIndex(_))) => {
                let mut next_step = step.clone();
                next_step.context = previous_step
                    .expect("got fn def without previous step")
                    .context
                    .clone();

                dbg!("got function definition, finding where it is referenced");
                Some(vec![(
                    LspMethod::References,
                    next_step.clone(),
                    vec![next_step],
                )])
            }
            ("identifier", Some("function_definition"), Some(StepContext::GetReturnValues)) => {
                let parent = node.parent().unwrap();

                let source = std::fs::read(&step.path).unwrap();
                let text = parent.utf8_text(&source).unwrap();

                let query =
                    Query::new(tree_sitter_python::language(), "(return_statement) @return")
                        .unwrap();

                let return_values = get_query_results(text, parent, &query, 0);

                dbg!(&return_values);

                dbg!("got call, finding return values");
                Some(
                    return_values
                        .into_iter()
                        .map(|return_value| {
                            let mut next_step = step_from_node(step.path.clone(), return_value);
                            next_step.context = Some(StepContext::GetReturnValues);
                            (
                                LspMethod::Nop,
                                step_from_node(step.path.clone(), return_value),
                                vec![next_step],
                            )
                        })
                        .collect(),
                )
            }
            ("return_statement", _, Some(StepContext::GetReturnValues)) => {
                let mut cursor = tree.walk();
                let mut next_targets = vec![];
                for child in node.named_children(&mut cursor) {
                    let step = step_from_node(step.path.clone(), child);
                    next_targets.push((LspMethod::Nop, step.clone(), vec![step]));
                }

                dbg!("got return statement, checking returned values");
                if next_targets.is_empty() {
                    None
                } else {
                    Some(next_targets)
                }
            }
            ("identifier", Some("return_statement"), _) => {
                dbg!("got returned identifier, finding where it is defined");
                Some(vec![(
                    LspMethod::Definition,
                    step.clone(),
                    vec![step.clone()],
                )])
            }
            ("identifier", Some("call"), Some(StepContext::ParameterIndex(index))) => {
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
                let arg = args_list.named_children(&mut cursor).nth(*index).unwrap();
                let next_step = step_from_node(step.path.clone(), arg);

                dbg!("got call, finding where parameter passed to call is defined");
                Some(vec![(
                    LspMethod::Definition,
                    next_step.clone(),
                    vec![next_step],
                )])
            }
            ("identifier", Some("assignment"), _)
                if node.parent().unwrap().child_by_field_name("left").unwrap() == node =>
            {
                let next_node = node.parent().unwrap().child_by_field_name("right").unwrap();
                let next_step = step_from_node(step.path.clone(), next_node);

                dbg!("got assignment, next step is the assigned value");
                Some(vec![(LspMethod::Nop, next_step, vec![step.clone()])])
            }
            (rhs_kind, Some("assignment"), _)
                if node.parent().unwrap().child_by_field_name("right").unwrap() == node =>
            {
                dbg!(rhs_kind);
                match rhs_kind {
                    "call" => {
                        dbg!("got assigned to output of call, getting return value");
                        let mut next_step = step.clone();
                        next_step.context = Some(StepContext::GetReturnValues);
                        Some(vec![(
                            LspMethod::Definition,
                            next_step.clone(),
                            vec![next_step],
                        )])
                    }
                    other => todo!("got other: {:?}", other),
                }
            }
            _ => {
                eprintln!(
                    "unexpected node kind: {:?} / parent: {:?} / context: {:?}, line:\n\n{}\n\n",
                    node.kind(),
                    node.parent().map(|p| p.kind()),
                    step.context,
                    get_step_line(step)
                );
                dbg!("-------------------------------");

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

fn get_tree<C>(step: &Step<C>) -> Tree {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(tree_sitter_python::language())
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
