//! Forge tests for cheatcodes.

use std::path::Path;

use crate::{
    config::*,
    test_helpers::{run_zk_script_test, TEST_DATA_DEFAULT},
};
use forge::revm::primitives::SpecId;
use foundry_config::{fs_permissions::PathPermission, Config, FsPermissions};
use foundry_test_utils::{forgetest_async, util, Filter, TestProject};

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
