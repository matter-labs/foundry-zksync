//! Invariant tests

use crate::{config::*, test_helpers::{ForgeTestData, ForgeTestProfile}};
use forge::revm::primitives::SpecId;
use foundry_test_utils::Filter;

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_invariant_deposit() {
    let mut test_data = ForgeTestData::new(ForgeTestProfile::Default);
    // FIXME: just use the inline config
    test_data.test_opts.invariant.no_zksync_reserved_addresses = true;
    test_data.test_opts.invariant.fail_on_revert = true;
    test_data.test_opts.invariant.runs = 10;

    let runner = test_data.runner_zksync();
    let filter = Filter::new(".*", "ZkInvariantTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}
