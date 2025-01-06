use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    str::FromStr,
};

use foundry_compilers_artifacts_solc::Remapping;
use foundry_test_utils::foundry_compilers::{
    buildinfo::BuildInfo, cache::CompilerCache, project_util::*, resolver::parse::SolData,
    CompilerOutput, Graph, ProjectBuilder, ProjectPathsConfig,
};

use foundry_zksync_compilers::{
    artifacts::{contract::Contract, error::Error},
    compilers::{
        artifact_output::zk::ZkArtifactOutput,
        zksolc::{
            input::ZkSolcInput,
            settings::{ZkSolcError, ZkSolcWarning},
            ZkSolc, ZkSolcCompiler, ZkSolcSettings,
        },
    },
};

#[test]
fn zksync_can_compile_dapp_sample() {
    // let _ = tracing_subscriber::fmt()
    //     .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    //     .try_init()
    //     .ok();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-data/dapp-sample");
    let paths = ProjectPathsConfig::builder().sources(root.join("src")).lib(root.join("lib"));
    let project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::new(paths).unwrap();

    let compiled = project.compile().unwrap();
    assert!(compiled.find_first("Dapp").is_some());
    compiled.assert_success();

    // nothing to compile
    let compiled = project.compile().unwrap();
    assert!(compiled.find_first("Dapp").is_some());
    assert!(compiled.is_unchanged());

    let cache = CompilerCache::<ZkSolcSettings>::read(project.cache_path()).unwrap();

    // delete artifacts
    std::fs::remove_dir_all(&project.paths().artifacts).unwrap();
    let compiled = project.compile().unwrap();
    assert!(compiled.find_first("Dapp").is_some());
    assert!(!compiled.is_unchanged());

    let updated_cache = CompilerCache::<ZkSolcSettings>::read(project.cache_path()).unwrap();
    assert_eq!(cache, updated_cache);
}

fn test_zksync_can_compile_contract_with_suppressed_errors(compiler: ZkSolcCompiler) {
    // let _ = tracing_subscriber::fmt()
    //     .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    //     .try_init()
    //     .ok();
    let mut project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::dapptools().unwrap();
    project.project_mut().compiler = compiler;

    project
        .add_source(
            "Erroneous",
            r#"
        // SPDX-License-Identifier: MIT OR Apache-2.0
        pragma solidity ^0.8.10;
        contract Erroneous {
            function distribute(address payable recipient) public {
                recipient.send(1);
                recipient.transfer(1);
            }
        }
        "#,
        )
        .unwrap();

    let compiled = project.compile().unwrap();
    assert!(compiled.has_compiler_errors());

    project.project_mut().settings.settings.suppressed_errors =
        HashSet::from([ZkSolcError::SendTransfer]);

    let compiled = project.compile().unwrap();
    compiled.assert_success();
    assert!(compiled.find_first("Erroneous").is_some());
}

#[test]
fn zksync_can_compile_contract_with_suppressed_errors() {
    test_zksync_can_compile_contract_with_suppressed_errors(ZkSolcCompiler::default());
}

#[test]
fn zksync_pre_1_5_7_can_compile_contract_with_suppressed_errors() {
    let compiler = ZkSolcCompiler {
        zksolc: ZkSolc::get_path_for_version(&semver::Version::new(1, 5, 6)).unwrap(),
        solc: Default::default(),
    };
    test_zksync_can_compile_contract_with_suppressed_errors(compiler);
}

fn test_zksync_can_compile_contract_with_suppressed_warnings(compiler: ZkSolcCompiler) {
    // let _ = tracing_subscriber::fmt()
    //     .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    //     .try_init()
    //     .ok();
    let mut project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::dapptools().unwrap();
    project.project_mut().compiler = compiler;

    project
        .add_source(
            "Warning",
            r#"
        // SPDX-License-Identifier: MIT OR Apache-2.0
        pragma solidity ^0.8.10;
        contract Warning {
            function test() public view {
                require(tx.origin != address(0), "what");
            }
        }
        "#,
        )
        .unwrap();

    let compiled = project.compile().unwrap();
    compiled.assert_success();
    assert!(
        compiled
            .output()
            .errors
            .iter()
            .any(|err| err.is_warning() && err.message.contains("tx.origin")),
        "{:#?}",
        compiled.output().errors
    );

    project.project_mut().settings.settings.suppressed_warnings =
        HashSet::from([ZkSolcWarning::TxOrigin]);

    let compiled = project.compile().unwrap();
    compiled.assert_success();
    assert!(compiled.find_first("Warning").is_some());
    assert!(
        !compiled
            .output()
            .errors
            .iter()
            .any(|err| err.is_warning() && err.message.contains("tx.origin")),
        "{:#?}",
        compiled.output().errors
    );
}

