use scanexr::{
    language_provider::LspProvider, languages::solidity::SolidityLs, test_utils::display_locations,
    utils::visit_dirs,
};
use tempfile::{tempdir, TempDir};

#[tokio::test]
async fn test_solidity() {
    _test_solidity().await;
}

async fn _test_solidity() {
    let (root_dir, location, _, _) = scanexr::test_utils::setup_test_dir(
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
        "#,
    );

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

    insta::assert_snapshot!(display_locations::<()>(definitions),
        @r###"
    contract.sol
    #@#

    pragma solidity ^0.8.19;

    contract Contract {
        function withdraw() public {
            address target = msg.sender;
                    ^^^^^^ Meta: ()

            (bool sent, ) = target.call{value: 1}("");
                         // ^^^^^^ start
        }
    }
            
    "###
    );
}
