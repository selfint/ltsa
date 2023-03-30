use std::process::Stdio;

use lsp_types::{notification::*, request::*, *};
use scanexr::{language_provider::LspProvider, languages::solidity::SolidityLs, utils::visit_dirs};
use tempfile::{tempdir, TempDir};
use tokio::process::{Child, Command};
use tree_sitter::Query;

fn get_temp_dir() -> TempDir {
    let contract = include_str!("contract/contract.sol");
    let other_file = include_str!("contract/other_file.sol");

    let temp_dir = tempdir().expect("failed to create tempdir");
    std::fs::create_dir(temp_dir.path().join("contract")).expect("failed to create dir");

    std::fs::write(
        temp_dir.path().join("contract").join("contract.sol"),
        contract,
    )
    .expect("failed to copy contract");
    std::fs::write(
        temp_dir.path().join("contract").join("other_file.sol"),
        other_file,
    )
    .expect("failed to copy contract");

    temp_dir
}

#[tokio::test]
async fn test_solidity() {
    _test_solidity().await;
}

async fn _test_solidity() {
    let (root_dir, location, _, _) = scanexr::test_utils::setup_test_dir(
        r#"
contract.sol
#@#
contract Contract {
    function withdraw() public {
        uint bal = balances[msg.sender];
        require(bal > 0);

        address target = msg.sender;

        (bool sent, ) = target.call{value: bal}("");
                     // ^^^^^^ start
        balances[msg.sender] = 0;
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
}
