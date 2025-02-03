//! Fork tests.

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use forge::revm::primitives::SpecId;
use foundry_test_utils::{
    forgetest_async,
    util::{self, OutputExt},
    Filter, Fork, ZkSyncNode,
};

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_setup_fork_failure() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter =
        Filter::new("testFail_ZkSetupForkFailureExecutesTest", "ZkSetupForkFailureTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
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
    util::initialize(prj.root());

    // Has deployment nonce (1) and transaction nonce (2) on mainnet block #55159219
    let test_address = "0x076d6da60aAAC6c97A8a0fE8057f9564203Ee545";

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
    uint128 constant TEST_ADDRESS_TRANSACTION_NONCE = 2;
    uint128 constant TEST_ADDRESS_DEPLOYMENT_NONCE = 1;

    function run() external {{
        require(TEST_ADDRESS_TRANSACTION_NONCE == vmExt.zkGetTransactionNonce(TEST_ADDRESS), "failed matching transaction nonce");
        require(TEST_ADDRESS_DEPLOYMENT_NONCE == vmExt.zkGetDeploymentNonce(TEST_ADDRESS), "failed matching deployment nonce");
    }}
}}
"#).as_str(),
    )
    .unwrap();

    let node = ZkSyncNode::start_with_fork(Fork::new_with_block(
        String::from("https://mainnet.era.zksync.io"),
        55159219,
    ))
    .await;

    cmd.arg("script").args([
        "ZkForkNonceTest",
        "--zk-startup",
        "./script/ForkNonce.s.sol",
        "--rpc-url",
        node.url().as_str(),
        "--sender",
        test_address,
    ]);

    cmd.assert_success();
});
