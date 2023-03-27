use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use async_recursion::async_recursion;
use async_trait::async_trait;
use lsp_client::client::Client;
use lsp_types::Position;
use tree_sitter::{Language, Point, Query};

use crate::utils::get_query_steps;

pub mod tracers;
pub mod utils;

#[derive(Debug, Clone, Eq)]
pub struct Step<C> {
    pub path: PathBuf,
    pub start: StepPosition,
    pub end: StepPosition,
    pub context: Option<C>,
}

impl<C> Step<C> {
    fn new(path: PathBuf, start: impl Into<StepPosition>, end: impl Into<StepPosition>) -> Step<C> {
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

pub type Stacktrace<C> = Vec<Step<C>>;

#[async_trait]
pub trait Tracer: Send + Sync {
    type StepContext: Debug + Default + Clone + Send + Sync;

    fn get_language(&self) -> Language;

    /// given a step, get all possible stack traces leading to it
    async fn get_stacktraces(
        &self,
        lsp_client: &Client,
        step: &Step<Self::StepContext>,
        stop_at: &[Step<Self::StepContext>],
    ) -> Result<Vec<Stacktrace<Self::StepContext>>>;
}

pub async fn get_all_stacktraces<T: Tracer>(
    tracer: &T,
    lsp_client: &Client,
    root_dir: &Path,
    pub_queries: &[(Query, u32)],
    hacky_query: &(Query, u32),
) -> Result<Vec<Stacktrace<T::StepContext>>> {
    let mut pub_steps = vec![];
    for pub_query in pub_queries {
        let steps = get_query_steps(root_dir, tracer.get_language(), pub_query)
            .context("getting pub query steps")?;
        for step in steps {
            if !pub_steps.contains(&step) {
                pub_steps.push(step);
            }
        }
    }

    let hacky_steps = get_query_steps(root_dir, tracer.get_language(), hacky_query)
        .context("getting hacky steps")?;

    let mut all_stacktraces = vec![];
    for hacky_step in &hacky_steps {
        let stacktraces = _get_all_stacktraces(tracer, lsp_client, hacky_step, &pub_steps)
            .await
            .context("completing stacktraces")?;

        all_stacktraces.extend(stacktraces);
    }

    Ok(all_stacktraces)
}

#[async_recursion]
async fn _get_all_stacktraces<T: Tracer>(
    tracer: &T,
    lsp_client: &Client,
    step: &Step<T::StepContext>,
    stop_at: &[Step<T::StepContext>],
) -> Result<Vec<Stacktrace<T::StepContext>>> {
    // get stacktraces leading to step
    let stacktraces = tracer.get_stacktraces(lsp_client, step, stop_at).await?;

    // complete stacktraces leading to step
    let mut completed_stacktraces = vec![];
    for stacktrace in stacktraces {
        let next_stacktraces = _get_all_stacktraces(tracer, lsp_client, step, stop_at).await?;
        for next_stacktrace in next_stacktraces {
            let mut completed_stacktrace = stacktrace.clone();
            completed_stacktrace.extend(next_stacktrace);

            completed_stacktraces.push(completed_stacktrace);
        }
    }

    Ok(completed_stacktraces)
}
