//! Forge tests for zksync contracts.

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use forge::revm::primitives::SpecId;
use foundry_config::fs_permissions::PathPermission;
use foundry_test_utils::Filter;

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_can_call_function() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new(
        "testZkContractCanCallMethod|testZkContractsMultipleTransactions",
        "ZkContractsTest",
        ".*",
    );

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_persisted_contracts_after_fork() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkContractsPersistedDeployedContractNoArgs|testZkContractsPersistedDeployedContractArgs", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_deployment() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkContractsInlineDeployedContractNoArgs|testZkContractsInlineDeployedContractComplexArgs", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_deployment_balance() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter =
        Filter::new("testZkContractsInlineDeployedContractBalance", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_deployment_balance_transfer() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkContractsExpectedBalances", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_create2() {
    let mut zk_config = TEST_DATA_DEFAULT.zk_test_data.zk_config.clone();
    zk_config.fs_permissions.add(PathPermission::read_write("./zk/zkout/ConstantNumber.sol"));
    let runner = TEST_DATA_DEFAULT.runner_with_zksync_config(zk_config);
    let filter = Filter::new("testZkContractsCreate2", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_can_call_system_contracts() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkContractsCallSystemContract", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_deployed_in_setup_can_be_mocked() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkContractsDeployedInSetupAreMockable", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_static_calls_keep_nonce_consistent() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkStaticCalls", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}
