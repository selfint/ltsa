

// use tokio::sync::Mutex;

use crate::utils::{
    debug_node_step, get_node, get_query_results, get_step_definitions, get_tree, step_from_node,
};
use crate::{Stacktrace, Step, Tracer};

use anyhow::{ensure, Ok, Result};
use async_trait::async_trait;
use lsp_client::client::Client;
use tree_sitter::{Language, Query, Tree};

pub struct SolidityTracer;

#[derive(Debug, Clone)]
pub enum StepContext {
    GetReturnValue(Box<Step<StepContext>>),
}

#[async_trait]
impl Tracer for SolidityTracer {
    type StepContext = StepContext;

    fn get_language(&self) -> Language {
        tree_sitter_solidity::language()
    }

    async fn get_stacktraces(
        &self,
        lsp_client: &Client,
        step_file_tree: Tree,
        step: &Step<Self::StepContext>,
        stop_at: &[Step<Self::StepContext>],
    ) -> Result<Option<Vec<Stacktrace<Self::StepContext>>>> {
        if stop_at.contains(step) {
            return Ok(Some(vec![vec![]]));
        }

        let (node_kind, parent_kind) = {
            let node = get_node(step, step_file_tree.root_node());
            let kind = node.kind();
            let parent = node.parent().unwrap();
            let parent_kind = parent.kind();
            debug_node_step(&node, &parent, step);

            (kind, parent_kind)
        };

        match (node_kind, parent_kind, &step.context) {
            ("number_literal", _, _) => {
                dbg!("got literal");

                Ok(None)
            }
            ("identifier", "member_expression", None) => {
                // if we are a property, return our parent object
                {
                    let node = get_node(step, step_file_tree.root_node());
                    let parent = node.parent().unwrap();
                    if parent.child_by_field_name("property") == Some(node) {
                        dbg!("got property, next step is object");

                        // get object definition
                        let object = parent
                            .child_by_field_name("object")
                            .expect("got member expression with property but without object");

                        let next_step = step_from_node(step.path.clone(), object);

                        return Ok(Some(vec![vec![next_step]]));
                    }
                }

                dbg!("got object, finding definition");
                let definitions = get_step_definitions(lsp_client, step).await?;

                Ok(Some(vec![definitions]))
            }
            ("identifier", "variable_declaration", _) => {
                dbg!("got declaration, next step is value");

                let node = get_node(step, step_file_tree.root_node());
                let declaration = node.parent().unwrap().parent().unwrap();
                let value = declaration.child_by_field_name("value").unwrap();

                let next_step = step_from_node(step.path.clone(), value);

                Ok(Some(vec![vec![next_step]]))
            }
            ("call_expression", "variable_declaration_statement" | "return_statement", _) => {
                dbg!("get function output assigned to value, getting function return value");

                let node = get_node(step, step_file_tree.root_node());
                let function = node.child_by_field_name("function").unwrap();
                let mut function_step = step_from_node(step.path.clone(), function);
                function_step.context = Some(StepContext::GetReturnValue(Box::new(step.clone())));

                Ok(Some(vec![vec![function_step]]))
            }
            ("identifier", "call_expression", _) => {
                dbg!("got call expression, going to function definition");

                let mut function_definitions = get_step_definitions(lsp_client, step).await?;

                ensure!(
                    function_definitions.len() <= 1,
                    "got multiple function definitions"
                );
                ensure!(
                    !function_definitions.is_empty(),
                    "failed to get function definition"
                );

                let mut function_definition = function_definitions.remove(0);
                function_definition.context = step.context.clone();

                Ok(Some(vec![vec![function_definition]]))
            }
            ("identifier", "function_definition", Some(StepContext::GetReturnValue(..))) => {
                let node = get_node(step, step_file_tree.root_node());
                let parent = node.parent().unwrap();

                let source = std::fs::read(&step.path).unwrap();
                let text = parent.utf8_text(&source).unwrap();

                let query = Query::new(
                    tree_sitter_solidity::language(),
                    "(return_statement (_) @return)",
                )
                .unwrap();

                let return_values = get_query_results(text, parent, &query, 0);

                dbg!("got function definition, finding return value");
                Ok(Some(
                    return_values
                        .into_iter()
                        .map(|return_node| {
                            let mut return_step = step_from_node(step.path.clone(), return_node);
                            return_step.context = step.context.clone();
                            vec![return_step]
                        })
                        .collect(),
                ))
            }
            ("identifier", "return_statement" | "call_argument", _) => {
                dbg!(format!(
                    "get identifier in {}, finding definition",
                    parent_kind
                ));

                let mut definitions = get_step_definitions(lsp_client, step).await?;
                ensure!(definitions.len() <= 1, "got multiple function definitions");
                ensure!(!definitions.is_empty(), "failed to get function definition");
                let mut definition = definitions.remove(0);
                definition.context = step.context.clone();

                Ok(Some(vec![vec![definition]]))
            }
            ("identifier", "parameter", Some(StepContext::GetReturnValue(anchor))) => {
                dbg!("return value is a parameter, we are done, returning to anchor");

                let node = get_node(step, step_file_tree.root_node());
                let parent = node.parent().unwrap();
                let fn_def = parent.parent().unwrap();
                let mut cursor = fn_def.walk();
                let index = fn_def
                    .named_children(&mut cursor)
                    .position(|p| p == parent)
                    .unwrap();

                let anchor_tree = get_tree(anchor);
                let anchor_node = get_node(anchor, anchor_tree.root_node());
                let mut cursor = anchor_node.walk();
                let anchor_param = anchor_node.named_children(&mut cursor).nth(index).unwrap();
                let anchor_step = step_from_node(anchor.path.clone(), anchor_param);

                Ok(Some(vec![vec![anchor_step]]))
            }
            _ => todo!(),
        }
    }
}
