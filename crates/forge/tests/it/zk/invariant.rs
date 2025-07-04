//! Invariant tests

use std::sync::Arc;

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use forge::revm::primitives::SpecId;
use foundry_config::Config;
use foundry_test_utils::Filter;

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_invariant_deposit() {
    let mut runner = TEST_DATA_DEFAULT.runner_zksync();

    // FIXME: just use the inline config
    let mut config = Config::clone(&runner.config);
    config.invariant.no_zksync_reserved_addresses = true;
    config.invariant.fail_on_revert = true;
    config.invariant.runs = 10;
    runner.config = Arc::new(config);

    let filter = Filter::new(".*", "ZkInvariantTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}
