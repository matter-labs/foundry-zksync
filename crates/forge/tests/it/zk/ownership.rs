//! Forge tests for testing ownership in zksync.
use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use forge::revm::primitives::SpecId;
use foundry_test_utils::Filter;

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_ownership() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkOwnership", "ZkOwnershipTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_ownership_delegate_call() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkOwnershipDelegateCall", "ZkOwnershipTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}