#[test]
fn zksync_can_compile_contract_with_suppressed_warnings() {
    test_zksync_can_compile_contract_with_suppressed_warnings(ZkSolcCompiler::default());
}

#[test]
fn zksync_pre_1_5_7_can_compile_contract_with_suppressed_warnings() {
    let compiler = ZkSolcCompiler {
        zksolc: ZkSolc::get_path_for_version(&semver::Version::new(1, 5, 6)).unwrap(),
        solc: Default::default(),
    };
    test_zksync_can_compile_contract_with_suppressed_warnings(compiler);
}

#[test]
fn zksync_can_compile_dapp_detect_changes_in_libs() {
    // let _ = tracing_subscriber::fmt()
    //     .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    //     .try_init()
    //     .ok();
    let mut project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::dapptools().unwrap();

    let remapping = project.paths().libraries[0].join("remapping");
    project
        .paths_mut()
        .remappings
        .push(Remapping::from_str(&format!("remapping/={}/", remapping.display())).unwrap());

    let src = project
        .add_source(
            "Foo",
            r#"
    pragma solidity ^0.8.10;
    import "remapping/Bar.sol";

    contract Foo {}
   "#,
        )
        .unwrap();

    let lib = project
        .add_lib(
            "remapping/Bar",
            r"
    pragma solidity ^0.8.10;

    contract Bar {}
    ",
        )
        .unwrap();

    let graph = Graph::<SolData>::resolve(project.paths()).unwrap();
    assert_eq!(graph.files().len(), 2);
    assert_eq!(graph.files().clone(), HashMap::from([(src, 0), (lib, 1),]));

    let compiled = project.compile().unwrap();
    assert!(compiled.find_first("Foo").is_some());
    assert!(compiled.find_first("Bar").is_some());
    compiled.assert_success();

    // nothing to compile
    let compiled = project.compile().unwrap();
    assert!(compiled.find_first("Foo").is_some());
    assert!(compiled.is_unchanged());

    let cache = CompilerCache::<ZkSolcSettings>::read(&project.paths().cache).unwrap();
    assert_eq!(cache.files.len(), 2);

    // overwrite lib
    project
        .add_lib(
            "remapping/Bar",
            r"
    pragma solidity ^0.8.10;

    // changed lib
    contract Bar {}
    ",
        )
        .unwrap();

    let graph = Graph::<SolData>::resolve(project.paths()).unwrap();
    assert_eq!(graph.files().len(), 2);

    let compiled = project.compile().unwrap();
    assert!(compiled.find_first("Foo").is_some());
    assert!(compiled.find_first("Bar").is_some());
    // ensure change is detected
    assert!(!compiled.is_unchanged());
}

