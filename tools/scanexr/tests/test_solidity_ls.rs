use scanexr::{
    language_provider::LspProvider, languages::solidity::SolidityLs, test_utils::display_locations,
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
