use foundry_config::fs_permissions::PathPermission;
use foundry_test_utils::{forgetest_async, util, util::OutputExt, TestProject};

use crate::test_helpers::run_zk_script_test;

forgetest_async!(setup_block_on_script_test, |prj, cmd| {
    setup_deploy_prj(&mut prj);
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/ScriptSetup.s.sol",
        "ScriptSetupNonce",
        None,
        5,
        Some(&["-vvvvv"]),
    );
});

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_nonce_mismatch() {
    let (prj, mut cmd) = util::setup_forge(
        "test_zk_contract_nonce_mismatch",
        foundry_test_utils::foundry_compilers::PathStyle::Dapptools,
    );
    util::initialize(prj.root());

    cmd.args(["install", "matter-labs/era-contracts", "--no-commit", "--shallow"]).assert_success();
    cmd.forge_fuse();

    let mut config = cmd.config();
    config.fs_permissions.add(PathPermission::read("./zkout"));
    prj.write_config(config);

    prj.add_source("Greeter.sol", include_str!("../../../../../testdata/zk/Greeter.sol")).unwrap();

    prj.add_test("NonceMismatch.t.sol", include_str!("../../fixtures/zk/NonceMismatch.t.sol"))
        .unwrap();

    cmd.args(["test", "--evm-version", "shanghai", "--mc", "NonceMismatchTest"]);
    cmd.assert_success().get_output().stdout_lossy().contains("Suite result: ok");
}

fn setup_deploy_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_script("ScriptSetup.s.sol", include_str!("../../fixtures/zk/ScriptSetup.s.sol"))
        .unwrap();
    prj.add_source("Greeter.sol", include_str!("../../../../../testdata/zk/Greeter.sol")).unwrap();
}
