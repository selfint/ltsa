// use tokio::sync::Mutex;

use std::path::Path;

use crate::utils::{
    debug_node_step, get_node, get_query_results, get_step_definitions, get_tree, step_from_node,
    visit_dirs,
};
use crate::{Stacktrace, Step, Tracer};

use anyhow::{bail, ensure, Context, Result};
use async_trait::async_trait;
use lsp_client::client::Client;
use tree_sitter::{Language, Node, Query, Tree};

/// TODO: if solc --lsp ever supports `findReferences`, do this properly
async fn find_references(
    step: &Step<StepContext>,
    lsp_client: &Client,
    root_dir: &Path,
) -> Result<Vec<Step<StepContext>>> {
    let query = (
        Query::new(
            tree_sitter_solidity::language(),
            "(call_expression function: [
                (identifier) @ident
                (member_expression
                    property: (identifier) @ident
                )
            ])",
        )
        .unwrap(),
        0,
    );

    let fn_call_steps = {
        let language = tree_sitter_solidity::language();

        let mut locations = vec![];
        visit_dirs(root_dir, &mut |dir| {
            let content =
                String::from_utf8(std::fs::read(dir.path()).expect("failed to read file"))
                    .expect("got non-utf8 file");

            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(language)
                .expect("failed to set language on parser");
            let tree = parser
                .parse(&content, None)
                .expect("failed to parse content");

            let results = get_query_results(&content, tree.root_node(), &query.0, query.1);

            for result in results {
                let start = result.start_position();
                let end = result.end_position();
                locations.push(Step::new(dir.path(), start, end));
            }
        })?;

        locations
    };

    let mut references = vec![];
    for fn_call_step in fn_call_steps {
        let Ok(definitions) = get_step_definitions(lsp_client, &fn_call_step).await
            else {
                continue;
            };

        for definition in definitions {
            if &definition == step {
                references.push(fn_call_step.clone());
            }
        }
    }

    Ok(references)
}

fn find_fn_parameter_index(node: Node) -> (Node, Option<usize>) {
    let parent = node.parent().unwrap();
    let fn_def = parent.parent().unwrap();
    let mut cursor = fn_def.walk();

    let index = fn_def
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "parameter")
        .position(|p| p == parent);

    (fn_def, index)
}

fn find_tuple_declaration_index(node: Node) -> (Node, Option<usize>) {
    let variable_declaration = node.parent().unwrap();
    let variable_declaration_tuple = variable_declaration.parent().unwrap();
    let mut cursor = variable_declaration_tuple.walk();

    let index = variable_declaration_tuple
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "variable_declaration")
        .position(|p| p == variable_declaration);

    (variable_declaration_tuple, index)
}

fn get_call_expression_arg(call_expression: Node, index: usize) -> Option<Node> {
    let mut cursor = call_expression.walk();

    let parameter = call_expression
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "call_argument")
        .nth(index);

    parameter
}

fn get_tuple_index(node: Node, index: usize) -> Option<Node> {
    let mut cursor = node.walk();

    let value = node
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "identifier")
        .nth(index);

    value
}

pub struct SolidityTracer;

