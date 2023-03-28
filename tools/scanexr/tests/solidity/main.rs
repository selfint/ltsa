use lsp_types::{notification::*, request::*, *};
use scanexr::{
    tracers::solidity::StepContext,
    utils::{format_node_step, get_node, get_tree},
};
use std::process::Stdio;
use tempfile::{tempdir, TempDir};
use tokio::process::{Child, Command};
use tree_sitter::Query;

fn start_solidity_ls() -> Child {
    Command::new("solc")
        .arg("--lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start solidity ls")
}

fn get_temp_dir() -> TempDir {
    let contract = include_str!("contract.sol");
    let other_file = include_str!("other_file.sol");

    let temp_dir = tempdir().expect("failed to create tempdir");
    std::fs::write(temp_dir.path().join("contract.sol"), contract)
        .expect("failed to copy contract");
    std::fs::write(temp_dir.path().join("other_file.sol"), other_file)
        .expect("failed to copy contract");

    temp_dir
}

#[test]
fn test_queries() {
    let temp_dir = get_temp_dir();

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(tree_sitter_solidity::language())
        .unwrap();

    let pub_query = (
        Query::new(
            tree_sitter_solidity::language(),
            r#"
            (member_expression
                object: (identifier) @obj
                (#match? @obj "msg")
                property: (identifier) @prop
                (#match? @prop "sender")
            ) @pub
            "#,
        )
        .unwrap(),
        2,
    );

    let hacky_query = (
        Query::new(
            tree_sitter_solidity::language(),
            r#"
        (call_expression
            function: (struct_expression
                type: (member_expression
                    property: (identifier) @hacky
                    (#match? @hacky "call")
                )
            )
        )
        "#,
        )
        .unwrap(),
        0,
    );

    let results = scanexr::utils::get_query_steps::<()>(
        temp_dir.path(),
        tree_sitter_solidity::language(),
        &pub_query,
    );
    let node_text = results
        .unwrap()
        .iter()
        .map(|step| {
            let tree = get_tree(step);
            let node = get_node(step, tree.root_node());
            let parent = node.parent().unwrap();
            format_node_step(&node, &parent, step)
        })
        .collect::<Vec<_>>()
        .join(",\n");

    insta::assert_snapshot!(node_text,
        @r###"

    got step with:
    node kind: "member_expression"
    parent: "array_access"
    context: ()
    line:

            balances[msg.sender] += msg.value;
                     ^^^^^^^^^^

    ,

    got step with:
    node kind: "member_expression"
    parent: "return_statement"
    context: ()
    line:

            return msg.sender;
                   ^^^^^^^^^^

    ,

    got step with:
    node kind: "member_expression"
    parent: "call_argument"
    context: ()
    line:

                return foo(0x0000000000000000000000000000000000000000, msg.sender);
                                                                       ^^^^^^^^^^

    ,

    got step with:
    node kind: "member_expression"
    parent: "call_argument"
    context: ()
    line:

                return ret2(0x0000000000000000000000000000000000000000, msg.sender);
                                                                        ^^^^^^^^^^

    "###
    );

    let results = scanexr::utils::get_query_steps::<()>(
        temp_dir.path(),
        tree_sitter_solidity::language(),
        &hacky_query,
    );
    let node_text = results
        .unwrap()
        .iter()
        .map(|step| {
            let tree = get_tree(step);
            let node = get_node(step, tree.root_node());
            let parent = node.parent().unwrap();
            format_node_step(&node, &parent, step)
        })
        .collect::<Vec<_>>()
        .join(",\n");

    insta::assert_snapshot!(node_text,
        @r###"

    got step with:
    node kind: "identifier"
    parent: "member_expression"
    context: ()
    line:

        (bool sent, ) = target.call{value: amount}("");
                               ^^^^

    "###
    );
}

#[tokio::test]
async fn test_solidity() {
    _test_solidity().await;
}

async fn _test_solidity() {
    let root_dir = get_temp_dir();
    let (lsp_client, handles) = lsp_client::clients::child_client(start_solidity_ls());

    lsp_client
        .request::<Initialize>(InitializeParams {
            root_uri: Some(Url::from_file_path(root_dir.path()).unwrap()),
            ..Default::default()
        })
        .await
        .unwrap()
        .unwrap();

    lsp_client
        .notify::<Initialized>(InitializedParams {})
        .unwrap();

    let pub_query = (
        Query::new(
            tree_sitter_solidity::language(),
            r#"
            (member_expression
                object: (identifier) @obj
                (#match? @obj "msg")
                property: (identifier) @prop
                (#match? @prop "sender")
            ) @pub
            "#,
        )
        .unwrap(),
        2,
    );

    let hacky_query = (
        Query::new(
            tree_sitter_solidity::language(),
            r#"
        (call_expression
            function: (struct_expression
                type: (member_expression
                    property: (identifier) @hacky
                    (#match? @hacky "call")
                )
            )
        )
        "#,
        )
        .unwrap(),
        0,
    );

    let tracer = scanexr::tracers::solidity::SolidityTracer;

    let stacktraces = scanexr::get_all_stacktraces(
        &tracer,
        &lsp_client,
        root_dir.path(),
        &[pub_query],
        &hacky_query,
    )
    .await
    .unwrap();

    fn format_context(ctx: StepContext) -> StepContext {
        match ctx {
            StepContext::GetReturnValue(mut anchor) => {
                anchor.path = anchor.path.file_name().unwrap().into();
                anchor.context = format_context(anchor.context);
                StepContext::GetReturnValue(anchor)
            }
            StepContext::GetReturnTupleValue(mut anchor, index) => {
                anchor.path = anchor.path.file_name().unwrap().into();
                anchor.context = format_context(anchor.context);
                StepContext::GetReturnTupleValue(anchor, index)
            }
            _ => ctx,
        }
    }

    let debug_stacktraces = stacktraces
        .into_iter()
        .map(|steps| {
            steps
                .into_iter()
                .map(|s| {
                    let path = s.path.file_name().unwrap().to_str().unwrap().to_string();
                    let source = String::from_utf8(std::fs::read(&s.path).unwrap()).unwrap();
                    let mut snippet = vec![];
                    let scroll = 5;
                    let start_line = s.start.line - scroll.min(s.start.line);
                    let end_line = s.end.line + scroll;
                    for (i, line) in source.lines().enumerate() {
                        if i < start_line || i > end_line {
                            continue;
                        }

                        snippet.push(line.to_string());
                        if i == s.start.line {
                            let mut pointer = " ".repeat(s.start.character)
                                + &"^".repeat(s.end.character - s.start.character);
                            pointer +=
                                &format!(" context: {:?}", format_context(s.context.clone()));
                            snippet.push(pointer);
                        }
                    }

                    let snippet = snippet.join("\n");
                    format!("# {path} #\n\n{snippet}")
                })
                .enumerate()
                .map(|(i, step_snippet)| format!("Step: {i}\n{step_snippet}\n"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .enumerate()
        .map(|(i, path_snippets)| format!("Stacktrace: {i}\n{path_snippets}\n"))
        .collect::<Vec<_>>()
        .join("\n");

    insta::assert_snapshot!(debug_stacktraces,
        @r###"
    Stacktrace: 0
    Step: 0
    # other_file.sol #

    function other(address a, uint b) pure returns (uint, address) {
        return (b, a);
    }

    function hacky(address target, uint amount) {
        (bool sent, ) = target.call{value: amount}("");
                        ^^^^^^ context: None
        require(sent, "Failed to send Ether");
    }

    Step: 1
    # other_file.sol #


    function other(address a, uint b) pure returns (uint, address) {
        return (b, a);
    }

    function hacky(address target, uint amount) {
                           ^^^^^^ context: None
        (bool sent, ) = target.call{value: amount}("");
        require(sent, "Failed to send Ether");
    }

    Step: 2
    # other_file.sol #


    function other(address a, uint b) pure returns (uint, address) {
        return (b, a);
    }

    function hacky(address target, uint amount) {
             ^^^^^ context: FindReference(0)
        (bool sent, ) = target.call{value: amount}("");
        require(sent, "Failed to send Ether");
    }

    Step: 3
    # contract.sol #

            address bar = foo(sender, sender);

            uint bal = balances[bar];
            require(bal > 0);

            hacky(bar, bal);
            ^^^^^ context: FindReference(0)

            balances[bar] = 0;
        }

        // Helper function to check the balance of this contract

    Step: 4
    # contract.sol #

            address bar = foo(sender, sender);

            uint bal = balances[bar];
            require(bal > 0);

            hacky(bar, bal);
                  ^^^ context: None

            balances[bar] = 0;
        }

        // Helper function to check the balance of this contract

    Step: 5
    # contract.sol #

        }

        function withdraw() public {
            address sender = getSender();

            address bar = foo(sender, sender);
                    ^^^ context: None

            uint bal = balances[bar];
            require(bal > 0);

            hacky(bar, bal);

    Step: 6
    # contract.sol #

        }

        function withdraw() public {
            address sender = getSender();

            address bar = foo(sender, sender);
                          ^^^^^^^^^^^^^^^^^^^ context: None

            uint bal = balances[bar];
            require(bal > 0);

            hacky(bar, bal);

    Step: 7
    # contract.sol #

        }

        function withdraw() public {
            address sender = getSender();

            address bar = foo(sender, sender);
                          ^^^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 66, character: 22 }, end: StepPosition { line: 66, character: 41 }, context: None })

            uint bal = balances[bar];
            require(bal > 0);

            hacky(bar, bal);

    Step: 8
    # contract.sol #

            (uint c, address d) = other(a2, 1);

            return d;
        }

        function foo(address a, address b) private pure returns (address) {
                 ^^^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 66, character: 22 }, end: StepPosition { line: 66, character: 41 }, context: None })
            return a;
        }

        function withdraw() public {
            address sender = getSender();

    Step: 9
    # contract.sol #


            return d;
        }

        function foo(address a, address b) private pure returns (address) {
            return a;
                   ^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 66, character: 22 }, end: StepPosition { line: 66, character: 41 }, context: None })
        }

        function withdraw() public {
            address sender = getSender();


    Step: 10
    # contract.sol #

            (uint c, address d) = other(a2, 1);

            return d;
        }

        function foo(address a, address b) private pure returns (address) {
                             ^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 66, character: 22 }, end: StepPosition { line: 66, character: 41 }, context: None })
            return a;
        }

        function withdraw() public {
            address sender = getSender();

    Step: 11
    # contract.sol #

        }

        function withdraw() public {
            address sender = getSender();

            address bar = foo(sender, sender);
                              ^^^^^^ context: None

            uint bal = balances[bar];
            require(bal > 0);

            hacky(bar, bal);

    Step: 12
    # contract.sol #

        function foo(address a, address b) private pure returns (address) {
            return a;
        }

        function withdraw() public {
            address sender = getSender();
                    ^^^^^^ context: None

            address bar = foo(sender, sender);

            uint bal = balances[bar];
            require(bal > 0);

    Step: 13
    # contract.sol #

        function foo(address a, address b) private pure returns (address) {
            return a;
        }

        function withdraw() public {
            address sender = getSender();
                             ^^^^^^^^^^^ context: None

            address bar = foo(sender, sender);

            uint bal = balances[bar];
            require(bal > 0);

    Step: 14
    # contract.sol #

        function foo(address a, address b) private pure returns (address) {
            return a;
        }

        function withdraw() public {
            address sender = getSender();
                             ^^^^^^^^^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 64, character: 25 }, end: StepPosition { line: 64, character: 36 }, context: None })

            address bar = foo(sender, sender);

            uint bal = balances[bar];
            require(bal > 0);

    Step: 15
    # contract.sol #


        function getSender2() private view returns (address) {
            return msg.sender;
        }

        function getSender() private view returns (address) {
                 ^^^^^^^^^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 64, character: 25 }, end: StepPosition { line: 64, character: 36 }, context: None })
            if (false) {
                return foo(0x0000000000000000000000000000000000000000, msg.sender);
            } else if (false) {
                return ret2(0x0000000000000000000000000000000000000000, msg.sender);
            }

    Step: 16
    # contract.sol #


        function getSender() private view returns (address) {
            if (false) {
                return foo(0x0000000000000000000000000000000000000000, msg.sender);
            } else if (false) {
                return ret2(0x0000000000000000000000000000000000000000, msg.sender);
                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 64, character: 25 }, end: StepPosition { line: 64, character: 36 }, context: None })
            }

            return getSender2();
        }


    Step: 17
    # contract.sol #


        function getSender() private view returns (address) {
            if (false) {
                return foo(0x0000000000000000000000000000000000000000, msg.sender);
            } else if (false) {
                return ret2(0x0000000000000000000000000000000000000000, msg.sender);
                       ^^^^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 47, character: 19 }, end: StepPosition { line: 47, character: 79 }, context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 64, character: 25 }, end: StepPosition { line: 64, character: 36 }, context: None }) })
            }

            return getSender2();
        }


    Step: 18
    # contract.sol #

            }

            return getSender2();
        }

        function ret2(address a, address a2) private pure returns (address) {
                 ^^^^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 47, character: 19 }, end: StepPosition { line: 47, character: 79 }, context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 64, character: 25 }, end: StepPosition { line: 64, character: 36 }, context: None }) })
            (uint c, address d) = other(a2, 1);

            return d;
        }


    Step: 19
    # contract.sol #

        }

        function ret2(address a, address a2) private pure returns (address) {
            (uint c, address d) = other(a2, 1);

            return d;
                   ^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 47, character: 19 }, end: StepPosition { line: 47, character: 79 }, context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 64, character: 25 }, end: StepPosition { line: 64, character: 36 }, context: None }) })
        }

        function foo(address a, address b) private pure returns (address) {
            return a;
        }

    Step: 20
    # contract.sol #


            return getSender2();
        }

        function ret2(address a, address a2) private pure returns (address) {
            (uint c, address d) = other(a2, 1);
                             ^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 47, character: 19 }, end: StepPosition { line: 47, character: 79 }, context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 64, character: 25 }, end: StepPosition { line: 64, character: 36 }, context: None }) })

            return d;
        }

        function foo(address a, address b) private pure returns (address) {

    Step: 21
    # contract.sol #


            return getSender2();
        }

        function ret2(address a, address a2) private pure returns (address) {
            (uint c, address d) = other(a2, 1);
                                  ^^^^^^^^^^^^ context: GetTupleValue(1)

            return d;
        }

        function foo(address a, address b) private pure returns (address) {

    Step: 22
    # contract.sol #


            return getSender2();
        }

        function ret2(address a, address a2) private pure returns (address) {
            (uint c, address d) = other(a2, 1);
                                  ^^^^^ context: GetReturnTupleValue(Step { path: "contract.sol", start: StepPosition { line: 54, character: 30 }, end: StepPosition { line: 54, character: 42 }, context: GetTupleValue(1) }, 1)

            return d;
        }

        function foo(address a, address b) private pure returns (address) {

    Step: 23
    # other_file.sol #

    // SPDX-License-Identifier: MIT
    pragma solidity ^0.8.17;

    function other(address a, uint b) pure returns (uint, address) {
             ^^^^^ context: GetReturnTupleValue(Step { path: "contract.sol", start: StepPosition { line: 54, character: 30 }, end: StepPosition { line: 54, character: 42 }, context: GetTupleValue(1) }, 1)
        return (b, a);
    }

    function hacky(address target, uint amount) {
        (bool sent, ) = target.call{value: amount}("");

    Step: 24
    # other_file.sol #

    // SPDX-License-Identifier: MIT
    pragma solidity ^0.8.17;

    function other(address a, uint b) pure returns (uint, address) {
        return (b, a);
               ^^^^^^ context: GetReturnTupleValue(Step { path: "contract.sol", start: StepPosition { line: 54, character: 30 }, end: StepPosition { line: 54, character: 42 }, context: GetTupleValue(1) }, 1)
    }

    function hacky(address target, uint amount) {
        (bool sent, ) = target.call{value: amount}("");
        require(sent, "Failed to send Ether");

    Step: 25
    # other_file.sol #

    // SPDX-License-Identifier: MIT
    pragma solidity ^0.8.17;

    function other(address a, uint b) pure returns (uint, address) {
        return (b, a);
                   ^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 54, character: 30 }, end: StepPosition { line: 54, character: 42 }, context: GetTupleValue(1) })
    }

    function hacky(address target, uint amount) {
        (bool sent, ) = target.call{value: amount}("");
        require(sent, "Failed to send Ether");

    Step: 26
    # other_file.sol #

    // SPDX-License-Identifier: MIT
    pragma solidity ^0.8.17;

    function other(address a, uint b) pure returns (uint, address) {
                           ^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 54, character: 30 }, end: StepPosition { line: 54, character: 42 }, context: GetTupleValue(1) })
        return (b, a);
    }

    function hacky(address target, uint amount) {
        (bool sent, ) = target.call{value: amount}("");

    Step: 27
    # contract.sol #


            return getSender2();
        }

        function ret2(address a, address a2) private pure returns (address) {
            (uint c, address d) = other(a2, 1);
                                        ^^ context: None

            return d;
        }

        function foo(address a, address b) private pure returns (address) {

    Step: 28
    # contract.sol #

            }

            return getSender2();
        }

        function ret2(address a, address a2) private pure returns (address) {
                                         ^^ context: None
            (uint c, address d) = other(a2, 1);

            return d;
        }


    Step: 29
    # contract.sol #

            }

            return getSender2();
        }

        function ret2(address a, address a2) private pure returns (address) {
                 ^^^^ context: FindReference(1)
            (uint c, address d) = other(a2, 1);

            return d;
        }


    Step: 30
    # contract.sol #


        function getSender() private view returns (address) {
            if (false) {
                return foo(0x0000000000000000000000000000000000000000, msg.sender);
            } else if (false) {
                return ret2(0x0000000000000000000000000000000000000000, msg.sender);
                       ^^^^ context: FindReference(1)
            }

            return getSender2();
        }


    Step: 31
    # contract.sol #


        function getSender() private view returns (address) {
            if (false) {
                return foo(0x0000000000000000000000000000000000000000, msg.sender);
            } else if (false) {
                return ret2(0x0000000000000000000000000000000000000000, msg.sender);
                                                                        ^^^^^^^^^^ context: None
            }

            return getSender2();
        }



    Stacktrace: 1
    Step: 0
    # other_file.sol #

    function other(address a, uint b) pure returns (uint, address) {
        return (b, a);
    }

    function hacky(address target, uint amount) {
        (bool sent, ) = target.call{value: amount}("");
                        ^^^^^^ context: None
        require(sent, "Failed to send Ether");
    }

    Step: 1
    # other_file.sol #


    function other(address a, uint b) pure returns (uint, address) {
        return (b, a);
    }

    function hacky(address target, uint amount) {
                           ^^^^^^ context: None
        (bool sent, ) = target.call{value: amount}("");
        require(sent, "Failed to send Ether");
    }

    Step: 2
    # other_file.sol #


    function other(address a, uint b) pure returns (uint, address) {
        return (b, a);
    }

    function hacky(address target, uint amount) {
             ^^^^^ context: FindReference(0)
        (bool sent, ) = target.call{value: amount}("");
        require(sent, "Failed to send Ether");
    }

    Step: 3
    # contract.sol #

            address bar = foo(sender, sender);

            uint bal = balances[bar];
            require(bal > 0);

            hacky(bar, bal);
            ^^^^^ context: FindReference(0)

            balances[bar] = 0;
        }

        // Helper function to check the balance of this contract

    Step: 4
    # contract.sol #

            address bar = foo(sender, sender);

            uint bal = balances[bar];
            require(bal > 0);

            hacky(bar, bal);
                  ^^^ context: None

            balances[bar] = 0;
        }

        // Helper function to check the balance of this contract

    Step: 5
    # contract.sol #

        }

        function withdraw() public {
            address sender = getSender();

            address bar = foo(sender, sender);
                    ^^^ context: None

            uint bal = balances[bar];
            require(bal > 0);

            hacky(bar, bal);

    Step: 6
    # contract.sol #

        }

        function withdraw() public {
            address sender = getSender();

            address bar = foo(sender, sender);
                          ^^^^^^^^^^^^^^^^^^^ context: None

            uint bal = balances[bar];
            require(bal > 0);

            hacky(bar, bal);

    Step: 7
    # contract.sol #

        }

        function withdraw() public {
            address sender = getSender();

            address bar = foo(sender, sender);
                          ^^^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 66, character: 22 }, end: StepPosition { line: 66, character: 41 }, context: None })

            uint bal = balances[bar];
            require(bal > 0);

            hacky(bar, bal);

    Step: 8
    # contract.sol #

            (uint c, address d) = other(a2, 1);

            return d;
        }

        function foo(address a, address b) private pure returns (address) {
                 ^^^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 66, character: 22 }, end: StepPosition { line: 66, character: 41 }, context: None })
            return a;
        }

        function withdraw() public {
            address sender = getSender();

    Step: 9
    # contract.sol #


            return d;
        }

        function foo(address a, address b) private pure returns (address) {
            return a;
                   ^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 66, character: 22 }, end: StepPosition { line: 66, character: 41 }, context: None })
        }

        function withdraw() public {
            address sender = getSender();


    Step: 10
    # contract.sol #

            (uint c, address d) = other(a2, 1);

            return d;
        }

        function foo(address a, address b) private pure returns (address) {
                             ^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 66, character: 22 }, end: StepPosition { line: 66, character: 41 }, context: None })
            return a;
        }

        function withdraw() public {
            address sender = getSender();

    Step: 11
    # contract.sol #

        }

        function withdraw() public {
            address sender = getSender();

            address bar = foo(sender, sender);
                              ^^^^^^ context: None

            uint bal = balances[bar];
            require(bal > 0);

            hacky(bar, bal);

    Step: 12
    # contract.sol #

        function foo(address a, address b) private pure returns (address) {
            return a;
        }

        function withdraw() public {
            address sender = getSender();
                    ^^^^^^ context: None

            address bar = foo(sender, sender);

            uint bal = balances[bar];
            require(bal > 0);

    Step: 13
    # contract.sol #

        function foo(address a, address b) private pure returns (address) {
            return a;
        }

        function withdraw() public {
            address sender = getSender();
                             ^^^^^^^^^^^ context: None

            address bar = foo(sender, sender);

            uint bal = balances[bar];
            require(bal > 0);

    Step: 14
    # contract.sol #

        function foo(address a, address b) private pure returns (address) {
            return a;
        }

        function withdraw() public {
            address sender = getSender();
                             ^^^^^^^^^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 64, character: 25 }, end: StepPosition { line: 64, character: 36 }, context: None })

            address bar = foo(sender, sender);

            uint bal = balances[bar];
            require(bal > 0);

    Step: 15
    # contract.sol #


        function getSender2() private view returns (address) {
            return msg.sender;
        }

        function getSender() private view returns (address) {
                 ^^^^^^^^^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 64, character: 25 }, end: StepPosition { line: 64, character: 36 }, context: None })
            if (false) {
                return foo(0x0000000000000000000000000000000000000000, msg.sender);
            } else if (false) {
                return ret2(0x0000000000000000000000000000000000000000, msg.sender);
            }

    Step: 16
    # contract.sol #

                return foo(0x0000000000000000000000000000000000000000, msg.sender);
            } else if (false) {
                return ret2(0x0000000000000000000000000000000000000000, msg.sender);
            }

            return getSender2();
                   ^^^^^^^^^^^^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 64, character: 25 }, end: StepPosition { line: 64, character: 36 }, context: None })
        }

        function ret2(address a, address a2) private pure returns (address) {
            (uint c, address d) = other(a2, 1);


    Step: 17
    # contract.sol #

                return foo(0x0000000000000000000000000000000000000000, msg.sender);
            } else if (false) {
                return ret2(0x0000000000000000000000000000000000000000, msg.sender);
            }

            return getSender2();
                   ^^^^^^^^^^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 50, character: 15 }, end: StepPosition { line: 50, character: 27 }, context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 64, character: 25 }, end: StepPosition { line: 64, character: 36 }, context: None }) })
        }

        function ret2(address a, address a2) private pure returns (address) {
            (uint c, address d) = other(a2, 1);


    Step: 18
    # contract.sol #


        function deposit() public payable {
            balances[msg.sender] += msg.value;
        }

        function getSender2() private view returns (address) {
                 ^^^^^^^^^^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 50, character: 15 }, end: StepPosition { line: 50, character: 27 }, context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 64, character: 25 }, end: StepPosition { line: 64, character: 36 }, context: None }) })
            return msg.sender;
        }

        function getSender() private view returns (address) {
            if (false) {

    Step: 19
    # contract.sol #

        function deposit() public payable {
            balances[msg.sender] += msg.value;
        }

        function getSender2() private view returns (address) {
            return msg.sender;
                   ^^^^^^^^^^ context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 50, character: 15 }, end: StepPosition { line: 50, character: 27 }, context: GetReturnValue(Step { path: "contract.sol", start: StepPosition { line: 64, character: 25 }, end: StepPosition { line: 64, character: 36 }, context: None }) })
        }

        function getSender() private view returns (address) {
            if (false) {
                return foo(0x0000000000000000000000000000000000000000, msg.sender);

    "###
    );

    for handle in handles {
        handle.abort()
    }
}
