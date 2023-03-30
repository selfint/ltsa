use std::path::{Path, PathBuf};

use lsp_types::{Position, Url};
use tree_sitter::Point;

pub mod language_provider;
pub mod languages;
pub mod utils;

pub trait Convert<From, To> {
    fn convert(from: From) -> To;
}

struct Converter;

impl Convert<Position, Point> for Converter {
    fn convert(from: Position) -> Point {
        Point {
            row: from.line as usize,
            column: from.character as usize,
        }
    }
}

impl Convert<Point, Position> for Converter {
    fn convert(from: Point) -> Position {
        Position {
            line: from.row as u32,
            character: from.column as u32,
        }
    }
}

impl Convert<&Path, Url> for Converter {
    fn convert(from: &Path) -> Url {
        Url::from_file_path(from).expect("failed to convert path to url")
    }
}

impl Convert<&Url, PathBuf> for Converter {
    fn convert(from: &Url) -> PathBuf {
        from.to_file_path()
            .expect("failed to convert url to file path")
    }
}

#[cfg(feature = "test-utils")]
pub mod test_utils {
    use std::fmt::Debug;

    use lsp_types::{Location, Position, Range, Url};
    use tempfile::{tempdir, TempDir};

    pub const FILE_SEP: &str = "---";
    pub const FILENAME_SEP: &str = "#@#";

    pub fn display_location<M: Debug>((location, meta): &(Location, M)) -> String {
        let path = location.uri.to_file_path().unwrap();
        let filename: &str = path.file_name().unwrap().to_str().unwrap();

        let content = String::from_utf8(std::fs::read(location.uri.path()).unwrap()).unwrap();

        let mut new_lines = vec![];
        for (i, line) in content.lines().enumerate() {
            new_lines.push(line.to_string());
            if location.range.start.line == i as u32 {
                new_lines.push(
                    " ".repeat(location.range.start.character as usize)
                        + &"^".repeat(
                            (location.range.end.character - location.range.start.character + 1)
                                as usize,
                        )
                        + &format!(" Meta: {:?}", meta),
                );
            }
        }
        let content = new_lines.join("\n");

        format!("{}\n{}\n{}", filename, FILENAME_SEP, content)
    }

    pub fn display_locations<M: Debug>(locations: Vec<(Location, M)>) -> String {
        let mut file_locations = vec![];

        for location in &locations {
            file_locations.push(display_location(location));
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
                    let start_end = line.find("^ start").unwrap();
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
                    let definition_end = line.find("^ definition").unwrap();
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
                    let reference_end_end = line.find("^ reference").unwrap();
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
}
