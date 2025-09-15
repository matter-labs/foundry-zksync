use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    str::FromStr,
};

use foundry_test_utils::foundry_compilers::{
    CompilerOutput, Graph, ProjectBuilder, ProjectPathsConfig, artifacts::Remapping,
    buildinfo::BuildInfo, cache::CompilerCache, project_util::*, resolver::parse::SolParser,
};

use foundry_zksync_compilers::{
    artifacts::{
        contract::Contract,
        error::Error,
        output_selection::{FileOutputSelection, OutputSelection, OutputSelectionFlag},
    },
    compilers::{
        artifact_output::zk::ZkArtifactOutput,
        zksolc::{
            ErrorType, WarningType, ZkSolc, ZkSolcCompiler, ZkSolcSettings,
            input::ZkSolcInput,
            settings::{BytecodeHash, SettingsMetadata},
        },
    },
};
use semver::Version;

#[test]
fn test_zk_can_compile_dapp_sample() {
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

#[test]
fn test_zk_can_compile_dapp_sample_with_supported_zksolc_versions() {
    for version in ZkSolc::zksolc_supported_versions() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-data/dapp-sample");
        let paths = ProjectPathsConfig::builder().sources(root.join("src")).lib(root.join("lib"));
        let mut project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::new(paths).unwrap();
        project.project_mut().settings.set_zksolc_version(version.clone()).unwrap();

        let compiled = project.compile().unwrap();
        compiled.assert_success();
        assert_eq!(compiled.compiled_artifacts().len(), 3, "zksolc {version}");
        for (n, c) in compiled.artifacts() {
            assert!(
                c.bytecode
                    .as_ref()
                    .unwrap_or_else(|| panic!(
                        "zksolc {version}: {n} artifact bytecode field should not be empty"
                    ))
                    .object()
                    .bytes_len()
                    > 0,
                "zksolc {version}",
            );
        }
    }
}

#[test]
fn test_zk_can_set_hash_type_with_supported_versions() {
    for version in ZkSolc::zksolc_supported_versions() {
        let mut project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::dapptools().unwrap();
        project.project_mut().settings.set_zksolc_version(version.clone()).unwrap();
        project.project_mut().settings.settings.metadata =
            Some(SettingsMetadata::new(Some(BytecodeHash::None)));

        project
            .add_source(
                "Contract",
                r#"
            // SPDX-License-Identifier: MIT OR Apache-2.0
            pragma solidity 0.8.10;
            contract Contract {
                function call() public {}
            }
            "#,
            )
            .unwrap();

        let compiled = project.compile().unwrap();
        compiled.assert_success();
        let contract_none = compiled.find_first("Contract").unwrap();
        let bytecode_none =
            contract_none.bytecode.as_ref().map(|b| b.object().into_bytes()).unwrap().unwrap();

        project.project_mut().settings.settings.metadata =
            Some(SettingsMetadata::new(Some(BytecodeHash::Keccak256)));

        let compiled = project.compile().unwrap();
        compiled.assert_success();
        let contract_keccak = compiled.find_first("Contract").unwrap();
        let bytecode_keccak =
            contract_keccak.bytecode.as_ref().map(|b| b.object().into_bytes()).unwrap().unwrap();

        assert!(
            (bytecode_none.len() as i32 - bytecode_keccak.len() as i32).abs() % 32 == 0,
            "zksolc {version}: Bytecode lengths can differ by multiples of 32 bytes when including metadata"
        );
        assert_ne!(bytecode_none, bytecode_keccak, "zksolc {version}");

        let end = bytecode_keccak.len() - 32;
        assert_eq!(bytecode_none.slice(..end), bytecode_keccak.slice(..end), "zksolc {version}");
    }
}

