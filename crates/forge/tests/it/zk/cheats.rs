//! Forge tests for cheatcodes.

// use foundry_zksync_core::utils::MAX_L2_GAS_LIMIT;
use std::path::Path;

use crate::{
    config::*,
    test_helpers::{run_zk_script_test, TEST_DATA_DEFAULT},
};
use forge::revm::primitives::SpecId;
use foundry_config::{fs_permissions::PathPermission, Config, FsPermissions};
use foundry_test_utils::{
    forgetest_async,
    util::{self, OutputExt},
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

forgetest_async!(test_zk_broadcast_raw_create2_deployer, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    let node = ZkSyncNode::start().await;
    let url = node.url();

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

    //deploy
    let _ = cmd
        .args([
            "create",
            "src/Counter.sol:Counter",
            "--zksync",
            "--private-key",
            private_key,
            "--rpc-url",
            &url,
        ])
        .assert_success()
        .get_output()
        .stdout_lossy();

    cmd.forge_fuse();

    prj.add_script(
        "Foo",
        r#"
import "forge-std/Script.sol";
import {Counter} from "../src/Counter.sol";
contract SimpleScript is Script {
    function run() external {
        // zk raw transaction
        vm.startBroadcast();
        // This raw transaction comes from cast mktx of increment() to Counter contract
        // `cast mktx "0x9086C95769C51E15D6a77672251Cf13Ce7ebf3AE" "increment()" --rpc-url http://127.0.0.1:49204 --private-key "0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e" --zksync --nonce 1`
        vm.broadcastRawTransaction(
            hex"71f88501808402b275d08304d718949086c95769c51e15d6a77672251cf13ce7ebf3ae8084d09de08a01a00d798053cc7a75a78d49adc0b893d9ad91f5301bb264eb2848979859fc366dc7a06469714a3ec85003c3f52ff668299c3c01e0a43be5ec0529cc204fd53be1629e82010494bc989fde9e54cad2ab4392af6df60f04873a033a80c08080"
        );
        vm.stopBroadcast();
    }
}
"#,
    )
    .unwrap();

    cmd.args([
        "script",
        "--zksync",
        "--private-key",
        private_key,
        "--rpc-url",
        &url,
        "--broadcast",
        "--slow",
        "--non-interactive",
        "SimpleScript",
    ]);

    let output = cmd.assert_success().get_output().stdout_lossy();
    assert!(output.contains("ONCHAIN EXECUTION COMPLETE & SUCCESSFUL."));
});

forgetest_async!(script_zk_broadcast_raw_create2_deployer, |prj, cmd| {
    util::initialize(prj.root());

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

    let node = ZkSyncNode::start().await;
    let (_, private_key) = ZkSyncNode::rich_wallets()
        .next()
        .map(|(addr, pk, _)| (addr, pk))
        .expect("No rich wallets available");

    let _ = cmd
        .args([
            "create",
            "src/Counter.sol:Counter",
            "--zksync",
            "--private-key",
            private_key,
            "--rpc-url",
            node.url().as_str(),
        ])
        .assert_success()
        .get_output()
        .stdout_lossy();

    cmd.forge_fuse();

    prj.add_script(
        "SimpleScript.s.sol",
        r#"
import "forge-std/Script.sol";
import {Counter} from "../src/Counter.sol";
contract SimpleScript is Script {
    function run() external {
        // zk raw transaction
        vm.startBroadcast();
        // This raw transaction comes from cast mktx of increment() to Counter contract
        // `cast mktx "0x9086C95769C51E15D6a77672251Cf13Ce7ebf3AE" "increment()" --rpc-url http://127.0.0.1:49204 --private-key "0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e" --zksync --nonce 1`
        vm.broadcastRawTransaction(
            hex"71f88501808402b275d08304d718949086c95769c51e15d6a77672251cf13ce7ebf3ae8084d09de08a01a00d798053cc7a75a78d49adc0b893d9ad91f5301bb264eb2848979859fc366dc7a06469714a3ec85003c3f52ff668299c3c01e0a43be5ec0529cc204fd53be1629e82010494bc989fde9e54cad2ab4392af6df60f04873a033a80c08080"
        );
        vm.stopBroadcast();
    }
}
"#,
    )
    .unwrap();

    cmd.args([
        "script",
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
    .assert_success()
    .get_output()
    .stdout_lossy();

    let run_latest = foundry_common::fs::json_files(prj.root().join("broadcast").as_path())
        .find(|file| file.ends_with("run-latest.json"))
        .expect("No broadcast artifacts");

    let content = foundry_common::fs::read_to_string(run_latest).unwrap();

    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    let txns = json["transactions"].as_array().expect("broadcastable txs");
    
    // check that the txs have the correct function and contract address
    assert_eq!(txns.len(), 1);
    txns[0]["function"].as_str().expect("function name").contains("increment");
    txns[0]["contractAddress"]
        .as_str()
        .expect("contract address")
        .contains("0x9086c95769c51e15d6a77672251cf13ce7ebf3ae");

    // check that the txs have strictly monotonically increasing nonces
    assert!(txns.iter().filter_map(|tx| tx["nonce"].as_u64()).is_sorted_by(|a, b| a + 1 == *b));
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
