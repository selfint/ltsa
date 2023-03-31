use std::path::{Path, PathBuf};

use lsp_types::{Position, Url};
use tree_sitter::Point;

pub trait Convert<From, To> {
    fn convert(from: From) -> To;
}

pub struct Converter;

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
