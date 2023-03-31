use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use lsp_types::Location;
use serde_json::{json, Value};
use tree_sitter::{Parser, Query, Tree};

use scanexr::{
    converter::{Convert, Converter},
    language_provider::{self, LanguageAutomata, LspProvider, SupportedLanguage},
    languages::solidity::{Solidity, SolidityLs},
    utils::{get_node_location, get_query_results, visit_dirs},
};

fn parse_file(path: &Path) -> Result<(String, Tree)> {
    let text = String::from_utf8(std::fs::read(path)?)?;

    let mut parser = Parser::new();
    parser.set_language(tree_sitter_solidity::language())?;

    let tree = parser
        .parse(&text, None)
        .ok_or_else(|| anyhow!("failed to parse text"))?;

    Ok((text, tree))
}

fn get_start_end(project_files: &[PathBuf]) -> Result<(Vec<Location>, Vec<Location>)> {
    let mut start_locations = vec![];
    let mut end_locations = vec![];

    for project_file in project_files {
        let (text, tree) = parse_file(project_file)?;
        let root = tree.root_node();

        end_locations.extend(
            get_query_results(
                &text,
                root,
                &Query::new(
                    tree_sitter_solidity::language(),
                    r#"
            (member_expression
                object: (identifier) @obj (#match? @obj "msg")
                property: (identifier) @prop (#match? @prop "sender")
            ) @pub
            "#,
                )
                .unwrap(),
                2,
            )
            .iter()
            .map(|n| get_node_location(Converter::convert(project_file.as_path()), n)),
        );

        start_locations.extend(
            get_query_results(
                &text,
                root,
                &Query::new(
                    tree_sitter_solidity::language(),
                    r#"
        (call_expression
            function: (struct_expression
                type: (member_expression
                    object: (identifier) @hacky
                    property: (identifier) @method
                    (#match? @method "call")
                )
            )
        )
        "#,
                )
                .unwrap(),
                0,
            )
            .iter()
            .map(|n| get_node_location(Converter::convert(project_file.as_path()), n)),
        );
    }

    Ok((start_locations, end_locations))
}

enum SupportedLanguages {
    Solidity,
}

impl SupportedLanguages {
    fn get_language(&self) -> Box<impl SupportedLanguage> {
        match self {
            Self::Solidity => Box::new(Solidity),
        }
    }
}

impl From<&str> for SupportedLanguages {
    fn from(from: &str) -> Self {
        match from.to_lowercase().trim() {
            "solidity" => SupportedLanguages::Solidity,
            _ => panic!("got unsupported language: {}", from),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args();
    let _binary = args.next();
    let lang: SupportedLanguages = args.next().unwrap().trim().into();
    let root_dir: PathBuf = args.next().unwrap().trim().into();

    let root_dir = root_dir.canonicalize().unwrap();
    let mut project_files = vec![];
    visit_dirs(root_dir.as_path(), &mut |f| project_files.push(f.path()))
        .context("failed to get project files")?;

    let (start_locations, _end_locations) = get_start_end(&project_files)?;

    let all_paths = lang
        .get_language()
        .find_paths(&root_dir, project_files, start_locations, &[])
        .await?;

    let mut json_stacktraces: Vec<Value> = vec![];
    for path in all_paths {
        let mut json_steps = vec![];
        for location in path {
            let json_step = json!({
                "path": location.uri.to_file_path().expect("failed to convert uri to path"),
                "start": {
                    "line": location.range.start.line,
                    "character": location.range.start.character
                },
                "end": {
                    "line": location.range.end.line,
                    "character": location.range.end.character
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

    Ok(())
}
