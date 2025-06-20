//! Forge tests for basic zkysnc functionality.

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use forge::revm::primitives::SpecId;
use foundry_test_utils::Filter;

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_can_invoke_test_contract() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter =
        Filter::new("test_callAnyFunction|test_callAnyFunctionReverts", "CallAnyMethodTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore] // TODO: right now, immutables are not migrated, see
          // `ZksyncCheatcodeInspectorStrategyRunner::select_zk_vm`.
async fn test_zk_can_invoke_test_contract_with_immutables() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("test_getImmutable", "CallAnyMethodTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}
