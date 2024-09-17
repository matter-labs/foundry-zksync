//! Forge tests for zkysnc issues, to avoid regressions.
//!
//! Issue list: https://github.com/matter-labs/foundry-zksync/issues

use crate::{
    config::*,
    repros::test_repro,
    test_helpers::{ForgeTestData, ForgeTestProfile, TEST_DATA_DEFAULT},
};
use alloy_primitives::Address;
use foundry_config::{fs_permissions::PathPermission, FsPermissions};
use foundry_test_utils::Filter;

// zk-specific repros configuration
async fn repro_config(
    issue: usize,
    should_fail: bool,
    sender: Option<Address>,
    test_data: &ForgeTestData,
) -> TestConfig {
    foundry_test_utils::init_tracing();
    let filter = Filter::path(&format!(".*repros/Issue{issue}.t.sol"));

    let mut config = test_data.config.clone();
    config.fs_permissions = FsPermissions::new(vec![
        PathPermission::read("./fixtures/zk"),
        PathPermission::read("zkout"),
    ]);
    if let Some(sender) = sender {
        config.sender = sender;
    }

    let runner = test_data.runner_with_zksync_config(config);
    TestConfig::with_filter(runner, filter).set_should_fail(should_fail)
}

// https://github.com/matter-labs/foundry-zksync/issues/497
test_repro!(497);

#[tokio::test(flavor = "multi_thread")]
async fn issue_565(){
    let mut test_data = ForgeTestData::new(ForgeTestProfile::Default);
    // FIXME: just use the inline config
    test_data.test_opts.invariant.no_zksync_reserved_addresses = true;
    test_data.test_opts.invariant.fail_on_revert = true;
    test_data.test_opts.invariant.runs = 2;

    repro_config(565,false,None.into(), &test_data).await.run().await;
}
