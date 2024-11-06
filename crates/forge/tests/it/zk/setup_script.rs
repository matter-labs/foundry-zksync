use foundry_test_utils::{forgetest_async, util, TestProject};

use crate::test_helpers::run_zk_script_test;

forgetest_async!(setup_block_on_script_test, |prj, cmd| {
    setup_deploy_prj(&mut prj);
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/ScriptSetup.s.sol",
        "ScriptSetupNonce",
        None,
        2,
        Some(&["-vvvvv"]),
    );
});

fn setup_deploy_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_script("ScriptSetup.s.sol", include_str!("../../fixtures/zk/ScriptSetup.s.sol"))
        .unwrap();
    prj.add_source("Greeter.sol", include_str!("../../../../../testdata/zk/Greeter.sol")).unwrap();
}
