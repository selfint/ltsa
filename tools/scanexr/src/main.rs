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
    let language: SupportedLanguages = args.next().unwrap().trim().into();
    let root_dir: PathBuf = args.next().unwrap().trim().into();

    let language = language.get_language();

    let root_dir = root_dir.canonicalize().unwrap();
    let mut project_files = vec![];
    visit_dirs(root_dir.as_path(), &mut |f| project_files.push(f.path()))
        .context("failed to get project files")?;

    let (start_locations, _end_locations) = language.get_start_end(&project_files)?;

    let all_paths = language
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
