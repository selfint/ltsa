use anyhow::{Context, Result};
use async_recursion::async_recursion;
use async_trait::async_trait;
use lsp_types::{Location, Url};
use tree_sitter::{Language, Node, Point, Tree};

use crate::{Convert, Converter};

#[async_trait]
pub trait LspProvider {
    async fn find_definitions(&self, location: &Location) -> Result<Vec<Location>>;
    async fn find_references(&self, location: &Location) -> Result<Vec<Location>>;
}

pub fn get_node_location(uri: Url, node: &Node) -> Location {
    Location {
        uri,
        range: lsp_types::Range {
            start: Converter::convert(node.start_position()),
            end: Converter::convert(node.end_position()),
        },
    }
}

pub fn get_location_node<'a>(root: Node<'a>, location: &Location) -> Option<Node<'a>> {
    let start = Point {
        row: location.range.start.line as usize,
        column: location.range.start.character as usize,
    };
    let end = Point {
        row: location.range.end.line as usize,
        column: location.range.end.character as usize,
    };

    root.named_descendant_for_point_range(start, end)
}

pub fn get_breadcrumbs<'a>(root: Node<'a>, location: &Location) -> Option<Vec<Node<'a>>> {
    let mut node = get_location_node(root, location)?;

    let mut breadcrumbs = vec![];
    while let Some(parent_node) = node.parent() {
        breadcrumbs.push(node);
        node = parent_node;
    }

    Some(breadcrumbs)
}

pub trait LanguageProvider {
    type State;
    type LspProvider: LspProvider;

    fn get_language(&self) -> Language;
    fn initial_state(&self) -> Self::State;
    fn get_next_steps(
        &self,
        step: (Location, Self::State),
        definitions: Result<Vec<Location>>,
        references: Result<Vec<Location>>,
    ) -> Result<Vec<(Location, Self::State)>>;

    fn get_tree(&self, location: &Location) -> Result<Tree> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(self.get_language())
            .context("failed to set language")?;

        let content =
            String::from_utf8(std::fs::read(location.uri.to_file_path().unwrap()).unwrap())
                .unwrap();

        parser.parse(content, None).context("failed to parse text")
    }
}

#[async_recursion]
pub async fn find_paths<S>(
    strategy: &S,
    lsp_provider: &S::LspProvider,
    start: (Location, S::State),
    stop_at: &[Location],
) -> Result<Vec<Vec<Location>>>
where
    S: LanguageProvider + Sync + Send,
    S::State: Sync + Send + Clone,
    S::LspProvider: Sync,
{
    if stop_at.contains(&start.0) {
        return Ok(vec![vec![start.0.clone()]]);
    }

    let definitions = lsp_provider.find_definitions(&start.0).await;
    let references = lsp_provider.find_references(&start.0).await;
    let next_steps = strategy.get_next_steps(start.clone(), definitions, references)?;

    let mut paths = vec![];
    for next_step in next_steps {
        let next_paths = find_paths::<S>(strategy, lsp_provider, next_step, stop_at).await?;

        for mut next_path in next_paths {
            next_path.insert(0, start.0.clone());
            paths.push(next_path);
        }
    }

    Ok(paths)
}