#[test]
fn zksync_can_compile_dapp_detect_changes_in_sources() {
    let project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::dapptools().unwrap();

    let src = project
        .add_source(
            "DssSpell.t",
            r#"
    pragma solidity ^0.8.10;
    import "./DssSpell.t.base.sol";

   contract DssSpellTest is DssSpellTestBase { }
   "#,
        )
        .unwrap();

    let base = project
        .add_source(
            "DssSpell.t.base",
            r"
    pragma solidity ^0.8.10;

  contract DssSpellTestBase {
       address deployed_spell;
       function setUp() public {
           deployed_spell = address(0xA867399B43aF7790aC800f2fF3Fa7387dc52Ec5E);
       }
  }
   ",
        )
        .unwrap();

    let graph = Graph::<SolData>::resolve(project.paths()).unwrap();
    assert_eq!(graph.files().len(), 2);
    assert_eq!(graph.files().clone(), HashMap::from([(base, 0), (src, 1),]));
    assert_eq!(graph.imported_nodes(1).to_vec(), vec![0]);

    let compiled = project.compile().unwrap();
    compiled.assert_success();
    assert!(compiled.find_first("DssSpellTest").is_some());
    assert!(compiled.find_first("DssSpellTestBase").is_some());

    // nothing to compile
    let compiled = project.compile().unwrap();
    assert!(compiled.is_unchanged());
    assert!(compiled.find_first("DssSpellTest").is_some());
    assert!(compiled.find_first("DssSpellTestBase").is_some());

    let cache = CompilerCache::<ZkSolcSettings>::read(&project.paths().cache).unwrap();
    assert_eq!(cache.files.len(), 2);

    let artifacts = compiled.into_artifacts().collect::<HashMap<_, _>>();

    // overwrite import
    let _ = project
        .add_source(
            "DssSpell.t.base",
            r"
    pragma solidity ^0.8.10;

  contract DssSpellTestBase {
       address deployed_spell;
       function setUp() public {
           deployed_spell = address(0);
       }
  }
   ",
        )
        .unwrap();
    let graph = Graph::<SolData>::resolve(project.paths()).unwrap();
    assert_eq!(graph.files().len(), 2);

    let compiled = project.compile().unwrap();
    assert!(compiled.find_first("DssSpellTest").is_some());
    assert!(compiled.find_first("DssSpellTestBase").is_some());
    // ensure change is detected
    assert!(!compiled.is_unchanged());

    // and all recompiled artifacts are different
    for (p, artifact) in compiled.into_artifacts() {
        let other = artifacts
            .iter()
            .find(|(id, _)| id.name == p.name && id.version == p.version && id.source == p.source)
            .unwrap()
            .1;
        assert_ne!(artifact, *other);
    }
}

#[test]
fn zksync_can_emit_build_info() {
    let mut project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::dapptools().unwrap();
    project.project_mut().build_info = true;
    project
        .add_source(
            "A",
            r#"
pragma solidity ^0.8.10;
import "./B.sol";
contract A { }
"#,
        )
        .unwrap();

    project
        .add_source(
            "B",
            r"
pragma solidity ^0.8.10;
contract B { }
",
        )
        .unwrap();

    let compiled = project.compile().unwrap();
    compiled.assert_success();

    let info_dir = project.project().build_info_path();
    assert!(info_dir.exists());

    let mut build_info_count = 0;
    for entry in fs::read_dir(info_dir).unwrap() {
        let info =
            BuildInfo::<ZkSolcInput, CompilerOutput<Error, Contract>>::read(&entry.unwrap().path())
                .unwrap();
        assert!(info.output.metadata.contains_key("zksyncSolcVersion"));
        build_info_count += 1;
    }
    assert_eq!(build_info_count, 1);
}

#[test]
fn zksync_can_clean_build_info() {
    let mut project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::dapptools().unwrap();

    project.project_mut().build_info = true;
    project.project_mut().paths.build_infos = project.project_mut().paths.root.join("build-info");
    project
        .add_source(
            "A",
            r#"
pragma solidity ^0.8.10;
import "./B.sol";
contract A { }
"#,
        )
        .unwrap();

    project
        .add_source(
            "B",
            r"
pragma solidity ^0.8.10;
contract B { }
",
        )
        .unwrap();

    let compiled = project.compile().unwrap();
    compiled.assert_success();

    let info_dir = project.project().build_info_path();
    assert!(info_dir.exists());

    let mut build_info_count = 0;
    for entry in fs::read_dir(info_dir).unwrap() {
        let _info =
            BuildInfo::<ZkSolcInput, CompilerOutput<Error, Contract>>::read(&entry.unwrap().path())
                .unwrap();
        build_info_count += 1;
    }
    assert_eq!(build_info_count, 1);

    project.project().cleanup().unwrap();

    assert!(!project.project().build_info_path().exists());
}

