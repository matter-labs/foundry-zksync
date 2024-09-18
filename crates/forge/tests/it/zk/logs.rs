//! Forge tests for zksync logs.

use std::collections::BTreeMap;

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use forge::revm::primitives::SpecId;
use foundry_test_utils::Filter;

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_logs_work_in_call() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkConsoleOutputDuringCall", "ZkConsoleTest", ".*");

    let results = TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).test();
    assert_multiple(
        &results,
        BTreeMap::from([(
            "zk/Console.t.sol:ZkConsoleTest",
            vec![(
                "testZkConsoleOutputDuringCall()",
                true,
                None,
                Some(vec![
                    "print".into(),
                    "outer print".into(),
                    "0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496".into(),
                    "print".into(),
                    "0xff".into(),
                    "print".into(),
                ]),
                None,
            )],
        )]),
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_logs_work_in_create() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkConsoleOutputDuringCreate", "ZkConsoleTest", ".*");

    let results = TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).test();
    assert_multiple(
        &results,
        BTreeMap::from([(
            "zk/Console.t.sol:ZkConsoleTest",
            vec![(
                "testZkConsoleOutputDuringCreate()",
                true,
                None,
                Some(vec![
                    "print".into(),
                    "outer print".into(),
                    "0xF9E9ba9Ed9B96AB918c74B21dD0f1D5f2ac38a30".into(),
                    "print".into(),
                    "0xff".into(),
                    "print".into(),
                ]),
                None,
            )],
        )]),
    );
}
