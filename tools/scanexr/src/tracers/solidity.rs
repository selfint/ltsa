use crate::utils::{debug_node_step, get_node, get_tree};
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
    type StepContext = Option<SolidityStepContext>;

    fn get_language(&self) -> Language {
        tree_sitter_solidity::language()
    }

    async fn get_stacktraces(
        &self,
        lsp_client: &Client,
        step: &Step<Self::StepContext>,
        stop_at: &[Step<Self::StepContext>],
    ) -> Result<Vec<Stacktrace<Self::StepContext>>> {
        if stop_at.contains(step) {
            return Ok(vec![vec![step.clone()]]);
        }

        let tree = get_tree(step);
        let node = get_node(step, tree.root_node());
        let parent = node.parent().unwrap();

        debug_node_step(&node, &parent, step);

        todo!()
    }
}
