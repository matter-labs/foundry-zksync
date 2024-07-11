//! Forge tests for cheatcodes.

use std::collections::BTreeMap;

use crate::{
    config::*,
    test_helpers::{RE_PATH_SEPARATOR, TEST_DATA_DEFAULT},
};
use forge::revm::primitives::SpecId;
use foundry_config::{fs_permissions::PathPermission, FsPermissions};
use foundry_test_utils::Filter;

/// Executes all zk basic tests
#[tokio::test(flavor = "multi_thread")]
async fn test_zk_basic() {
    let mut config = TEST_DATA_DEFAULT.config.clone();
    config.fs_permissions = FsPermissions::new(vec![PathPermission::read_write("./")]);
    let runner = TEST_DATA_DEFAULT.runner_with_zksync_config(config);
    let filter = Filter::new(".*", "ZkBasicTest", &format!(".*zk{RE_PATH_SEPARATOR}*"));

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

/// Executes all zk contract tests
#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contracts() {
    let mut config = TEST_DATA_DEFAULT.config.clone();
    config.fs_permissions = FsPermissions::new(vec![PathPermission::read_write("./")]);
    let runner = TEST_DATA_DEFAULT.runner_with_zksync_config(config);
    let filter = Filter::new(".*", "ZkContractsTest", &format!(".*zk{RE_PATH_SEPARATOR}*"));

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

/// Executes all zk cheatcode tests
#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheats() {
    let mut config = TEST_DATA_DEFAULT.config.clone();
    config.fs_permissions = FsPermissions::new(vec![PathPermission::read_write("./")]);
    let runner = TEST_DATA_DEFAULT.runner_with_zksync_config(config);
    let filter = Filter::new(".*", "ZkCheatcodesTest", &format!(".*zk{RE_PATH_SEPARATOR}*"));

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

/// Executes all zk console tests
#[tokio::test(flavor = "multi_thread")]
async fn test_zk_logs() {
    let mut config = TEST_DATA_DEFAULT.config.clone();
    config.fs_permissions = FsPermissions::new(vec![PathPermission::read_write("./")]);
    let runner = TEST_DATA_DEFAULT.runner_with_zksync_config(config);
    let filter = Filter::new(".*", "ZkConsoleTest", &format!(".*zk{RE_PATH_SEPARATOR}*"));

    let results = TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).test();

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
