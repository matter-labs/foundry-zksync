//! Forge tests for zksync logs.

use std::collections::BTreeMap;

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use forge::revm::primitives::SpecId;
use foundry_test_utils::Filter;

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_logs_work_in_call() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkConsoleOutputDuringCall", "ZkConsoleTest", ".*");

    let results = TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).test();
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

    let results = TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).test();
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
                    "0xB5c1DF089600415B21FB76bf89900Adb575947c8".into(),
                    "print".into(),
                    "0xff".into(),
                    "print".into(),
                ]),
                None,
            )],
        )]),
    );
}
