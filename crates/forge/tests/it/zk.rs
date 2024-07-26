//! Forge tests for cheatcodes.

use std::collections::BTreeMap;

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use forge::revm::primitives::SpecId;
use foundry_config::fs_permissions::PathPermission;
use foundry_test_utils::Filter;

/// Executes all zk basic tests
#[tokio::test(flavor = "multi_thread")]
async fn test_zk_basic() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new(".*", "ZkBasicTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

/// Executes all zk contract tests
#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contracts() {
    let mut zk_config = TEST_DATA_DEFAULT.zk_test_data.zk_config.clone();
    zk_config.fs_permissions.add(PathPermission::read_write("./zk/zkout/ConstantNumber.sol"));
    let runner = TEST_DATA_DEFAULT.runner_with_zksync_config(zk_config);
    let filter = Filter::new(".*", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

/// Executes all zk cheatcode tests
#[tokio::test(flavor = "multi_thread")]
async fn test_zk_cheats() {
    let mut zk_config = TEST_DATA_DEFAULT.zk_test_data.zk_config.clone();
    zk_config.fs_permissions.add(PathPermission::read_write("./zk/zkout/ConstantNumber.sol"));
    let runner = TEST_DATA_DEFAULT.runner_with_zksync_config(zk_config);
    let filter = Filter::new(".*", "ZkCheatcodesTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

/// Executes all zk console tests
#[tokio::test(flavor = "multi_thread")]
async fn test_zk_logs() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new(".*", "ZkConsoleTest", ".*");

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