#[derive(Debug, Clone, Default)]
pub enum StepContext {
    #[default]
    None,
    GetReturnValue(Box<Step<StepContext>>),
    FindReference(usize),
    GetTupleValue(usize),
    GetReturnTupleValue(Box<Step<StepContext>>, usize),
    /// goto where the member's value is set (including initiation)
    FindMember(String),
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
        root_dir: &Path,
        step_file_tree: Tree,
        step: &Step<Self::StepContext>,
        stop_at: &[Step<Self::StepContext>],
    ) -> Result<Option<Vec<Stacktrace<Self::StepContext>>>> {
        let (node_kind, parent_kind) = {
            let node = get_node(step, step_file_tree.root_node());
            let kind = node.kind();
            let parent = node.parent().unwrap();
            let parent_kind = parent.kind();
            debug_node_step(&node, &parent, step);

            (kind, parent_kind)
        };

        if stop_at.contains(step) {
            return Ok(Some(vec![vec![]]));
        }

        match (node_kind, parent_kind, &step.context) {
            ("number_literal", _, _) => {
                dbg!("got literal");

                Ok(None)
            }
            ("identifier", "state_variable_declaration", StepContext::FindMember(member)) => {
                dbg!(format!("got state variable declaration"));
                // TODO: implement this
                Ok(None)
            }
            ("type_cast_expression", _, _) => {
                dbg!("got type cast expression, going to identifier");
                let type_cast_expression = get_node(step, step_file_tree.root_node());
                let mut cursor = type_cast_expression.walk();

                let identifiers = type_cast_expression
                    .named_children(&mut cursor)
                    .filter(|c| c.kind() == "identifier")
                    .collect::<Vec<_>>();
                ensure!(
                    identifiers.len() == 1,
                    "got multiple identifiers in type cast expression"
                );

                let identifier = identifiers[0];
                let next_step = step_from_node(step.path.clone(), identifier);

                Ok(Some(vec![vec![next_step]]))
            }
            ("identifier", "interface_declaration", StepContext::GetReturnValue(anchor)) => {
                dbg!("got identifier cast as interface with get return value context, returning to anchor");
                let anchor_tree = get_tree(anchor);
                let anchor_call_expression = get_node(anchor, anchor_tree.root_node());

                dbg!(anchor_call_expression.to_sexp());
                let arg = get_call_expression_arg(anchor_call_expression, 0).unwrap();
                let next_step = step_from_node(anchor.path.clone(), arg);

                Ok(Some(vec![vec![next_step]]))
            }
            ("identifier", "member_expression", StepContext::None) => {
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

                        let mut next_step = step_from_node(step.path.clone(), object);
                        let content = std::fs::read(&step.path).unwrap();
                        let property_name = node.utf8_text(&content).unwrap();

                        next_step.context = StepContext::FindMember(property_name.to_string());

                        return Ok(Some(vec![vec![next_step]]));
                    }
                }

                dbg!("got object, finding definition");
                let definitions = get_step_definitions(lsp_client, step).await?;
                ensure!(!definitions.is_empty(), "got no definitions");

                Ok(Some(vec![definitions]))
            }
            ("identifier", "variable_declaration", _) => {
                dbg!("got declaration, next step is value");

                let node = get_node(step, step_file_tree.root_node());
                let declaration = node.parent().unwrap().parent().unwrap();
                match declaration.kind() {
                    "variable_declaration_statement" => {
                        let value = declaration.child_by_field_name("value").unwrap();

                        let next_step = step_from_node(step.path.clone(), value);

                        Ok(Some(vec![vec![next_step]]))
                    }
                    "variable_declaration_tuple" => {
                        let (tuple_declaration, Some(tuple_index)) = find_tuple_declaration_index(node) else {
                            bail!("failed to find tuple index");
                        };

                        let declaration_statement = tuple_declaration.parent().unwrap();
                        let declaration_value =
                            declaration_statement.child_by_field_name("value").unwrap();

                        let mut next_step = step_from_node(step.path.clone(), declaration_value);
                        next_step.context = StepContext::GetTupleValue(tuple_index);

                        Ok(Some(vec![vec![next_step]]))
                    }
                    _ => todo!(),
                }
            }
            (
                "call_expression",
                "variable_declaration_statement" | "return_statement",
                StepContext::GetTupleValue(index),
            ) => {
                dbg!("get function output assigned to value, getting function return value");

                let node = get_node(step, step_file_tree.root_node());
                let function = node.child_by_field_name("function").unwrap();
                let mut function_step = step_from_node(step.path.clone(), function);
                function_step.context =
                    StepContext::GetReturnTupleValue(Box::new(step.clone()), *index);

                Ok(Some(vec![vec![function_step]]))
            }
            ("call_expression", parent_kind, ctx) => {
                dbg!(ctx);
                dbg!(format!(
                    "got call expression in {}, getting return value",
                    parent_kind
                ));

                let node = get_node(step, step_file_tree.root_node());
                let function = node.child_by_field_name("function").unwrap();
                let mut function_step = step_from_node(step.path.clone(), function);
                function_step.context = StepContext::GetReturnValue(Box::new(step.clone()));

                Ok(Some(vec![vec![function_step]]))
            }
            (
                "identifier",
                "function_definition",
                StepContext::GetReturnValue(..) | StepContext::GetReturnTupleValue(..),
            ) => {
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
            ("identifier", "parameter", StepContext::GetReturnValue(anchor)) => {
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
            ("identifier", "parameter", ctx) => {
                dbg!(ctx);
                dbg!("got parameter, finding function references");

                let node = get_node(step, step_file_tree.root_node());
                let (fn_def, Some(parameter_index)) = find_fn_parameter_index(node) else {
                    bail!("failed to get parameter index");
                };

                let fn_ident = fn_def.child_by_field_name("name").unwrap();
                let mut fn_step = step_from_node(step.path.clone(), fn_ident);
                fn_step.context = StepContext::FindReference(parameter_index);

                Ok(Some(vec![vec![fn_step]]))
            }
            ("identifier", "function_definition", StepContext::FindReference(..)) => {
                dbg!("got function definition with find reference context, finding references");

                let references = find_references(step, lsp_client, root_dir).await?;
                dbg!(references.len());

                if !references.is_empty() {
                    Ok(Some(
                        references
                            .into_iter()
                            .map(|mut r| {
                                r.context = step.context.clone();
                                vec![r]
                            })
                            .collect(),
                    ))
                } else {
                    Ok(None)
                }
            }
            ("identifier", "member_expression", StepContext::FindReference(index)) => {
                dbg!("got member expression with find reference context, going to argument");

                let identifier = get_node(step, step_file_tree.root_node());
                let member_expression = identifier.parent().unwrap();

                // if index is 0, then the argument is our matching object
                let argument = if *index == 0 {
                    member_expression.child_by_field_name("object").unwrap()
                } else {
                    let call_expression = member_expression.parent().unwrap();
                    let Some(argument) = get_call_expression_arg(call_expression, index - 1)
                            else {
                                bail!("failed to get call expression argument");
                            };

                    argument
                };

                let next_step = step_from_node(step.path.clone(), argument);
                Ok(Some(vec![vec![next_step]]))
            }
            (
                "identifier" | "member_expression",
                "call_expression",
                StepContext::FindReference(index),
            ) => {
                dbg!("got call expression with find reference context, going to argument");
                let node = get_node(step, step_file_tree.root_node());
                let call_expression = node.parent().unwrap();
                let Some(argument) = get_call_expression_arg(call_expression, *index) else {
                    bail!("failed to get argument");
                };

                let step = step_from_node(step.path.clone(), argument);

                Ok(Some(vec![vec![step]]))
            }
            (
                "member_expression",
                "call_expression",
                StepContext::GetReturnValue(..) | StepContext::GetReturnTupleValue(..),
            ) => {
                let member_expression = get_node(step, step_file_tree.root_node());
                let function_identifier =
                    member_expression.child_by_field_name("property").unwrap();
                let mut function_step = step_from_node(step.path.clone(), function_identifier);
                function_step.context = step.context.clone();

                Ok(Some(vec![vec![function_step]]))
            }
            (
                "identifier",
                parent_kind,
                StepContext::GetReturnValue(..) | StepContext::GetReturnTupleValue(..),
            ) => {
                dbg!(format!(
                    "got identifier in {} with return value context, going to function definition",
                    parent_kind
                ));

                let mut function_definitions = get_step_definitions(lsp_client, step)
                    .await
                    .context("getting definition")?;

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
            (
                "tuple_expression",
                "return_statement",
                StepContext::GetReturnTupleValue(anchor, index),
            ) => {
                let node = get_node(step, step_file_tree.root_node());
                let Some(value) = get_tuple_index(node, *index) else {
                    bail!("failed to get tuple index value");
                };

                let mut next_step = step_from_node(step.path.clone(), value);
                next_step.context = StepContext::GetReturnValue(anchor.clone());

                Ok(Some(vec![vec![next_step]]))
            }
            ("member_expression", _, ctx) => {
                dbg!(ctx);
                dbg!(format!(
                    "got member_expression in {}, going to property",
                    parent_kind
                ));

                let member_expression = get_node(step, step_file_tree.root_node());
                let property = member_expression.child_by_field_name("property").unwrap();
                let next_step = step_from_node(step.path.clone(), property);

                Ok(Some(vec![vec![next_step]]))
            }
            ("identifier", _, _) => {
                dbg!(format!(
                    "got identifier in {}, finding definition",
                    parent_kind
                ));

                let mut definitions = get_step_definitions(lsp_client, step).await?;
                ensure!(definitions.len() <= 1, "got multiple function definitions");
                ensure!(!definitions.is_empty(), "failed to get function definition");
                let mut definition = definitions.remove(0);
                definition.context = step.context.clone();

                Ok(Some(vec![vec![definition]]))
            }
            _ => todo!(),
        }
    }
}
