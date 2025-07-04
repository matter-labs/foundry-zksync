use crate::{
    config::TestConfig,
    test_helpers::{run_zk_script_test, TEST_DATA_DEFAULT},
};
use forge::revm::primitives::SpecId;
use foundry_test_utils::{forgetest_async, util, Filter, TestProject};

forgetest_async!(setup_block_on_script_test, |prj, cmd| {
    setup_deploy_prj(&mut prj);
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/ScriptSetup.s.sol",
        "ScriptSetupNonce",
        None,
        3,
        Some(&["-vvvvv"]),
    )
    .await;
});

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_nonce_mismatch() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testTxOriginNonceDoesNotUpdate", "NonceMismatchTest", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

forgetest_async!(setup_broadcast_in_setup_test, |prj, cmd| {
    setup_deploy_prj(&mut prj);
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/ScriptBroadcastInSetup.s.sol",
        "ScriptBroadcastInSetup",
        None,
        4,
        Some(&["-vvvvv", "--broadcast"]),
    )
    .await;
});

fn setup_deploy_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_script("ScriptSetup.s.sol", include_str!("../../fixtures/zk/ScriptSetup.s.sol"))
        .unwrap();
    prj.add_script(
        "ScriptBroadcastInSetup.s.sol",
        include_str!("../../fixtures/zk/ScriptBroadcastInSetup.s.sol"),
    )
    .unwrap();
    prj.add_source("Greeter.sol", include_str!("../../../../../testdata/zk/Greeter.sol")).unwrap();
}
