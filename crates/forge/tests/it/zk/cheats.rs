//! Forge tests for cheatcodes.

use std::path::Path;

use crate::{
    config::*,
    test_helpers::{run_zk_script_test, TEST_DATA_DEFAULT},
};
use forge::revm::primitives::SpecId;
use foundry_config::{fs_permissions::PathPermission, Config, FsPermissions};
use foundry_test_utils::{
    forgetest_async,
    util::{self},
    Filter, TestProject, ZkSyncNode,
};

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheat_roll_works() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkCheatcodesRoll", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheat_get_code() {
    let mut zk_config = TEST_DATA_DEFAULT.zk_test_data.zk_config.clone();
    zk_config.fs_permissions.add(PathPermission::read("./zk"));

    let runner = TEST_DATA_DEFAULT.runner_with_zksync_config(zk_config);
    let filter = Filter::new("testZkCheatcodesGetCode", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheat_warp_works() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkCheatcodesWarp", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheat_deal_works() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkCheatcodesDeal", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheat_set_nonce_works() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkCheatcodesSetNonce", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheat_etch_works() {
    let mut zk_config = TEST_DATA_DEFAULT.zk_test_data.zk_config.clone();
    zk_config.fs_permissions.add(PathPermission::read_write("./zk/zkout/ConstantNumber.sol"));
    let runner = TEST_DATA_DEFAULT.runner_with_zksync_config(zk_config);
    let filter = Filter::new("testZkCheatcodesEtch", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheat_record_works() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testRecord", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheat_expect_emit_works() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testExpectEmit", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheat_expect_revert_works() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("test(ExpectRevert$|ExpectRevertFails)", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheat_expect_revert_works_with_nested_call_reverts() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testExpectRevertWithNestedCallReverts", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheat_expect_revert_works_with_nested_create_reverts() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testExpectRevertWithNestedCreateReverts", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheat_expect_revert_works_with_internal_reverts() {
    let mut runner = TEST_DATA_DEFAULT.runner_zksync();
    let mut config = runner.config.as_ref().clone();
    config.allow_internal_expect_revert = true;
    runner.config = std::sync::Arc::new(config);
    let filter = Filter::new(
        "testExpectRevertDeeperDepthsWithInternalRevertsEnabled",
        "ZkCheatcodesTest",
        ".*",
    );

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheat_expect_call_works() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testExpectCall", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheat_mock_with_value_function() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkCheatcodesValueFunctionMockReturn", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheat_mock_calls() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new(
        "testZkCheatcodesCanMockCallTestContract|testZkCheatcodesCanMockCall",
        "ZkCheatcodesTest",
        ".*",
    );

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheat_works_after_fork() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkCheatcodesCanBeUsedAfterFork", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_eravm_force_return_feature() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new(".*", "ZkRetTest", ".*");
    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_can_mock_modifiers() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new(".*", "MockedModifierTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_record_logs() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("RecordLogs", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheatcodes_in_zkvm() {
    let mut runner = TEST_DATA_DEFAULT.runner_zksync();
    let mut config = runner.config.as_ref().clone();
    // This is now false by default so in order to expect a revert from an internal call, we need to
    // set it to true https://github.com/foundry-rs/foundry/pull/9537
    config.allow_internal_expect_revert = true;
    runner.config = std::sync::Arc::new(config);
    let filter = Filter::new(".*", "ZkCheatcodesInZkVmTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_zk_vm_skip_works() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new(".*", "ZkCheatcodeZkVmSkipTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_state_diff_works() {
    let mut runner = TEST_DATA_DEFAULT.runner_zksync();
    let mut config = runner.config.as_ref().clone();
    config.fs_permissions =
        FsPermissions::new(vec![PathPermission::read(Path::new("zk/zkout/Bank.sol/Bank.json"))]);
    runner.config = std::sync::Arc::new(config);
    let filter = Filter::new(".*", "ZkStateDiffTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

forgetest_async!(test_zk_use_factory_dep, |prj, cmd| {
    setup_deploy_prj(&mut prj);

    cmd.forge_fuse();
    // We added the optimizer flag which is now false by default so we need to set it to true
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/DeployCounterWithBytecodeHash.s.sol",
        "DeployCounterWithBytecodeHash",
        Some("transmissions11/solmate@v7 OpenZeppelin/openzeppelin-contracts cyfrin/zksync-contracts"),
        2,
        Some(&["-vvvvv", "--via-ir", "--system-mode", "true", "--broadcast", "--optimize", "true"]),
    ).await;
});

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_broadcast_raw_executes() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testBroadcastTX", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

forgetest_async!(script_zk_broadcast_raw_in_output_json, |prj, cmd| {
    util::initialize(prj.root());
    let node = ZkSyncNode::start().await;
    let (_, private_key) = ZkSyncNode::rich_wallets()
        .next()
        .map(|(addr, pk, _)| (addr, pk))
        .expect("No rich wallets available");

    prj.add_source(
        "Counter.sol",
        r#"
pragma solidity ^0.8.0;

contract Counter {
    uint256 public count;
    function increment() external {
        count++;
    }
}
    "#,
    )
    .unwrap();

    prj.add_script(
    "SimpleScript.s.sol",
    r#"
import "forge-std/Script.sol";
import {Counter} from "../src/Counter.sol";
contract SimpleScript is Script {
    function run() external {
        // zk raw transaction
        vm.startBroadcast();
        
        // cast mktx --rpc-url &url --private-key &private_key --zksync --create  "0x000000800300003900 .... " // <-- The bytecode comes from the the Counter contract above
        vm.broadcastRawTransaction(
        hex"71f903ae80808402b275d0834020b694000000000000000000000000000000000000800680b8849c4d535b000000000000000000000000000000000000000000000000000000000000000001000015eef9c25753fdda281d958bddd70867399645894ec102744c554999fd0000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000080a005820228fcbfc5a16562ad38e4f7976de211f827c9f8b554f0f4587002d3cd3ba05de935fcae8d2e93fe31029a3c1cb9f6d3c6255e979917e030f5731f6383495782010494bc989fde9e54cad2ab4392af6df60f04873a033a830f4240f902a3b902a00000008003000039000000400030043f0000000100200190000000130000c13d0000000d00100198000000270000613d000000000101043b000000e0011002700000000e0010009c0000001b0000613d0000000f0010009c000000270000c13d0000000001000416000000000001004b000000270000c13d000000000100041a000000800010043f00000012010000410000002d0001042e0000000001000416000000000001004b000000270000c13d0000002001000039000001000010044300000120000004430000000c010000410000002d0001042e0000000001000416000000000001004b000000270000c13d000000000100041a000000010110003a000000290000c13d0000001001000041000000000010043f0000001101000039000000040010043f00000011010000410000002e0001043000000000010000190000002e00010430000000000010041b00000000010000190000002d0001042e0000002c000004320000002d0001042e0000002e000104300000000000000000000000020000000000000000000000000000004000000100000000000000000000000000000000000000000000000000fffffffc00000000000000000000000000000000000000000000000000000000000000000000000000000000d09de08a0000000000000000000000000000000000000000000000000000000006661abd4e487b7100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002400000000000000000000000000000000000000000000000000000000000000200000008000000000000000000000000000000000000000000000000000000000000000000000000000000000b34d28be4c2607700146bb47d0b91f49fda73afa4b2b4cccde86e0ea842d71ed8080"
        );

        Counter counter = Counter(0x9c1a3d7C98dBF89c7f5d167F2219C29c2fe775A7);
        uint256 prev = counter.count();
        require(prev == 0);

        // `cast mktx "0x9c1a3d7C98dBF89c7f5d167F2219C29c2fe775A7" "increment()" --rpc-url http://127.0.0.1:49204 --private-key "0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e" --zksync --nonce 1`
        vm.broadcastRawTransaction(
            hex"71f88501808402b275d08304d0e4949c1a3d7c98dbf89c7f5d167f2219c29c2fe775a78084d09de08a80a0bda10084650ce446c1c22d72a98ed4489040c7702a6ea97d5e9f5ec3bb76a55da04c97363c5491dedd78dd955f2903338f75325836218f59cf62e9acdf454bbf3082010494bc989fde9e54cad2ab4392af6df60f04873a033a80c08080"            
        );

        require(counter.count() == prev + 1);

        vm.stopBroadcast();
    }
}
"#,
    )
    .unwrap();

    cmd.forge_fuse();

    cmd.args([
        "script",
        "-vvvvv",
        "--zksync",
        "--private-key",
        private_key,
        "--rpc-url",
        node.url().as_str(),
        "--broadcast",
        "--slow",
        "--non-interactive",
        "SimpleScript",
    ])
    .assert_success();

    let run_latest = foundry_common::fs::json_files(prj.root().join("broadcast").as_path())
        .find(|file| file.ends_with("run-latest.json"))
        .expect("No broadcast artifacts");

    let content = foundry_common::fs::read_to_string(run_latest).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    let txns = json["transactions"].as_array().expect("broadcastable txs");

    let deployment = &txns[0];
    deployment["contractAddress"]
        .as_str()
        .expect("contract address key was not a string")
        .contains("0x0000000000000000000000000000000000008006");
    deployment["transaction"]["from"]
        .as_str()
        .expect("from key was not a string")
        .contains("0xbc989fde9e54cad2ab4392af6df60f04873a033a");

    let zksync =
        deployment["transaction"]["zksync"].as_object().expect("zksync key was not an object");
    let factory_deps = zksync["factoryDeps"].as_array().expect("factoryDeps was not an array");
    assert!(!factory_deps.is_empty());

    let broadcasted = &txns[1];

    // check that the broadcasted tx have the correct function and contract address
    assert_eq!(txns.len(), 2);
    broadcasted["function"]
        .as_str()
        .expect("function name key was not a string")
        .contains("increment");
    broadcasted["contractAddress"]
        .as_str()
        .expect("contract address key was not a string")
        .contains("0x9c1a3d7c98dbf89c7f5d167f2219c29c2fe775a7");

    broadcasted["transaction"]["from"]
        .as_str()
        .expect("from was key not a string")
        .contains("0xbc989fde9e54cad2ab4392af6df60f04873a033a");
    broadcasted["transaction"]["to"]
        .as_str()
        .expect("to was not key a string")
        .contains("0x9c1a3d7c98dbf89c7f5d167f2219c29c2fe775a7");
    broadcasted["transaction"]["value"].as_str().expect("value was not a string").contains("0x0");
});

fn setup_deploy_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    let permissions = FsPermissions::new(vec![
        PathPermission::read(Path::new("zkout/Counter.sol/Counter.json")),
        PathPermission::read(Path::new("zkout/Factory.sol/Factory.json")),
    ]);
    let config = Config { fs_permissions: permissions, ..Default::default() };
    prj.write_config(config);
    prj.add_script(
        "DeployCounterWithBytecodeHash.s.sol",
        include_str!("../../fixtures/zk/DeployCounterWithBytecodeHash.s.sol"),
    )
    .unwrap();
    prj.add_source("Factory.sol", include_str!("../../fixtures/zk/Factory.sol")).unwrap();
    prj.add_source("Counter", "contract Counter {}").unwrap();
}
