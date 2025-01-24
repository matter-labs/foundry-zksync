use crate::test_helpers::{deploy_zk_contract, run_zk_script_test};
use foundry_test_utils::{forgetest_async, util, TestProject};

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
        )
        .await;
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

// test that checks that you have to recompile the project if the zksolc version changes (the
// cache is invalidated)
// step 1, create a config with a specific zksolc version i.e 1.5.6
// step 2, create a project with the config
// compile the project
// check that output contains the zksolc version 1.5.6
// step 3, create a new config with a different zksolc version i.e 1.5.7
// step 4, create a project with the new config
// compile the project
// check that output contains the zksolc version 1.5.7 (demonstrating that the cache was
// invalidated, and the project was recompiled) compile the project again,
// check the output once more it should say that the cache is ok
// forgetest_async!(
//     zksync_project_has_zksync_solc_when_solc_req_is_a_version_and_zksolc_version_changes,
//     |prj, cmd| {
//         let mut zk_config = ForgeTestProfile::Default.zk_config();

//         let project = config_create_project(&zk_config, false, true).unwrap();

//         let version = get_solc_version_info(&path.solc).unwrap();
//         assert!(version.zksync_version.is_some());
//         assert_eq!(version.zksync_version.unwrap(), Version::new(1, 5, 6));

//         zk_config.zksync.zksolc = Some(SolcReq::Version(Version::new(1, 5, 7)));
//         let project = config_create_project(&zk_config, false, true).unwrap();
//         let solc_compiler = project.compiler.solc;
//         if let SolcCompiler::Specific(path) = solc_compiler {
//             let version = get_solc_version_info(&path.solc).unwrap();
//             assert!(version.zksync_version.is_some());
//             assert_eq!(version.zksync_version.unwrap(), Version::new(1, 5, 7));
//         } else {
//             panic!("Expected SolcCompiler::Specific");
//         }

//         let project = config_create_project(&zk_config, false, true).unwrap();
//         let solc_compiler = project.compiler.solc;
//         if let SolcCompiler::Specific(path) = solc_compiler {
//             let version = get_solc_version_info(&path.solc).unwrap();
//             assert!(version.zksync_version.is_some());
//             assert_eq!(version.zksync_version.unwrap(), Version::new(1, 5, 7));
//         } else {
//             panic!("Expected SolcCompiler::Specific");
//         }
//     }
// );