fn test_zksync_can_compile_contract_with_suppressed_errors(zksolc_version: Version) {
    // let _ = tracing_subscriber::fmt()
    //     .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    //     .try_init()
    //     .ok();

    let mut project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::dapptools().unwrap();
    project.project_mut().settings.set_zksolc_version(zksolc_version).unwrap();

    project
        .add_source(
            "Erroneous",
            r#"
        // SPDX-License-Identifier: MIT OR Apache-2.0
        pragma solidity 0.8.10;
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
        HashSet::from([ErrorType::SendTransfer]);

    let compiled = project.compile().unwrap();

    compiled.assert_success();
    assert!(compiled.find_first("Erroneous").is_some());
}

#[test]
fn test_zk_can_compile_contract_with_suppressed_errors() {
    test_zksync_can_compile_contract_with_suppressed_errors(
        ZkSolc::zksolc_latest_supported_version(),
    );
}

#[test]
fn test_zk_pre_1_5_7_can_compile_contract_with_suppressed_errors() {
    test_zksync_can_compile_contract_with_suppressed_errors(Version::new(1, 5, 6));
}

fn test_zksync_can_compile_contract_with_suppressed_warnings(zksolc_version: Version) {
    // let _ = tracing_subscriber::fmt()
    //     .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    //     .try_init()
    //     .ok();
    let mut project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::dapptools().unwrap();
    project.project_mut().settings.set_zksolc_version(zksolc_version).unwrap();

    project
        .add_source(
            "Warning",
            r#"
        // SPDX-License-Identifier: MIT OR Apache-2.0
        pragma solidity 0.8.10;
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
        HashSet::from([WarningType::TxOrigin]);

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
fn test_zk_can_compile_contract_with_suppressed_warnings() {
    test_zksync_can_compile_contract_with_suppressed_warnings(
        ZkSolc::zksolc_latest_supported_version(),
    );
}

#[test]
fn test_zk_pre_1_5_7_can_compile_contract_with_suppressed_warnings() {
    test_zksync_can_compile_contract_with_suppressed_warnings(Version::new(1, 5, 6));
}

fn test_zksync_can_compile_contract_with_assembly_create_suppressed_warnings(
    zksolc_version: Version,
) {
    let mut project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::dapptools().unwrap();
    project.project_mut().settings.set_zksolc_version(zksolc_version).unwrap();

    project
        .add_source(
            "Warning",
            r#"
        // SPDX-License-Identifier: MIT OR Apache-2.0
        pragma solidity 0.8.10;
        contract Warning {
            function deployWithCreate(bytes memory bytecode) public returns (address addr) {
                assembly {
                    addr := create(0, add(bytecode, 0x20), mload(bytecode))
                }
            }
        }
        "#,
        )
        .unwrap();

    // Compile the project and ensure it succeeds with warnings
    let compiled = project.compile().unwrap();
    compiled.assert_success();
    assert!(
        compiled
            .output()
            .errors
            .iter()
            .any(|err| err.is_warning() && err.message.contains("create")),
        "Expected assembly `create` warning, but none found: {:#?}",
        compiled.output().errors
    );

    project.project_mut().settings.settings.suppressed_warnings =
        HashSet::from([WarningType::AssemblyCreate]);

    let compiled = project.compile().unwrap();
    compiled.assert_success();
    assert!(compiled.find_first("Warning").is_some());

    assert!(
        !compiled
            .output()
            .errors
            .iter()
            .any(|err| err.is_warning() && err.message.contains("create")),
        "Assembly `create` warning was not suppressed: {:#?}",
        compiled.output().errors
    )
}

#[test]
fn test_zk_can_compile_contract_with_assembly_create_suppressed_warnings_1_5_10() {
    test_zksync_can_compile_contract_with_assembly_create_suppressed_warnings(Version::new(
        1, 5, 10,
    ));
}

#[test]
fn test_zk_can_compile_dapp_detect_changes_in_libs() {
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

    let graph = Graph::<SolParser>::resolve(project.paths()).unwrap();
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

    let graph = Graph::<SolParser>::resolve(project.paths()).unwrap();
    assert_eq!(graph.files().len(), 2);

    let compiled = project.compile().unwrap();
    assert!(compiled.find_first("Foo").is_some());
    assert!(compiled.find_first("Bar").is_some());
    // ensure change is detected
    assert!(!compiled.is_unchanged());
}

#[test]
fn test_zk_can_compile_dapp_detect_changes_in_sources() {
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

    let graph = Graph::<SolParser>::resolve(project.paths()).unwrap();
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
    let graph = Graph::<SolParser>::resolve(project.paths()).unwrap();
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
fn test_zk_can_emit_build_info() {
    let mut project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::dapptools().unwrap();
    project.project_mut().build_info = true;
    project
        .add_source(
            "A",
            r#"
pragma solidity 0.8.10;
import "./B.sol";
contract A { }
"#,
        )
        .unwrap();

    project
        .add_source(
            "B",
            r"
pragma solidity 0.8.10;
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
fn test_zk_can_clean_build_info() {
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
fn test_zk_cant_compile_a_file_outside_allowed_paths() {
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
    assert!(compiled.output().errors.iter().any(|error| {
        error
            .formatted_message
            .as_ref()
            .is_some_and(|msg| msg.contains("File outside of allowed directories"))
    }));
}

#[test]
fn test_zk_can_compile_a_file_in_allowed_paths_successfully() {
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
fn test_zk_can_compile_yul_sample() {
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
                .find(|(name, _)| name.ends_with("SimpleStore"))
                .and_then(|(_, artifacts)| artifacts.first())
        })
        .expect("SimpleStore artifact not found")
        .artifact
        .bytecode
        .clone()
        .unwrap();

    let yul_bytecode = simple_store_artifact.object().into_bytes().unwrap();

    assert!(!yul_bytecode.is_empty(), "SimpleStore bytecode is empty");
}

#[test]
fn test_zk_detects_change_on_cache_if_zksolc_version_changes() {
    let mut project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::dapptools().unwrap();

    project.project_mut().build_info = true;

    project
        .add_source(
            "A",
            r#"
pragma solidity 0.8.10;
import "./B.sol";
contract A { }
"#,
        )
        .unwrap();

    project
        .add_source(
            "B",
            r"
pragma solidity 0.8.10;
contract B { }
",
        )
        .unwrap();

    project.project_mut().settings.set_zksolc_version(Version::new(1, 5, 6)).unwrap();

    let compiled_1 = project.compile().unwrap();
    compiled_1.assert_success();

    for bi in compiled_1.output().build_infos.iter() {
        let zksolc_version =
            bi.build_info.get("output").unwrap()["metadata"]["zksolcVersion"].to_string();
        assert_eq!(zksolc_version, "\"1.5.6\"");
    }

    let compiled_2 = project.compile().unwrap();
    assert!(compiled_2.is_unchanged());

    project.project_mut().settings.set_zksolc_version(Version::new(1, 5, 7)).unwrap();

    let compiled_3 = project.compile().unwrap();
    compiled_3.assert_success();
    assert!(!compiled_3.is_unchanged());

    for bi in compiled_3.output().build_infos.iter() {
        let zksolc_version =
            bi.build_info.get("output").unwrap()["metadata"]["zksolcVersion"].to_string();
        assert_eq!(zksolc_version, "\"1.5.7\"");
    }
}

#[test]
fn test_zk_can_compile_with_ast_output() {
    let mut project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::dapptools().unwrap();

    // Configure output selection to include AST
    let mut settings = project.project().settings.clone();
    settings.settings.output_selection = OutputSelection {
        all: FileOutputSelection {
            per_file: [OutputSelectionFlag::AST].into(),
            per_contract: [OutputSelectionFlag::ABI, OutputSelectionFlag::Metadata].into(),
        },
    };
    project.project_mut().settings = settings;

    project
        .add_source(
            "TestContract",
            r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.10;

contract TestContract {
    uint256 public value;
    
    event ValueChanged(uint256 indexed newValue);
    
    constructor(uint256 _initialValue) {
        value = _initialValue;
    }
    
    function setValue(uint256 _newValue) public {
        value = _newValue;
        emit ValueChanged(_newValue);
    }
    
    function getValue() public view returns (uint256) {
        return value;
    }
}
"#,
        )
        .unwrap();

    let compiled = project.compile().unwrap();
    compiled.assert_success();

    let sources = &compiled.output().sources;
    let (_path, versioned_files) = sources
        .0
        .iter()
        .find(|(path, _)| {
            path.file_name().and_then(|name| name.to_str()) == Some("TestContract.sol")
        })
        .expect("TestContract.sol source not found");

    let versioned_source_file = &versioned_files[0]; // Get first version
    let source_file = &versioned_source_file.source_file;

    assert!(source_file.ast.is_some(), "AST should be present in source file");
    let ast =
        serde_json::to_value(source_file.ast.as_ref().unwrap()).expect("Failed to serialize AST");

    assert_eq!(ast["nodeType"].as_str(), Some("SourceUnit"), "AST root should be SourceUnit");
    assert!(ast["src"].is_string(), "AST should have src field");
    assert!(ast["nodes"].is_array(), "AST should have nodes array");

    let nodes = ast["nodes"].as_array().expect("nodes should be array");
    assert!(!nodes.is_empty(), "AST nodes should not be empty");

    // Find the contract definition node
    let contract_node = nodes
        .iter()
        .find(|node| {
            node["nodeType"].as_str() == Some("ContractDefinition")
                && node["name"].as_str() == Some("TestContract")
        })
        .expect("Should find TestContract definition in AST");

    assert!(contract_node["src"].is_string(), "Contract node should have src field");
    assert!(contract_node["nodes"].is_array(), "Contract should have nodes array");

    let contract_nodes = contract_node["nodes"].as_array().expect("Contract nodes should be array");

    let has_constructor = contract_nodes.iter().any(|node| {
        node["nodeType"].as_str() == Some("FunctionDefinition")
            && node["kind"].as_str() == Some("constructor")
    });
    assert!(has_constructor, "Should find constructor in AST");

    let has_set_value_function = contract_nodes.iter().any(|node| {
        node["nodeType"].as_str() == Some("FunctionDefinition")
            && node["name"].as_str() == Some("setValue")
    });
    assert!(has_set_value_function, "Should find setValue function in AST");

    let has_value_variable = contract_nodes.iter().any(|node| {
        node["nodeType"].as_str() == Some("VariableDeclaration")
            && node["name"].as_str() == Some("value")
    });
    assert!(has_value_variable, "Should find value variable in AST");

    let has_event = contract_nodes.iter().any(|node| {
        node["nodeType"].as_str() == Some("EventDefinition")
            && node["name"].as_str() == Some("ValueChanged")
    });
    assert!(has_event, "Should find ValueChanged event in AST");
}

#[test]
fn test_zk_ast_available_in_sources() {
    let mut project = TempProject::<ZkSolcCompiler, ZkArtifactOutput>::dapptools().unwrap();

    // Configure output selection to include AST
    let mut settings = project.project().settings.clone();
    settings.settings.output_selection = OutputSelection {
        all: FileOutputSelection {
            per_file: [OutputSelectionFlag::AST].into(),
            per_contract: [OutputSelectionFlag::ABI].into(),
        },
    };
    project.project_mut().settings = settings;

    project
        .add_source(
            "SimpleAstTest",
            r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.10;

contract SimpleAstTest {
    uint256 public counter;
    
    function increment() public {
        counter += 1;
    }
}
"#,
        )
        .unwrap();

    let compiled = project.compile().unwrap();
    compiled.assert_success();

    let sources = &compiled.output().sources;
    let (_path, versioned_files) = sources
        .0
        .iter()
        .find(|(path, _)| {
            path.file_name().and_then(|name| name.to_str()) == Some("SimpleAstTest.sol")
        })
        .expect("SimpleAstTest.sol source not found");

    let versioned_source_file = &versioned_files[0]; // Get first version
    let source_file = &versioned_source_file.source_file;

    assert!(source_file.ast.is_some(), "AST should be present in source file");
    let ast =
        serde_json::to_value(source_file.ast.as_ref().unwrap()).expect("Failed to serialize AST");

    assert_eq!(ast["nodeType"].as_str(), Some("SourceUnit"));

    let nodes = ast["nodes"].as_array().expect("AST should have nodes");
    let contract_node = nodes
        .iter()
        .find(|node| {
            node["nodeType"].as_str() == Some("ContractDefinition")
                && node["name"].as_str() == Some("SimpleAstTest")
        })
        .expect("Should find SimpleAstTest in AST");

    let contract_elements = contract_node["nodes"].as_array().expect("Contract should have nodes");
    let has_counter_var = contract_elements.iter().any(|node| {
        node["nodeType"].as_str() == Some("VariableDeclaration")
            && node["name"].as_str() == Some("counter")
    });
    assert!(has_counter_var, "Should find counter variable in AST");
}
