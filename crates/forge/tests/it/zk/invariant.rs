//! Invariant tests

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use forge::revm::primitives::SpecId;
use foundry_test_utils::Filter;

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_invariant_deposit() {
    let mut runner = TEST_DATA_DEFAULT.runner_zksync();

    // FIXME: just use the inline config
    runner.test_options.invariant.no_zksync_reserved_addresses = true;
    runner.test_options.invariant.fail_on_revert = true;
    runner.test_options.invariant.runs = 10;

    let filter = Filter::new(".*", "ZkInvariantTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}
