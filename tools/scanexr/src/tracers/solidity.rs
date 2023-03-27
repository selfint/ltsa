use crate::{Stacktrace, Step, Tracer};
use anyhow::Result;
use async_trait::async_trait;
use lsp_client::client::Client;
use tree_sitter::Language;

pub struct SolidityTracer;

#[derive(Debug, Clone)]
pub enum SolidityStepContext {}

#[async_trait]
impl Tracer for SolidityTracer {
    type Context = Option<SolidityStepContext>;

    fn get_language(&self) -> Language {
        tree_sitter_solidity::language()
    }

    async fn get_stacktraces(
        &self,
        lsp_client: &Client,
        step: &Step<Self::Context>,
        stop_at: &[Step<Self::Context>],
    ) -> Result<Option<Vec<Stacktrace<Self>>>> {
        if stop_at.contains(step) {
            // let stacktrace: Stacktrace<Self> = Stacktrace::from(step.clone());
        }

        todo!()
    }
}
