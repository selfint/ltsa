use anyhow::Result;
use async_recursion::async_recursion;
use async_trait::async_trait;
use lsp_types::{Location, Position};
use tree_sitter::Point;

#[async_trait]
pub trait LspProvider {
    async fn find_definitions(&self, location: &Location) -> Result<Vec<Location>>;
    async fn find_references(&self, location: &Location) -> Result<Vec<Location>>;
}

pub trait CstProvider {
    type Node;
    fn get_location_node(&self, location: &Location) -> Result<Self::Node>;
    fn get_parent(&self, node: &Self::Node) -> Result<Option<Self::Node>>;

    fn get_breadcrumbs(&self, location: &Location) -> Result<Vec<Self::Node>> {
        let mut node = self.get_location_node(location)?;

        let mut breadcrumbs = vec![];
        while let Some(parent_node) = self.get_parent(&node)? {
            breadcrumbs.push(node);
            node = parent_node;
        }

        Ok(breadcrumbs)
    }
}

#[async_trait]
pub trait LanguageProvider {
    type State;
    type CstProvider: CstProvider;
    type LspProvider: LspProvider;

    fn initial_state(&self) -> Self::State;

    async fn get_next_step(
        &self,
        step: &(Location, Self::State),
        lsp_provider: &Self::LspProvider,
        cst_provider: &Self::CstProvider,
    ) -> Result<Vec<(Location, Self::State)>>;
}

#[async_recursion]
pub async fn find_paths<L, C, S>(
    strategy: &S,
    cst_provider: &S::CstProvider,
    lsp_provider: &S::LspProvider,
    start: &(Location, S::State),
    stop_at: &[Location],
) -> Result<Vec<Vec<Location>>>
where
    S: LanguageProvider + Sync + Send,
    S::State: Sync + Send,
    S::CstProvider: Sync,
    S::LspProvider: Sync,
{
    if stop_at.contains(&start.0) {
        return Ok(vec![vec![start.0.clone()]]);
    }

    let next_steps = strategy
        .get_next_step(start, lsp_provider, cst_provider)
        .await?;

    let mut paths = vec![];
    for next_step in next_steps {
        let next_paths =
            find_paths::<L, C, S>(strategy, cst_provider, lsp_provider, &next_step, stop_at)
                .await?;

        for mut next_path in next_paths {
            next_path.insert(0, start.0.clone());
            paths.push(next_path);
        }
    }

    Ok(paths)
}