#[test]
fn zksync_cant_compile_a_file_outside_allowed_paths() {
    // For this test we should create the following directory structure:
    // project_root/
    // ├── outer/
    // │   ├── Util.sol
    // │   └── Helper.sol
    // └── contracts/
    //     ├── src/
    //     │   └── Main.sol

    let tmp_dir = tempfile::tempdir().unwrap();
    let project_root = tmp_dir.path().to_path_buf();
    let contracts_dir = tempfile::tempdir_in(&project_root).unwrap();

    fs::create_dir_all(contracts_dir.path().join("src")).unwrap();
    fs::create_dir_all(project_root.join("outer")).unwrap();

    fs::write(
        contracts_dir.path().join("src/Main.sol"),
        r#"
pragma solidity ^0.8.0;
import "@outer/Helper.sol";
contract Main {
    Helper helper = new Helper();
    function run() public {}
}
"#,
    )
    .unwrap();

    fs::write(
        project_root.join("outer/Helper.sol"),
        r#"
pragma solidity ^0.8.0;
import "./Util.sol";
contract Helper {
    Util util = new Util();
}
"#,
    )
    .unwrap();

    fs::write(
        project_root.join("outer/Util.sol"),
        r#"
pragma solidity ^0.8.0;
contract Util {}
"#,
    )
    .unwrap();

    let root = contracts_dir.path().to_path_buf();
    let paths = ProjectPathsConfig::builder()
        .root(root.clone())
        .sources(root.join("src"))
        .remappings(vec![Remapping::from_str("@outer/=../outer/").unwrap()])
        .build()
        .unwrap();

    let inner = ProjectBuilder::<ZkSolcCompiler, ZkArtifactOutput>::new(Default::default())
        .paths(paths)
        .build(Default::default())
        .unwrap();
    let project =
        TempProject::<ZkSolcCompiler, ZkArtifactOutput>::create_new(contracts_dir, inner).unwrap();

    let compiled = project.compile().unwrap();
    assert!(compiled.has_compiler_errors());
    assert!(compiled.output().errors.iter().any(|error| error
        .formatted_message
        .as_ref()
        .map_or(false, |msg| msg.contains("File outside of allowed directories"))));
}

#[test]
fn zksync_can_compile_a_file_in_allowed_paths_successfully() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let project_root = tmp_dir.path().to_path_buf();
    let contracts_dir = tempfile::tempdir_in(&project_root).unwrap();

    fs::create_dir_all(contracts_dir.path().join("src")).unwrap();
    fs::create_dir_all(project_root.join("outer")).unwrap();

    fs::write(
        contracts_dir.path().join("src/Main.sol"),
        r#"
pragma solidity ^0.8.0;
import "@outer/Helper.sol";
contract Main {
    Helper helper = new Helper();
    function run() public {}
}
"#,
    )
    .unwrap();

    fs::write(
        project_root.join("outer/Helper.sol"),
        r#"
pragma solidity ^0.8.0;
import "./Util.sol";
contract Helper {
    Util util = new Util();
}
"#,
    )
    .unwrap();

    fs::write(
        project_root.join("outer/Util.sol"),
        r#"
pragma solidity ^0.8.0;
contract Util {}
"#,
    )
    .unwrap();

    let root = contracts_dir.path().to_path_buf();
    let paths = ProjectPathsConfig::builder()
        .root(root.clone())
        .sources(root.join("src"))
        .allowed_paths(vec!["../"])
        .remappings(vec![Remapping::from_str("@outer/=../outer/").unwrap()])
        .build()
        .unwrap();

    let inner = ProjectBuilder::<ZkSolcCompiler, ZkArtifactOutput>::new(Default::default())
        .paths(paths)
        .build(Default::default())
        .unwrap();
    let project =
        TempProject::<ZkSolcCompiler, ZkArtifactOutput>::create_new(contracts_dir, inner).unwrap();

    let compiled = project.compile().unwrap();
    compiled.assert_success();
}

#[test]
fn zksync_can_compile_yul_sample() {
    // let _ = tracing_subscriber::fmt()
    //     .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    //     .try_init()
    //     .ok();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-data/yul-sample");
    let paths = ProjectPathsConfig::builder().sources(root);
    let project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::new(paths).unwrap();

    let compiled = project.compile().unwrap();
    compiled.assert_success();

    let simple_store_artifact = compiled
        .compiled_artifacts()
        .values()
        .find_map(|contracts| {
            contracts
                .iter()
                .find(|(name, _)| name.ends_with("SimpleStore.yul"))
                .and_then(|(_, artifacts)| artifacts.first())
        })
        .expect("SimpleStore.yul artifact not found")
        .artifact
        .bytecode
        .clone()
        .unwrap();

    let yul_bytecode = simple_store_artifact.object().into_bytes().unwrap();

    assert!(!yul_bytecode.is_empty(), "SimpleStore.yul bytecode is empty");
}
