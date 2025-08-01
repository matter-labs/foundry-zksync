use foundry_test_utils::{TestProject, forgetest_async, util};

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
        Some(&["-vvvvv", "--broadcast"]),
    )
    .await;
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/Deploy.s.sol",
        "DeployScript",
        None,
        3,
        Some(&["-vvvvv", "--broadcast"]),
    )
    .await;
});

fn setup_deploy_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_script("Deploy.s.sol", include_str!("../../fixtures/zk/Deploy.s.sol")).unwrap();
    prj.add_source("Greeter.sol", include_str!("../../../../../testdata/zk/Greeter.sol")).unwrap();
}
