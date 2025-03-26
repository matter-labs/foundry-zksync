//! Fork tests.

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use alloy_provider::Provider;
use forge::revm::primitives::SpecId;
use foundry_common::provider::try_get_zksync_http_provider;
use foundry_test_utils::{
    forgetest_async,
    util::{self, OutputExt},
    Filter, Fork, MockServer, ZkSyncNode,
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

forgetest_async!(test_zk_signal_bytecode_by_hash_retrieval_failure, |prj, cmd| {
    let node =
        ZkSyncNode::start_with_fork(Fork::new_with_block("mainnet".to_owned(), 0x3605436)).await;
    let mock_server = MockServer::builder()
        .expect(
            "zks_getBytecodeByHash",
            Some(serde_json::json!([
                "0x0100015d3d7d4b367021d7c7519afb343ee967aa37d9a89df298bf9fbfcaca0e"
            ])),
            serde_json::json!("force failure"),
        )
        .wrapping(node.url())
        .build();

    let rpc_url = mock_server.url();
    util::initialize(prj.root());

    prj.add_test(
        "UsePredeployedContract.t.sol",
        format!(
            r#"
import "forge-std/Test.sol";

contract UsePredeployedContract is Test {{
  string constant ZKSYNC_RPC_URL = "{rpc_url}";
  uint256 constant FORK_BLOCK = 56_644_662;

  address constant ZK_TOKEN_ADDRESS = 0x5A7d6b2F92C77FAD6CCaBd7EE0624E64907Eaf3E;

  function setUp() external {{
    uint256 _forkId = vm.createFork(vm.rpcUrl(ZKSYNC_RPC_URL), FORK_BLOCK);
    vm.selectFork(_forkId);
  }}

  function testUsePredeployedContract() public {{
    address alice = makeAddr("alice");
    deal(ZK_TOKEN_ADDRESS, alice, 10);
  }}
}}
"#
        )
        .as_str(),
    )
    .unwrap();

    cmd.args(["test", "--zksync", "--no-storage-caching", "--mc", "UsePredeployedContract"]);

    let output = cmd.assert_failure().get_output().stdout_lossy();
    assert!(output.contains("unable to obtain bytecode by hash from backend"));
});
