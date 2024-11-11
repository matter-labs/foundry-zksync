//! Fork tests.

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use forge::revm::primitives::SpecId;
use foundry_test_utils::Filter;

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_setup_fork_failure() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter =
        Filter::new("testFail_ZkSetupForkFailureExecutesTest", "ZkSetupForkFailureTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_immutable_vars_persist_after_fork() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new(".*", "ZkForkImmutableVarsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}
