use std::{fmt::Debug, path::PathBuf};

use lsp_types::Position;
use tree_sitter::Point;

#[derive(Debug, Clone, Eq)]
pub struct Step<C> {
    pub path: PathBuf,
    pub start: StepPosition,
    pub end: StepPosition,
    pub context: Option<C>,
}

impl<C> Step<C> {
    pub fn new(
        path: PathBuf,
        start: impl Into<StepPosition>,
        end: impl Into<StepPosition>,
    ) -> Step<C> {
        Self {
            path,
            start: start.into(),
            end: end.into(),
            context: None,
        }
    }
}

impl<C> PartialEq for Step<C> {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && self.start == other.start && self.end == other.end
        // && self.context == other.context
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct StepPosition {
    pub line: usize,
    pub character: usize,
}

impl From<Point> for StepPosition {
    fn from(point: Point) -> Self {
        Self {
            line: point.row,
            character: point.column,
        }
    }
}

impl From<StepPosition> for Point {
    fn from(step_position: StepPosition) -> Self {
        Self {
            row: step_position.line,
            column: step_position.character,
        }
    }
}

impl From<Position> for StepPosition {
    fn from(position: Position) -> Self {
        Self {
            line: position.line as usize,
            character: position.character as usize,
        }
    }
}
