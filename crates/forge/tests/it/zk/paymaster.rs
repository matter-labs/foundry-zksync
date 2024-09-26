//! Forge tests for zksync contracts.

use foundry_test_utils::util;

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
    .ensure_execute_success()
    .expect("able to install dependencies");

    cmd.forge_fuse();

    let config = cmd.config();
    prj.write_config(config);

    prj.add_source("MyPaymaster.sol", include_str!("../../fixtures/zk/MyPaymaster.sol")).unwrap();
    prj.add_source("Paymaster.t.sol", include_str!("../../fixtures/zk/Paymaster.t.sol")).unwrap();

    cmd.args(["test", "--zk-startup", "--via-ir", "--match-contract", "TestPaymasterFlow"]);
    assert!(cmd.stdout_lossy().contains("Suite result: ok"));
}
