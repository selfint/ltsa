use std::path::PathBuf;

use tree_sitter::{Node, Point, Tree};

use crate::{LanguageProvider, LspMethod, Step};

pub struct SolidityLanguageProvider;
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum StepContext {}

impl LanguageProvider for SolidityLanguageProvider {
    type Context = StepContext;

    fn get_previous_step(
        &self,
        step: &Step<Self::Context>,
        previous_step: Option<&Step<Self::Context>>,
    ) -> Option<Vec<(LspMethod, Step<Self::Context>, Vec<Step<Self::Context>>)>> {
        let tree = get_tree(step);
        let node = get_node(step, tree.root_node());
        let parent = node.parent().unwrap();

        eprintln!(
            "got step with node kind: {:?} / parent: {:?} / context: {:?}, line:\n\n{}\n{}\n",
            node.kind(),
            parent.kind(),
            step.context,
            get_step_line(step),
            " ".repeat(node.start_position().column)
                + &"^".repeat(node.end_position().column - node.start_position().column)
        );

        match (
            node.kind(),
            parent.kind(),
            previous_step.and_then(|p| p.context.as_ref()),
        ) {
            ("identifier", "member_expression", None) => {
                // if we are a property
                if parent.child_by_field_name("property") == Some(node) {
                    dbg!("got property, next step is object");

                    // get object definition
                    let object = parent
                        .child_by_field_name("object")
                        .expect("got member expression with property but without object");
                    let next_step = step_from_node(step.path.clone(), object);

                    Some(vec![(
                        LspMethod::Nop,
                        next_step.clone(),
                        vec![step.clone(), next_step],
                    )])
                } else {
                    dbg!("got object, finding definition");
                    Some(vec![(
                        LspMethod::Definition,
                        step.clone(),
                        vec![step.clone()],
                    )])
                }
            }
            ("identifier", "variable_declaration", None) => {
                dbg!("got variable declaration, finding references");
                Some(vec![(
                    LspMethod::References,
                    step.clone(),
                    vec![step.clone()],
                )])
            }
            _ => {
                todo!()
            }
        }
    }
}

fn get_tree<C>(step: &Step<C>) -> Tree {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(tree_sitter_solidity::language())
        .expect("failed to set language");

    let content = String::from_utf8(std::fs::read(&step.path).unwrap()).unwrap();

    parser
        .parse(&content, None)
        .expect("failed to parse content")
}

fn get_node<'a, C>(step: &Step<C>, root: Node<'a>) -> Node<'a> {
    root.descendant_for_point_range(
        Point {
            row: step.start.0 as usize,
            column: step.start.1 as usize,
        },
        Point {
            row: step.end.0 as usize,
            column: step.end.1 as usize,
        },
    )
    .expect("failed to get node at location range")
}

fn get_step_line<C>(step: &Step<C>) -> String {
    let content = String::from_utf8(std::fs::read(&step.path).unwrap()).unwrap();
    let line = step.start.0;
    content.lines().nth(line as usize).unwrap().to_string()
}

fn step_from_node<C>(path: PathBuf, node: Node) -> Step<C> {
    let start = node.start_position();
    let end = node.end_position();

    let start = (start.row as u32, start.column as u32);
    let end = (end.row as u32, end.column as u32);

    Step::new(path, start, end)
}
