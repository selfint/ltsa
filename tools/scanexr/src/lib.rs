use std::{fmt::Debug, path::Path};

use anyhow::{Context, Result};
use async_recursion::async_recursion;
use async_trait::async_trait;
use lsp_client::client::Client;
use step::Step;
use tree_sitter::{Language, Query};

use crate::utils::get_query_steps;

pub mod step;
pub mod tracers;
pub mod utils;

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
