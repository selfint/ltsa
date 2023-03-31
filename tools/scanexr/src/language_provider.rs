use anyhow::{Context, Result};
use async_recursion::async_recursion;
use async_trait::async_trait;
use lsp_types::Location;
use tree_sitter::{Language, Tree};

#[async_trait]
pub trait LspProvider {
    async fn find_definitions(&self, location: &Location) -> Result<Vec<Location>>;
    async fn find_references(&self, location: &Location) -> Result<Vec<Location>>;
}

/// Push down automaton that receives an input
/// and returns the next states.
///
/// Notice that the 'state' of the automaton is the
/// current input, and the next states will be the
/// next inputs.
pub trait LanguageAutomata {
    type Stack;
    type LspProvider: LspProvider;

    fn get_language(&self) -> Language;
    fn initial_state(&self) -> Vec<Self::Stack>;

    /// Get the next (inputs, items to push to the stack) values.
    fn transition(
        &self,
        input: Location,
        stack: Self::Stack,
        definitions: Result<Vec<Location>>,
        references: Result<Vec<Location>>,
    ) -> Result<Vec<(Location, Vec<Self::Stack>)>>;

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
pub async fn find_paths<P>(
    language_provider: &P,
    lsp_provider: &P::LspProvider,
    location: Location,
    mut stack: Vec<P::Stack>,
    stop_at: &[Location],
) -> Result<Vec<Vec<Location>>>
where
    P: LanguageAutomata + Sync + Send,
    P::Stack: Sync + Send + Clone,
    P::LspProvider: Sync,
{
    if stop_at.contains(&location) {
        return Ok(vec![vec![location.clone()]]);
    }

    let definitions = lsp_provider.find_definitions(&location).await;
    let references = lsp_provider.find_references(&location).await;
    let stack_head = stack.pop().unwrap();
    let next_steps =
        language_provider.transition(location.clone(), stack_head, definitions, references)?;

    if next_steps.is_empty() {
        return Ok(vec![vec![location.clone()]]);
    }

    let mut paths = vec![];
    for (next_location, mut pushed_items) in next_steps {
        let mut next_stack = stack.clone();
        next_stack.append(&mut pushed_items);
        let next_paths = find_paths::<P>(
            language_provider,
            lsp_provider,
            next_location,
            next_stack,
            stop_at,
        )
        .await?;

        for mut next_path in next_paths {
            next_path.insert(0, location.clone());
            paths.push(next_path);
        }
    }

    Ok(paths)
}
