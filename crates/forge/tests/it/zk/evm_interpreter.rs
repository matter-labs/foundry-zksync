//! Forge tests for zksync evm interpreter.

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use foundry_test_utils::Filter;
use revm::primitives::hardfork::SpecId;

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_evm_interpreter_create() {
    let mut zk_config = TEST_DATA_DEFAULT.zk_test_data.as_ref().unwrap().zk_config.clone();
    zk_config.verbosity = 5;
    zk_config.zksync.evm_interpreter = true;
    let runner = TEST_DATA_DEFAULT.runner_with_zksync_config(zk_config);
    let filter = Filter::new("testCreate|testCall", "EvmInterpreterTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}
