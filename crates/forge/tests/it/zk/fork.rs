//! Fork tests.

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use alloy_provider::Provider;
use forge::revm::primitives::SpecId;
use foundry_common::provider::try_get_zksync_http_provider;
use foundry_test_utils::{
    forgetest_async,
    util::{self},
    Filter, ZkSyncNode,
};
use foundry_zksync_core::state::{get_nonce_storage, new_full_nonce};

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_setup_fork_failure() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("test_ZkSetupForkFailureExecutesTest", "ZkSetupForkFailureTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).should_fail().run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_immutable_vars_persist_after_fork() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new(".*", "ZkForkImmutableVarsTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_consistent_storage_migration_after_fork() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new(".*", "ZkForkStorageMigrationTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

forgetest_async!(test_zk_consistent_nonce_migration_after_fork, |prj, cmd| {
    let test_address = alloy_primitives::address!("076d6da60aAAC6c97A8a0fE8057f9564203Ee545");
    let transaction_nonce = 2;
    let deployment_nonce = 1;

    let node = ZkSyncNode::start().await;
    // set nonce
    let (nonce_key_addr, nonce_key_slot) = get_nonce_storage(test_address);
    let full_nonce = new_full_nonce(transaction_nonce, deployment_nonce);
    let result = try_get_zksync_http_provider(node.url())
        .unwrap()
        .raw_request::<_, bool>(
            "anvil_setStorageAt".into(),
            (nonce_key_addr, nonce_key_slot, full_nonce),
        )
        .await
        .unwrap();
    assert!(result, "failed setting nonce on anvil-zksync");

    // prepare script
    util::initialize(prj.root());
    prj.add_script(
        "ZkForkNonceTest.s.sol",
        format!(r#"
import "forge-std/Script.sol";
import "forge-std/Test.sol";

interface VmExt {{
    function zkGetTransactionNonce(
        address account
    ) external view returns (uint64 nonce);
    function zkGetDeploymentNonce(
        address account
    ) external view returns (uint64 nonce);
}}

contract ZkForkNonceTest is Script {{
    VmExt internal constant vmExt = VmExt(VM_ADDRESS);

    address constant TEST_ADDRESS = {test_address};
    uint128 constant TEST_ADDRESS_TRANSACTION_NONCE = {transaction_nonce};
    uint128 constant TEST_ADDRESS_DEPLOYMENT_NONCE = {deployment_nonce};

    function run() external {{
        require(TEST_ADDRESS_TRANSACTION_NONCE == vmExt.zkGetTransactionNonce(TEST_ADDRESS), "failed matching transaction nonce");
        require(TEST_ADDRESS_DEPLOYMENT_NONCE == vmExt.zkGetDeploymentNonce(TEST_ADDRESS), "failed matching deployment nonce");
    }}
}}
"#).as_str(),
    )
    .unwrap();

    cmd.arg("script").args([
        "ZkForkNonceTest",
        "--zk-startup",
        "./script/ForkNonce.s.sol",
        "--no-storage-caching", // prevents rpc caching
        "--rpc-url",
        node.url().as_str(),
        // set address as sender to be migrated on startup, so storage is read immediately
        "--sender",
        test_address.to_string().as_str(),
    ]);

    cmd.assert_success();
});
