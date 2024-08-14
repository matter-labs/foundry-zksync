//! Forge tests for zksync factory contracts.

use forge::revm::primitives::SpecId;
use foundry_test_utils::{forgetest_async, util, Filter, TestCommand, TestProject, ZkSyncNode};

use crate::{config::TestConfig, test_helpers::TEST_DATA_DEFAULT};

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_can_deploy_in_method() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    {
        let filter = Filter::new("testClassicFactory|testNestedFactory", "ZkFactoryTest", ".*");
        TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
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
        TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_can_use_predeployed_factory() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    {
        let filter = Filter::new("testUser.*", "ZkFactoryTest", ".*");
        TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
    }
}

forgetest_async!(script_zk_can_deploy_in_method, |prj, cmd| {
    setup_factory_prj(&mut prj);
    run_factory_script_test(prj.root(), &mut cmd, "ZkClassicFactoryScript", 2);
    run_factory_script_test(prj.root(), &mut cmd, "ZkNestedFactoryScript", 2);
});

forgetest_async!(script_zk_can_deploy_in_constructor, |prj, cmd| {
    setup_factory_prj(&mut prj);
    run_factory_script_test(prj.root(), &mut cmd, "ZkConstructorFactoryScript", 1);
    run_factory_script_test(prj.root(), &mut cmd, "ZkNestedConstructorFactoryScript", 1);
});

forgetest_async!(script_zk_can_use_predeployed_factory, |prj, cmd| {
    setup_factory_prj(&mut prj);
    run_factory_script_test(prj.root(), &mut cmd, "ZkUserFactoryScript", 3);
    run_factory_script_test(prj.root(), &mut cmd, "ZkUserConstructorFactoryScript", 2);
});

fn setup_factory_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_source("Factory.sol", include_str!("../../../../../testdata/zk/Factory.sol")).unwrap();
    prj.add_script("Factory.s.sol", include_str!("../../fixtures/zk/Factory.s.sol")).unwrap();
}

fn run_factory_script_test(
    root: impl AsRef<std::path::Path>,
    cmd: &mut TestCommand,
    name: &str,
    expected_broadcastable_txs: usize,
) {
    let node = ZkSyncNode::start();

    cmd.arg("script").args([
        "--zk-startup",
        &format!("./script/Factory.s.sol:{name}"),
        "--broadcast",
        "--private-key",
        "0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e",
        "--chain",
        "260",
        "--gas-estimate-multiplier",
        "310",
        "--rpc-url",
        node.url().as_str(),
        "--slow",
        "--evm-version",
        "shanghai",
    ]);

    assert!(cmd.stdout_lossy().contains("ONCHAIN EXECUTION COMPLETE & SUCCESSFUL"));

    let run_latest = foundry_common::fs::json_files(root.as_ref().join("broadcast").as_path())
        .find(|file| file.ends_with("run-latest.json"))
        .expect("No broadcast artifacts");

    let content = foundry_common::fs::read_to_string(run_latest).unwrap();

    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(
        json["transactions"].as_array().expect("broadcastable txs").len(),
        expected_broadcastable_txs
    );
    cmd.forge_fuse();
}
