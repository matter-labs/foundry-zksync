//! Forge tests for zksync contracts.

use foundry_test_utils::{
    forgetest_async,
    util::{self, OutputExt},
    TestProject,
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
        "--no-commit",
        "--shallow",
    ])
    .assert_success();
    cmd.forge_fuse();

    let config = cmd.config();
    prj.write_config(config);

    prj.add_source("MyPaymaster.sol", include_str!("../../fixtures/zk/MyPaymaster.sol")).unwrap();
    prj.add_source("Paymaster.t.sol", include_str!("../../fixtures/zk/Paymaster.t.sol")).unwrap();

    cmd.args(["test", "--zk-startup", "--via-ir", "--match-contract", "TestPaymasterFlow"]);
    assert!(cmd.assert_success().get_output().stdout_lossy().contains("Suite result: ok"));
}

forgetest_async!(paymaster_script_test, |prj, cmd| {
    setup_deploy_prj(&mut prj);
    cmd.forge_fuse();
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/Paymaster.s.sol",
        "PaymasterScript",
        Some("OpenZeppelin/openzeppelin-contracts cyfrin/zksync-contracts"),
        3,
        Some(&["-vvvvv", "--via-ir"]),
    );
});

fn setup_deploy_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_script("Paymaster.s.sol", include_str!("../../fixtures/zk/Paymaster.s.sol")).unwrap();
    prj.add_source("MyPaymaster.sol", include_str!("../../fixtures/zk/MyPaymaster.sol")).unwrap();
    prj.add_source("Greeter.sol", include_str!("../../../../../testdata/zk/Greeter.sol")).unwrap();
}
