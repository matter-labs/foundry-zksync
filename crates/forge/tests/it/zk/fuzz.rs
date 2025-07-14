//! Fuzz tests.

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use foundry_test_utils::Filter;
use revm::primitives::hardfork::SpecId;

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_fuzz_avoid_system_addresses() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkFuzzAvoidSystemAddresses", "ZkFuzzTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}
