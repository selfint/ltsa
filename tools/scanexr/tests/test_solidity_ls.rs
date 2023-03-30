use scanexr::{
    language_provider::{find_paths, LanguageProvider, LspProvider},
    languages::solidity::{Solidity, SolidityLs},
    test_utils::{display_locations, setup_test_dir},
    utils::visit_dirs,
};

#[tokio::test]
async fn test_find_definitions() {
    macro_rules! test_definitions {
        ($input:literal) => {
            let (root_dir, location, _, _) = scanexr::test_utils::setup_test_dir($input);

            let mut project_files = vec![];
            visit_dirs(root_dir.path(), &mut |f| project_files.push(f.path()))
                .expect("failed to get project files");

            let lsp = SolidityLs::new(root_dir.path(), project_files)
                .await
                .expect("failed to start solidity ls");

            let definitions = lsp
                .find_definitions(&location)
                .await
                .expect("failed to find definition");

            let definitions = definitions.into_iter().map(|d| (d, ())).collect();
            let snapshot = format!(
                r#"
### input ###
{}
### output ###
{}"#,
                $input,
                display_locations::<()>(definitions),
            );

            insta::assert_snapshot!(snapshot);
        };
    }

    test_definitions!(
        r#"
contract.sol
#@#
pragma solidity ^0.8.19;

contract Contract {
    function withdraw() public {
        address target = msg.sender;

        (bool sent, ) = target.call{value: 1}("");
                     // ^^^^^^ start
    }
}
        "#
    );

    test_definitions!(
        r#"
contract.sol
#@#
pragma solidity ^0.8.19;

contract Contract {
    function foo() public {}

    function withdraw() public {
        address target = msg.sender;
        
        foo();
    //  ^^^ start

        (bool sent, ) = target.call{value: 1}("");
    }
}
        "#
    );
}

#[tokio::test]
async fn test_find_references() {
    macro_rules! test_references {
        ($input:literal) => {
            let (root_dir, location, _, _) = scanexr::test_utils::setup_test_dir($input);

            let mut project_files = vec![];
            visit_dirs(root_dir.path(), &mut |f| project_files.push(f.path()))
                .expect("failed to get project files");

            let lsp = SolidityLs::new(root_dir.path(), project_files)
                .await
                .expect("failed to start solidity ls");

            let references = lsp
                .find_references(&location)
                .await
                .expect("failed to find references");

            let references = references.into_iter().map(|d| (d, ())).collect();
            let snapshot = format!(
                r#"
### input ###
{}
### output ###
{}"#,
                $input,
                display_locations::<()>(references),
            );

            insta::assert_snapshot!(snapshot);
        };
    }

    test_references!(
        r#"
contract.sol
#@#
pragma solidity ^0.8.19;

contract Contract {
    function foo() public {}
         //  ^^^ start

    function withdraw() public {
        address target = msg.sender;
        
        foo();
        foo();

        (bool sent, ) = target.call{value: 1}("");
    }
}
        "#
    );
}

#[tokio::test]
async fn test_contract() {
    let contract = include_str!("solidity/contract/contract.sol");
    let other_file = include_str!("solidity/contract/other_file.sol");
    let input = format!(
        r#"
contract.sol
#@#
{}
---
other_file.sol
#@#
{}
    "#,
        contract, other_file
    );

    let (root_dir, start, stop_at, _) = setup_test_dir(&input);
    let mut project_files = vec![];
    visit_dirs(root_dir.path(), &mut |f| project_files.push(f.path()))
        .expect("failed to get project files");
    let lsp = SolidityLs::new(root_dir.path(), project_files)
        .await
        .expect("failed to start solidity ls");
    let strategy = Solidity;

    let paths = find_paths(&strategy, &lsp, (start, strategy.initial_state()), &stop_at)
        .await
        .expect("failed to find paths");

    let path_strings = paths
        .into_iter()
        .enumerate()
        .map(|(i, path)| {
            let path = path.into_iter().map(|p| (p, ())).collect();
            format!("Path: {i}\n{}", display_locations(path))
        })
        .collect::<Vec<_>>()
        .join("\n---\n");

    let snapshot = format!("input:\n{input}\noutput:{path_strings}");

    insta::assert_snapshot!(snapshot);
}
