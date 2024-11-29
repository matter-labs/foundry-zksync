//! Forge tests for constructor functionality with and without value.

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use foundry_test_utils::{forgetest_async, util, TestProject};

use crate::test_helpers::run_zk_script_test;
use forge::revm::primitives::SpecId;
use foundry_test_utils::Filter;

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_constructor_works() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkConstructor", "ZkConstructorTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

forgetest_async!(test_zk_constructor_works_in_script, |prj, cmd| {
    setup_deploy_prj(&mut prj);
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/Constructor.s.sol",
        "ConstructorScript",
        None,
        3,
        Some(&["-vvvvv", "--broadcast"]),
    );
});

fn setup_deploy_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_script("Constructor.s.sol", include_str!("../../fixtures/zk/Constructor.s.sol"))
        .unwrap();
    prj.add_source("Bank.sol", include_str!("../../../../../testdata/zk/Bank.sol")).unwrap();
}
