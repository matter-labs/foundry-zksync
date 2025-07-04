//! Forge tests for zksync factory contracts.

use forge::revm::primitives::SpecId;
use foundry_test_utils::{forgetest_async, util, Filter, TestProject};

use crate::{
    config::TestConfig,
    test_helpers::{run_zk_script_test, TEST_DATA_DEFAULT},
};

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_can_deploy_in_method() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    {
        let filter = Filter::new("testClassicFactory|testNestedFactory", "ZkFactoryTest", ".*");
        TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_can_deploy_in_constructor() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    {
        let filter = Filter::new(
            "testConstructorFactory|testNestedConstructorFactory",
            "ZkFactoryTest",
            ".*",
        );
        TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_can_use_predeployed_factory() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    {
        let filter = Filter::new("testUser.*", "ZkFactoryTest", ".*");
        TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
    }
}

forgetest_async!(script_zk_can_deploy_in_method, |prj, cmd| {
    setup_factory_prj(&mut prj);
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/Factory.s.sol",
        "ZkClassicFactoryScript",
        None,
        2,
        Some(&["--broadcast"]),
    )
    .await;
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/Factory.s.sol",
        "ZkNestedFactoryScript",
        None,
        2,
        Some(&["--broadcast"]),
    )
    .await;
});

forgetest_async!(script_zk_can_deploy_in_constructor, |prj, cmd| {
    setup_factory_prj(&mut prj);
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/Factory.s.sol",
        "ZkConstructorFactoryScript",
        None,
        1,
        Some(&["--broadcast"]),
    )
    .await;
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/Factory.s.sol",
        "ZkNestedConstructorFactoryScript",
        None,
        1,
        Some(&["--broadcast"]),
    )
    .await;
});

forgetest_async!(script_zk_can_use_predeployed_factory, |prj, cmd| {
    setup_factory_prj(&mut prj);
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/Factory.s.sol",
        "ZkUserFactoryScript",
        None,
        3,
        Some(&["--broadcast"]),
    )
    .await;
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/Factory.s.sol",
        "ZkUserConstructorFactoryScript",
        None,
        2,
        Some(&["--broadcast"]),
    )
    .await;
});

fn setup_factory_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_source("Factory.sol", include_str!("../../../../../testdata/zk/Factory.sol")).unwrap();
    prj.add_script("Factory.s.sol", include_str!("../../fixtures/zk/Factory.s.sol")).unwrap();
}
