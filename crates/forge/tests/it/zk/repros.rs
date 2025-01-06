//! Forge tests for zkysnc issues, to avoid regressions.
//!
//! Issue list: https://github.com/matter-labs/foundry-zksync/issues

use std::sync::Arc;

use crate::{config::*, repros::test_repro, test_helpers::TEST_DATA_DEFAULT};
use alloy_primitives::Address;
use foundry_config::{fs_permissions::PathPermission, Config, FsPermissions};
use foundry_test_utils::Filter;

// zk-specific repros configuration
async fn repro_config(issue: usize, should_fail: bool, sender: Option<Address>) -> TestConfig {
    foundry_test_utils::init_tracing();
    let filter = Filter::path(&format!(".*repros/Issue{issue}.t.sol"));

    let test_data = &TEST_DATA_DEFAULT;
    let mut config = Config::clone(&test_data.config);
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

test_repro!(565; |cfg| {
    // FIXME: just use the inline config
    let mut config = Config::clone(&cfg.runner.config);
    config.invariant.no_zksync_reserved_addresses = true;
    config.invariant.fail_on_revert = true;
    config.invariant.runs = 2;

    cfg.runner.config = Arc::new(config);
});

// https://github.com/matter-labs/foundry-zksync/issues/687
test_repro!(687);
