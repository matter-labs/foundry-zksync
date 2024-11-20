use foundry_test_utils::{forgetest_async, util, TestProject};

use crate::test_helpers::{deploy_zk_contract, run_zk_script_test};

// TODO(zk): add test that actually does the deployment
// of the unlinked contract via script, once recursive linking is supported
// and once we also support doing deploy-time linking

forgetest_async!(
    #[should_panic = "no bytecode for contract; is it abstract or unlinked?"]
    script_using_unlinked_fails,
    |prj, cmd| {
        setup_libs_prj(&mut prj);
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/Libraries.s.sol",
            "DeployUsesFoo",
            None,
            1,
            Some(&["-vvvvv"]),
        );
    }
);

forgetest_async!(
    #[should_panic = "Dynamic linking not supported"]
    create_using_unlinked_fails,
    |prj, cmd| {
        setup_libs_prj(&mut prj);

        // we don't really connect to the rpc because
        // we expect to fail before that point
        let foo_address = deploy_zk_contract(
            &mut cmd,
            "127.0.0.1:1234",
            "0x0000000000000000000000000000000000000000000000000000000000000000",
            "./src/WithLibraries.sol:UsesFoo",
        )
        .expect("Failed to deploy UsesFoo contract");

        assert!(!foo_address.is_empty(), "Deployed address should not be empty");
    }
);

fn setup_libs_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_script("Libraries.s.sol", include_str!("../../fixtures/zk/Libraries.s.sol")).unwrap();
    prj.add_source(
        "WithLibraries.sol",
        include_str!("../../../../../testdata/zk/WithLibraries.sol"),
    )
    .unwrap();
}
