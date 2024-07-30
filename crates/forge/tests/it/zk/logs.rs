//! Forge tests for zksync logs.

use std::collections::BTreeMap;

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use forge::revm::primitives::SpecId;
use foundry_test_utils::Filter;

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_logs_work() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new(".*", "ZkConsoleTest", ".*");

    let results = TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).test();
    assert_multiple(
        &results,
        BTreeMap::from([(
            "zk/Console.t.sol:ZkConsoleTest",
            vec![(
                "testZkConsoleOutput()",
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
