use forge::revm::primitives::SpecId;
use foundry_test_utils::{forgetest_async, util, Filter, TestCommand, TestProject};
use semver::Version;

use crate::{
    config::TestConfig,
    test_helpers::{deploy_zk_contract, run_zk_script_test, TEST_DATA_DEFAULT},
};

const ZKSOLC_MIN_LINKING_VERSION: Version = Version::new(1, 5, 9);

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_deploy_time_linking() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new(".*", "DeployTimeLinking", ".*");

    TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
}

// TODO(zk): add equivalent test for `GetCodeUnlinked`
// would probably need to split in separate file (and skip other file)
// as tests look for _all_ lib deps and deploy them for every test

forgetest_async!(
    #[should_panic = "no bytecode for contract; is it abstract or unlinked?"]
    script_zk_fails_indirect_reference_to_unlinked,
    |prj, cmd| {
        setup_libs_prj(&mut prj, &mut cmd, None);
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/Libraries.s.sol",
            "GetCodeUnlinked",
            None,
            1,
            Some(&["-vvvvv"]),
        )
        .await;
    }
);

forgetest_async!(script_zk_deploy_time_linking, |prj, cmd| {
    setup_libs_prj(&mut prj, &mut cmd, None);
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/Libraries.s.sol",
        "DeployTimeLinking",
        None,
        // lib `Foo` + `UsesFoo` deployment
        2,
        Some(&["-vvvvv", "--broadcast"]),
    )
    .await;
});

forgetest_async!(
    #[ignore]
    #[should_panic = "deploy-time linking not supported"]
    script_zk_deploy_time_linking_fails_older_version,
    |prj, cmd| {
        let mut version = ZKSOLC_MIN_LINKING_VERSION;
        version.patch -= 1;

        setup_libs_prj(&mut prj, &mut cmd, Some(version));
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/Libraries.s.sol",
            "DeployTimeLinking",
            None,
            1,
            Some(&["-vvvvv"]),
        )
        .await;
    }
);

forgetest_async!(
    #[should_panic = "Dynamic linking not supported"]
    create_zk_using_unlinked_fails,
    |prj, cmd| {
        setup_libs_prj(&mut prj, &mut cmd, None);

        // we don't really connect to the rpc because
        // we expect to fail before that point
        let foo_address = deploy_zk_contract(
            &mut cmd,
            "127.0.0.1:1234",
            "0x0000000000000000000000000000000000000000000000000000000000000000",
            "./src/WithLibraries.sol:UsesFoo",
            None,
        )
        .expect("Failed to deploy UsesFoo contract");

        assert!(!foo_address.is_empty(), "Deployed address should not be empty");
    }
);

fn setup_libs_prj(prj: &mut TestProject, cmd: &mut TestCommand, zksolc: Option<Version>) {
    util::initialize(prj.root());

    let mut config = cmd.config();
    if let Some(zksolc) = zksolc {
        config.zksync.zksolc.replace(foundry_config::SolcReq::Version(zksolc));
    }
    prj.write_config(config);

    prj.add_script("Libraries.s.sol", include_str!("../../fixtures/zk/Libraries.s.sol")).unwrap();
    prj.add_source(
        "WithLibraries.sol",
        include_str!("../../../../../testdata/zk/WithLibraries.sol"),
    )
    .unwrap();
}
