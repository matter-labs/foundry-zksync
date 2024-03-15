//! Forge tests for cheatcodes.

use std::collections::BTreeMap;

use crate::{
    config::*,
    test_helpers::{PROJECT, RE_PATH_SEPARATOR},
};
use forge::revm::primitives::SpecId;
use foundry_config::{fs_permissions::PathPermission, Config, FsPermissions};
use foundry_test_utils::Filter;

/// Executes all zk basic tests
#[tokio::test(flavor = "multi_thread")]
async fn test_zk_basic() {
    let mut config = Config::with_root(PROJECT.root());
    config.fs_permissions = FsPermissions::new(vec![PathPermission::read_write("./")]);
    let runner = runner_with_config_and_zk(config);
    let filter = Filter::new(".*", "ZkBasicTest", &format!(".*zk{RE_PATH_SEPARATOR}*"));

    TestConfig::with_filter(runner.await, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

/// Executes all zk contract tests
#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contracts() {
    let mut config = Config::with_root(PROJECT.root());
    config.fs_permissions = FsPermissions::new(vec![PathPermission::read_write("./")]);
    let runner = runner_with_config_and_zk(config);
    let filter = Filter::new(".*", "ZkContractsTest", &format!(".*zk{RE_PATH_SEPARATOR}*"));

    TestConfig::with_filter(runner.await, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

/// Executes all zk cheatcode tests
#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheats() {
    let mut config = Config::with_root(PROJECT.root());
    config.fs_permissions = FsPermissions::new(vec![PathPermission::read_write("./")]);
    let runner = runner_with_config_and_zk(config);
    let filter = Filter::new(".*", "ZkCheatcodesTest", &format!(".*zk{RE_PATH_SEPARATOR}*"));

    TestConfig::with_filter(runner.await, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

/// Executes all zk console tests
#[tokio::test(flavor = "multi_thread")]
async fn test_zk_logs() {
    let mut config = Config::with_root(PROJECT.root());
    config.fs_permissions = FsPermissions::new(vec![PathPermission::read_write("./")]);
    let runner = runner_with_config_and_zk(config);
    let filter = Filter::new(".*", "ZkConsoleTest", &format!(".*zk{RE_PATH_SEPARATOR}*"));

    let results =
        TestConfig::with_filter(runner.await, filter).evm_spec(SpecId::SHANGHAI).test().await;

    assert_multiple(
        &results,
        BTreeMap::from([(
            "zk/Console.t.sol:ZkConsoleTest",
            vec![(
                "testZkConsoleOutput()",
                true,
                None,
                Some(vec![
                    "print".into(),
                    "outer print".into(),
                    "0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496".into(),
                    "print".into(),
                    "0xff".into(),
                    "print".into(),
                ]),
                None,
            )],
        )]),
    );
}
