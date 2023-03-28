use std::{fmt::Debug, path::PathBuf};

use lsp_types::Position;
use tree_sitter::{Node, Point};

#[derive(Debug, Clone, Eq)]
pub struct Step<C: Default> {
    pub path: PathBuf,
    pub start: StepPosition,
    pub end: StepPosition,
    pub context: C,
}

impl<C: Default> Step<C> {
    pub fn new(
        path: PathBuf,
        start: impl Into<StepPosition>,
        end: impl Into<StepPosition>,
    ) -> Step<C> {
        Self {
            path,
            start: start.into(),
            end: end.into(),
            context: Default::default(),
        }
    }
}

impl<C: Default> PartialEq for Step<C> {
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

impl From<StepPosition> for Position {
    fn from(step_position: StepPosition) -> Self {
        Self {
            line: step_position.line as u32,
            character: step_position.character as u32,
        }
    }
}

impl<C: Default> From<(PathBuf, Node<'_>)> for Step<C> {
    fn from((path, node): (PathBuf, Node)) -> Self {
        Step::new(path, node.start_position(), node.end_position())
    }
}
