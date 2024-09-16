use foundry_test_utils::{forgetest_async, util, TestProject};

use crate::test_helpers::run_zk_script_test;

forgetest_async!(multiple_deployments_of_the_same_contract, |prj, cmd| {
    setup_deploy_prj(&mut prj);
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/Deploy.s.sol",
        "DeployScript",
        None,
        3,
        Some(&["-vvvvv"]),
    );
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/Deploy.s.sol",
        "DeployScript",
        None,
        3,
        Some(&["-vvvvv"]),
    );
});

forgetest_async!(can_deploy_via_create2, |prj, cmd| {
    setup_deploy_prj(&mut prj);
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/Create2.s.sol",
        "Create2Script",
        None,
        3,
        Some(&["-vvvvv"]),
    );
});

fn setup_deploy_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_script("Deploy.s.sol", include_str!("../../fixtures/zk/Deploy.s.sol")).unwrap();
    prj.add_script("Create2.s.sol", include_str!("../../fixtures/zk/Create2.s.sol")).unwrap();
    prj.add_source("Greeter.sol", include_str!("../../../../../testdata/zk/Greeter.sol")).unwrap();
    prj.add_source("CustomNumber.sol", include_str!("../../../../../testdata/zk/CustomNumber.sol"))
        .unwrap();
}
