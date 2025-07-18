//! Forge tests for zksync contracts.

use foundry_test_utils::{
    forgetest_async,
    util::{self, OutputExt},
    TestProject, ZkSyncNode,
};

use crate::test_helpers::run_zk_script_test;

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_paymaster() {
    let (prj, mut cmd) = util::setup_forge(
        "test_zk_contract_paymaster",
        foundry_test_utils::foundry_compilers::PathStyle::Dapptools,
    );
    util::initialize(prj.root());

    cmd.args([
        "install",
        "OpenZeppelin/openzeppelin-contracts",
        "cyfrin/zksync-contracts",
        "--shallow",
    ])
    .assert_success();
    cmd.forge_fuse();

    let config = cmd.config();
    prj.write_config(config);

    prj.add_source("MyPaymaster.sol", include_str!("../../fixtures/zk/MyPaymaster.sol")).unwrap();
    prj.add_source("Paymaster.t.sol", include_str!("../../fixtures/zk/Paymaster.t.sol")).unwrap();

    cmd.args([
        "test",
        "--zk-startup",
        "--via-ir",
        "--match-contract",
        "TestPaymasterFlow",
        "--optimize",
        "true",
    ]);
    assert!(cmd.assert_success().get_output().stdout_lossy().contains("Suite result: ok"));
}

// Tests the deployment of contracts using a paymaster for fee abstraction
forgetest_async!(test_zk_deploy_with_paymaster, |prj, cmd| {
    setup_deploy_prj(&mut prj);
    let node = ZkSyncNode::start().await;
    let url = node.url();

    let private_key =
        ZkSyncNode::rich_wallets().next().map(|(_, pk, _)| pk).expect("No rich wallets available");

    // Install required dependencies
    cmd.args([
        "install",
        "OpenZeppelin/openzeppelin-contracts",
        "cyfrin/zksync-contracts",
        "--shallow",
    ])
    .assert_success();
    cmd.forge_fuse();

    // Deploy the paymaster contract first
    let paymaster_deployment = cmd
        .forge_fuse()
        .args([
            "create",
            "./src/MyPaymaster.sol:MyPaymaster",
            "--rpc-url",
            url.as_str(),
            "--private-key",
            private_key,
            "--via-ir",
            "--value",
            "1000000000000000000",
            "--zksync",
        ])
        .assert_success()
        .get_output()
        .stdout_lossy();

    // Extract the deployed paymaster address
    let re = regex::Regex::new(r"Deployed to: (0x[a-fA-F0-9]{40})").unwrap();
    let paymaster_address = re
        .captures(&paymaster_deployment)
        .and_then(|caps| caps.get(1))
        .map(|addr| addr.as_str())
        .expect("Failed to extract paymaster address");

    // Test successful deployment with valid paymaster input
    let greeter_deployment = cmd.forge_fuse()
        .args([
            "create",
            "./src/Greeter.sol:Greeter",
            "--rpc-url",
            url.as_str(),
            "--private-key",
            private_key,
            "--zk-paymaster-address",
            paymaster_address,
            "--zk-paymaster-input",
            "0x8c5a344500000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000",
            "--via-ir",
            "--zksync"
        ])
        .assert_success()
        .get_output()
        .stdout_lossy();

    // Verify successful deployment
    assert!(greeter_deployment.contains("Deployed to:"));

    // Test deployment failure with invalid paymaster input
    cmd.forge_fuse()
        .args([
            "create",
            "./src/Greeter.sol:Greeter",
            "--rpc-url",
            url.as_str(),
            "--private-key",
            private_key,
            "--zk-paymaster-address",
            paymaster_address,
            "--zk-paymaster-input",
            "0x0000000000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000",
            "--via-ir",
            "--zksync"
        ])
        .assert_failure();
});

forgetest_async!(paymaster_script_test, |prj, cmd| {
    setup_deploy_prj(&mut prj);
    cmd.forge_fuse();
    // We added the optimizer flag which is now false by default so we need to set it to true
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/Paymaster.s.sol",
        "PaymasterScript",
        Some("OpenZeppelin/openzeppelin-contracts cyfrin/zksync-contracts"),
        3,
        Some(&["-vvvvv", "--via-ir", "--optimize", "true"]),
    )
    .await;
});

fn setup_deploy_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_script("Paymaster.s.sol", include_str!("../../fixtures/zk/Paymaster.s.sol")).unwrap();
    prj.add_source("MyPaymaster.sol", include_str!("../../fixtures/zk/MyPaymaster.sol")).unwrap();
    prj.add_source("Greeter.sol", include_str!("../../../../../testdata/zk/Greeter.sol")).unwrap();
}
