use foundry_test_utils::{forgetest_async, util, TestProject};

use crate::test_helpers::run_zk_script_test;

forgetest_async!(script_zk_can_deploy_proxy, |prj, cmd| {
    setup_proxy_prj(&mut prj);
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/Proxy.s.sol",
        "ProxyScript",
        Some("OpenZeppelin/openzeppelin-contracts"),
        4,
        Some(&["--broadcast"]),
    );
});

fn setup_proxy_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_script("Proxy.s.sol", include_str!("../../fixtures/zk/Proxy.s.sol")).unwrap();
}
