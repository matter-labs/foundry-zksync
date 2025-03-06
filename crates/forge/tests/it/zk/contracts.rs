//! Forge tests for zksync contracts.

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use forge::revm::primitives::SpecId;
use foundry_config::fs_permissions::PathPermission;
use foundry_test_utils::{
    util::{self, OutputExt},
    Filter,
};

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_can_call_function() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new(
        "testZkContractCanCallMethod|testZkContractsMultipleTransactions",
        "ZkContractsTest",
        ".*",
    );

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_persisted_contracts_after_fork() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter =
        Filter::new("testZkContractsPersistedDeployedContractNoArgs|testZkContractsPersistedDeployedContractArgs", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_deployment() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkContractsInlineDeployedContractNoArgs|testZkContractsInlineDeployedContractComplexArgs", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_deployment_balance() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter =
        Filter::new("testZkContractsInlineDeployedContractBalance", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_deployment_balance_transfer() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkContractsExpectedBalances", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_create2() {
    let (prj, mut cmd) = util::setup_forge(
        "test_zk_contract_create2_with_deps",
        foundry_test_utils::foundry_compilers::PathStyle::Dapptools,
    );
    util::initialize(prj.root());

    cmd.args(["install", "matter-labs/era-contracts", "--shallow"]).assert_success();
    cmd.forge_fuse();

    let mut config = cmd.config();
    config.fs_permissions.add(PathPermission::read("./zkout"));
    prj.write_config(config);

    prj.add_source("Greeter.sol", include_str!("../../../../../testdata/zk/Greeter.sol")).unwrap();

    prj.add_source("CustomNumber.sol", include_str!("../../../../../testdata/zk/CustomNumber.sol"))
        .unwrap();

    prj.add_source("Create2Utils.sol", include_str!("../../../../../testdata/zk/Create2Utils.sol"))
        .unwrap();

    prj.add_test("Create2.t.sol", include_str!("../../fixtures/zk/Create2.t.sol")).unwrap();

    cmd.args([
        "test",
        "--zk-startup",
        "--evm-version",
        "shanghai",
        "--mc",
        "Create2Test",
        "--optimize",
        "true",
    ]);
    cmd.assert_success().get_output().stdout_lossy().contains("Suite result: ok");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_can_call_system_contracts() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkContractsCallSystemContract", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_deployed_in_setup_can_be_mocked() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkContractsDeployedInSetupAreMockable", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_static_calls_keep_nonce_consistent() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkStaticCalls", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}
