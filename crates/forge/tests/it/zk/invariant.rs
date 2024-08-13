//! Invariant tests

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use forge::revm::primitives::SpecId;
use foundry_test_utils::Filter;

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_invariant_deposit() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkInvariantDeposit", "ZkInvariantTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}
