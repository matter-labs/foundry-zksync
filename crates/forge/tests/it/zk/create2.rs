use foundry_config::fs_permissions::PathPermission;
use foundry_test_utils::{forgetest_async, util, TestProject};

use crate::test_helpers::run_zk_script_test;

forgetest_async!(can_deploy_via_create2, |prj, cmd| {
    setup_create2_prj(&mut prj);
    let mut config = cmd.config();
    config.fs_permissions.add(PathPermission::read("./zkout"));
    prj.write_config(config);
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/Create2.s.sol",
        "Create2Script",
        None,
        2,
        Some(&["-vvvvv", "--broadcast"]),
    )
    .await;
});

fn setup_create2_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_script("Create2.s.sol", include_str!("../../fixtures/zk/Create2.s.sol")).unwrap();
    prj.add_source("Greeter.sol", include_str!("../../../../../testdata/zk/Greeter.sol")).unwrap();
    prj.add_source("Create2Utils.sol", include_str!("../../../../../testdata/zk/Create2Utils.sol"))
        .unwrap();
}
