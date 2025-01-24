    // test that checks that you have to recompile the project if the zksolc version changes (the
    // cache is invalidated)
    #[test]
    fn zksync_project_has_zksync_solc_when_solc_req_is_a_version_and_zksolc_version_changes() {
        let zk_config = zk_config();

        let config =
            Config { zksolc: Some(SolcReq::Version(Version::new(0, 8, 26))), ..Default::default() };
        let project = config_create_project(&config, false, true).unwrap();
        let solc_compiler = project.compiler.solc;
        if let SolcCompiler::Specific(path) = solc_compiler {
            let version = get_solc_version_info(&path.solc).unwrap();
            assert!(version.zksync_version.is_some());
        } else {
            panic!("Expected SolcCompiler::Specific");
        }
    }