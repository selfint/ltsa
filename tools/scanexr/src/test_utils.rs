use std::fmt::Debug;

use lsp_types::{Location, Position, Range, Url};
use tempfile::{tempdir, TempDir};

pub const FILE_SEP: &str = "---";
pub const FILENAME_SEP: &str = "#@#";

pub fn display_location<M: Debug>(location: &Location, meta: &M, scrolloff: Option<u32>) -> String {
    let path = location.uri.to_file_path().unwrap();
    let filename: &str = path.file_name().unwrap().to_str().unwrap();

    let content = String::from_utf8(std::fs::read(location.uri.path()).unwrap()).unwrap();

    let mut new_lines = vec![];
    let first_line = if let Some(scrolloff) = scrolloff {
        location.range.start.line - scrolloff.min(location.range.start.line)
    } else {
        0
    };
    let last_line = scrolloff.map(|scrolloff| location.range.end.line + scrolloff);

    for (i, line) in content.lines().enumerate() {
        if i >= first_line as usize {
            if let Some(last_line) = last_line {
                if i <= last_line as usize {
                    new_lines.push(line.to_string());
                }
            } else {
                new_lines.push(line.to_string());
            }
        }

        if location.range.start.line == i as u32 {
            new_lines.push(
                " ".repeat(location.range.start.character as usize)
                    + &"^".repeat(
                        (location.range.end.character - location.range.start.character) as usize,
                    )
                    + &format!(" Meta: {:?}", meta),
            );
        }
    }
    let content = new_lines.join("\n");

    format!("{}\n{}\n{}", filename, FILENAME_SEP, content)
}

pub fn display_locations<M: Debug>(
    locations: Vec<(Location, M)>,
    scrolloff: Option<u32>,
) -> String {
    let mut file_locations = vec![];

    for (location, meta) in &locations {
        file_locations.push(display_location(location, meta, scrolloff));
    }

    file_locations.join(&format!("\n{}\n", FILE_SEP))
}

pub fn setup_test_dir(content: &str) -> (TempDir, Location, Vec<Location>, Vec<Location>) {
    let tempdir = tempdir().expect("failed to create tempdir");

    let files = content.split(FILE_SEP);
    let mut start = None;
    let mut definitions = vec![];
    let mut references = vec![];

    for file in files {
        let binding = file.split(FILENAME_SEP).collect::<Vec<_>>();
        let [name, content] = binding.as_slice() else {
            panic!("invalid file content")
        };

        let filepath = tempdir.path().join(name.trim());

        for (i, line) in content.lines().enumerate() {
            if line.contains("^ start") {
                let start_start = line.find('^').unwrap();
                let start_end = line.find("^ start").unwrap() + 1;
                assert!(start.is_none(), "found multiple start locations");
                start = Some(Location::new(
                    Url::from_file_path(&filepath).unwrap(),
                    Range {
                        start: Position {
                            line: (i - 1) as u32,
                            character: start_start as u32,
                        },
                        end: Position {
                            line: (i - 1) as u32,
                            character: start_end as u32,
                        },
                    },
                ));
            }

            if line.contains("^ definition") {
                let definition_start = line.find('^').unwrap();
                let definition_end = line.find("^ definition").unwrap() + 1;
                let definition = Location::new(
                    Url::from_file_path(&filepath).unwrap(),
                    Range {
                        start: Position {
                            line: (i - 1) as u32,
                            character: definition_start as u32,
                        },
                        end: Position {
                            line: (i - 1) as u32,
                            character: definition_end as u32,
                        },
                    },
                );

                definitions.push(definition);
            }

            if line.contains("^ reference") {
                let reference_start = line.find('^').unwrap();
                let reference_end_end = line.find("^ reference").unwrap() + 1;
                let reference = Location::new(
                    Url::from_file_path(&filepath).unwrap(),
                    Range {
                        start: Position {
                            line: (i - 1) as u32,
                            character: reference_start as u32,
                        },
                        end: Position {
                            line: (i - 1) as u32,
                            character: reference_end_end as u32,
                        },
                    },
                );

                references.push(reference);
            }
        }

        std::fs::write(filepath, content).expect("failed to write file");
    }

    let start = start.expect("failed to find start");

    (tempdir, start, definitions, references)
}
