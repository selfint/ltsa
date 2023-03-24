use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use tempfile::tempdir;
use tokio::process::{Child, Command};

use anyhow::Result;

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
        fs::write(src_file.path, src_file.content).expect("failed to write src file");
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
    let (client, handles) = lsp_client::clients::stdio_client(stdin, stdout, stderr);

    let actual_steps = pub2fn::get_steps(root_dir.as_path(), &["eval"], client)
        .await
        .expect("failed to get steps");

    assert_eq!(expected_steps, actual_steps);

    for handle in handles {
        handle.abort();
    }

    fs::remove_dir_all(root_dir).expect("failed to delete src");
}
