//! # foundry-linking
//!
//! EVM bytecode linker.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use alloy_primitives::{
    hex::FromHex,
    map::{HashMap, HashSet},
    Address, Bytes, B256,
};
use foundry_compilers::{
    artifacts::{
        BytecodeObject, CompactBytecode, CompactContractBytecode, CompactContractBytecodeCow,
        CompactDeployedBytecode, Libraries,
    },
    contracts::ArtifactContracts,
    Artifact, ArtifactId,
};
use foundry_zksync_compilers::{
    compilers::zksolc::{ZkSolc, ZkSolcCompiler},
    link::{self as zk_link, MissingLibrary},
};
use semver::Version;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    str::FromStr,
};

/// Errors that can occur during linking.
#[derive(Debug, thiserror::Error)]
pub enum LinkerError {
    #[error("wasn't able to find artifact for library {name} at {file}")]
    MissingLibraryArtifact { file: String, name: String },
    #[error("target artifact is not present in provided artifacts set")]
    MissingTargetArtifact,
    #[error(transparent)]
    InvalidAddress(<Address as std::str::FromStr>::Err),
    #[error("cyclic dependency found, can't link libraries via CREATE2")]
    CyclicDependency,
}

/// Errors that can occur during linking.
#[derive(Debug, thiserror::Error)]
pub enum ZkLinkerError {
    #[error(transparent)]
    Inner(#[from] LinkerError),
    #[error("unable to fully link due to missing libraries")]
    MissingLibraries(BTreeSet<MissingLibrary>),
    #[error("unable to fully link due to unlinked factory dependencies")]
    MissingFactoryDeps(BTreeSet<MissingLibrary>),
}

#[derive(Debug)]
pub struct Linker<'a> {
    /// Root of the project, used to determine whether artifact/library path can be stripped.
    pub root: PathBuf,
    /// Compilation artifacts.
    pub contracts: ArtifactContracts<CompactContractBytecodeCow<'a>>,
}

/// Output of the `link_with_nonce_or_address`
pub struct LinkOutput {
    /// Resolved library addresses. Contains both user-provided and newly deployed libraries.
    /// It will always contain library paths with stripped path prefixes.
    pub libraries: Libraries,
    /// Vector of libraries that need to be deployed from sender address.
    /// The order in which they appear in the vector is the order in which they should be deployed.
    pub libs_to_deploy: Vec<Bytes>,
}

impl<'a> Linker<'a> {
    pub fn new(
        root: impl Into<PathBuf>,
        contracts: ArtifactContracts<CompactContractBytecodeCow<'a>>,
    ) -> Self {
        Linker { root: root.into(), contracts }
    }

    /// Helper method to convert [ArtifactId] to the format in which libraries are stored in
    /// [Libraries] object.
    ///
    /// Strips project root path from source file path.
    fn convert_artifact_id_to_lib_path(&self, id: &ArtifactId) -> (PathBuf, String) {
        let path = id.source.strip_prefix(self.root.as_path()).unwrap_or(&id.source);
        // name is either {LibName} or {LibName}.{version}
        let name = id.name.split('.').next().unwrap();

        (path.to_path_buf(), name.to_owned())
    }

    /// Finds an [ArtifactId] object in the given [ArtifactContracts] keys which corresponds to the
    /// library path in the form of "./path/to/Lib.sol:Lib"
    ///
    /// Optionally accepts solc version, and if present, only compares artifacts with given version.
    fn find_artifact_id_by_library_path(
        &'a self,
        file: &str,
        name: &str,
        version: Option<&Version>,
    ) -> Option<&'a ArtifactId> {
        for id in self.contracts.keys() {
            if let Some(version) = version {
                if id.version != *version {
                    continue;
                }
            }
            let (artifact_path, artifact_name) = self.convert_artifact_id_to_lib_path(id);

            if artifact_name == *name && artifact_path == Path::new(file) {
                return Some(id);
            }
        }

        None
    }

    /// Performs DFS on the graph of link references, and populates `deps` with all found libraries.
    fn collect_dependencies(
        &'a self,
        target: &'a ArtifactId,
        deps: &mut BTreeSet<&'a ArtifactId>,
    ) -> Result<(), LinkerError> {
        let contract = self.contracts.get(target).ok_or(LinkerError::MissingTargetArtifact)?;

        let mut references = BTreeMap::new();
        if let Some(bytecode) = &contract.bytecode {
            references.extend(bytecode.link_references.clone());
        }
        if let Some(deployed_bytecode) = &contract.deployed_bytecode {
            if let Some(bytecode) = &deployed_bytecode.bytecode {
                references.extend(bytecode.link_references.clone());
            }
        }

        for (file, libs) in &references {
            for contract in libs.keys() {
                let id = self
                    .find_artifact_id_by_library_path(file, contract, Some(&target.version))
                    .ok_or_else(|| LinkerError::MissingLibraryArtifact {
                        file: file.to_string(),
                        name: contract.to_string(),
                    })?;
                if deps.insert(id) {
                    self.collect_dependencies(id, deps)?;
                }
            }
        }

        Ok(())
    }

    /// Links given artifact with either given library addresses or address computed from sender and
    /// nonce.
    ///
    /// Each key in `libraries` should either be a global path or relative to project root. All
    /// remappings should be resolved.
    ///
    /// When calling for `target` being an external library itself, you should check that `target`
    /// does not appear in `libs_to_deploy` to avoid deploying it twice. It may happen in cases
    /// when there is a dependency cycle including `target`.
    pub fn link_with_nonce_or_address(
        &'a self,
        libraries: Libraries,
        sender: Address,
        mut nonce: u64,
        targets: impl IntoIterator<Item = &'a ArtifactId>,
    ) -> Result<LinkOutput, LinkerError> {
        // Library paths in `link_references` keys are always stripped, so we have to strip
        // user-provided paths to be able to match them correctly.
        let mut libraries = libraries.with_stripped_file_prefixes(self.root.as_path());

        let mut needed_libraries = BTreeSet::new();
        for target in targets {
            self.collect_dependencies(target, &mut needed_libraries)?;
        }

        let mut libs_to_deploy = Vec::new();

        // If `libraries` does not contain needed dependency, compute its address and add to
        // `libs_to_deploy`.
        for id in needed_libraries {
            let (lib_path, lib_name) = self.convert_artifact_id_to_lib_path(id);

            libraries.libs.entry(lib_path).or_default().entry(lib_name).or_insert_with(|| {
                let address = sender.create(nonce);
                libs_to_deploy.push((id, address));
                nonce += 1;

                address.to_checksum(None)
            });
        }

        // Link and collect bytecodes for `libs_to_deploy`.
        let libs_to_deploy = libs_to_deploy
            .into_iter()
            .map(|(id, _)| {
                Ok(self.link(id, &libraries)?.get_bytecode_bytes().unwrap().into_owned())
            })
            .collect::<Result<Vec<_>, LinkerError>>()?;

        Ok(LinkOutput { libraries, libs_to_deploy })
    }

    /// Links given artifact with either given library addresses or address computed from sender and
    /// nonce.
    ///
    /// Each key in `libraries` should either be a global path or relative to project root. All
    /// remappings should be resolved.
    ///
    /// When calling for `target` being an external library itself, you should check that `target`
    /// does not appear in `libs_to_deploy` to avoid deploying it twice. It may happen in cases
    /// when there is a dependency cycle including `target`.
    pub fn zk_link_with_nonce_or_address(
        &'a self,
        libraries: Libraries,
        sender: Address,
        mut nonce: u64,
        targets: impl IntoIterator<Item = &'a ArtifactId>,
    ) -> Result<LinkOutput, ZkLinkerError> {
        // Library paths in `link_references` keys are always stripped, so we have to strip
        // user-provided paths to be able to match them correctly.
        let mut libraries = libraries.with_stripped_file_prefixes(self.root.as_path());

        let mut needed_libraries = BTreeSet::new();
        for target in targets {
            self.collect_dependencies(target, &mut needed_libraries)?;
        }

        let mut libs_to_deploy = Vec::new();

        // If `libraries` does not contain needed dependency, compute its address and add to
        // `libs_to_deploy`.
        for id in needed_libraries {
            let (lib_path, lib_name) = self.convert_artifact_id_to_lib_path(id);

            libraries.libs.entry(lib_path).or_default().entry(lib_name).or_insert_with(|| {
                let address = foundry_zksync_core::compute_create_address(sender, nonce);
                libs_to_deploy.push((id, address));
                nonce += 1;

                address.to_checksum(None)
            });
        }

        // Link and collect bytecodes for `libs_to_deploy`.
        let libs_to_deploy = self
            .zk_get_linked_artifacts(libs_to_deploy.into_iter().map(|(id, _)| id), &libraries)?
            .into_iter()
            .map(|(_, linked)| linked.get_bytecode_bytes().unwrap().into_owned())
            .collect();

        Ok(LinkOutput { libraries, libs_to_deploy })
    }

    // TODO(zk): zk_link_with_create2
    // a bit more difficult due to the lack of bytecode
    // until the contract is fully linked
    pub fn link_with_create2(
        &'a self,
        libraries: Libraries,
        sender: Address,
        salt: B256,
        target: &'a ArtifactId,
    ) -> Result<LinkOutput, LinkerError> {
        // Library paths in `link_references` keys are always stripped, so we have to strip
        // user-provided paths to be able to match them correctly.
        let mut libraries = libraries.with_stripped_file_prefixes(self.root.as_path());

        let mut needed_libraries = BTreeSet::new();
        self.collect_dependencies(target, &mut needed_libraries)?;

        let mut needed_libraries = needed_libraries
            .into_iter()
            .filter(|id| {
                // Filter out already provided libraries.
                let (file, name) = self.convert_artifact_id_to_lib_path(id);
                !libraries.libs.contains_key(&file) || !libraries.libs[&file].contains_key(&name)
            })
            .map(|id| {
                // Link library with provided libs and extract bytecode object (possibly unlinked).
                let bytecode = self.link(id, &libraries).unwrap().bytecode.unwrap();
                (id, bytecode)
            })
            .collect::<Vec<_>>();

        let mut libs_to_deploy = Vec::new();

        // Iteratively compute addresses and link libraries until we have no unlinked libraries
        // left.
        while !needed_libraries.is_empty() {
            // Find any library which is fully linked.
            let deployable = needed_libraries
                .iter()
                .enumerate()
                .find(|(_, (_, bytecode))| !bytecode.object.is_unlinked());

            // If we haven't found any deployable library, it means we have a cyclic dependency.
            let Some((index, &(id, _))) = deployable else {
                return Err(LinkerError::CyclicDependency);
            };
            let (_, bytecode) = needed_libraries.swap_remove(index);
            let code = bytecode.bytes().unwrap();
            let address = sender.create2_from_code(salt, code);
            libs_to_deploy.push(code.clone());

            let (file, name) = self.convert_artifact_id_to_lib_path(id);

            for (_, bytecode) in &mut needed_libraries {
                bytecode.to_mut().link(&file.to_string_lossy(), &name, address);
            }

            libraries.libs.entry(file).or_default().insert(name, address.to_checksum(None));
        }

        Ok(LinkOutput { libraries, libs_to_deploy })
    }

    /// Links given artifact with given libraries.
    pub fn link(
        &self,
        target: &ArtifactId,
        libraries: &Libraries,
    ) -> Result<CompactContractBytecodeCow<'a>, LinkerError> {
        let mut contract =
            self.contracts.get(target).ok_or(LinkerError::MissingTargetArtifact)?.clone();
        for (file, libs) in &libraries.libs {
            for (name, address) in libs {
                let address = Address::from_str(address).map_err(LinkerError::InvalidAddress)?;
                if let Some(bytecode) = contract.bytecode.as_mut() {
                    bytecode.to_mut().link(&file.to_string_lossy(), name, address);
                }
                if let Some(deployed_bytecode) =
                    contract.deployed_bytecode.as_mut().and_then(|b| b.to_mut().bytecode.as_mut())
                {
                    deployed_bytecode.link(&file.to_string_lossy(), name, address);
                }
            }
        }
        Ok(contract)
    }

    /// Links given artifact with given libraries.
    // TODO(zk): improve interface to reflect batching operation (all bytecodes in all bytecodes
    // out)
    pub fn zk_link(
        contracts: &ArtifactContracts<CompactContractBytecodeCow<'a>>,
        target: &ArtifactId,
        libraries: &Libraries,
    ) -> Result<CompactContractBytecodeCow<'a>, ZkLinkerError> {
        let artifact_to_link_id = |id: &ArtifactId| format!("{}:{}", id.source.display(), id.name);

        // collect bytecodes & libraries for input to zksolc_link
        let bytecodes = contracts
            .iter()
            .filter_map(|(id, bytecode)| {
                let link_id = artifact_to_link_id(id);
                let object = bytecode.bytecode.as_ref().map(|bc| bc.object.clone())?;

                let bytes = match object {
                    BytecodeObject::Bytecode(bytes) => bytes,
                    BytecodeObject::Unlinked(unlinked) => alloy_primitives::hex::decode(unlinked)
                        .expect("malformed unlinked bytecode object")
                        .into(),
                };

                Some((link_id, bytes))
            })
            .collect::<HashMap<_, _>>();

        let libraries = libraries
            .libs
            .iter()
            .flat_map(|(file, libs)| {
                libs.iter()
                    .map(|(name, address)| (file.to_string_lossy(), name.clone(), address.clone()))
            })
            .map(|(filename, name, address)| zk_link::Library {
                filename: filename.into_owned(),
                name,
                address: Address::from_hex(address).unwrap(),
            })
            .collect::<HashSet<_>>();

        let zksolc = ZkSolcCompiler {
            // NOTE(zk): zksolc --link --standard-json requires >1.5.8
            // FIXME(zk): compiler from config
            zksolc: ZkSolc::get_path_for_version(&Version::new(1, 5, 8)).unwrap(),
            solc: Default::default(),
        };
        let mut link_output =
            zk_link::zksolc_link(&zksolc, zk_link::LinkJsonInput { bytecodes, libraries })
                .expect("able to call zksolc --link"); // FIXME: proper error check

        let link_id = &artifact_to_link_id(target);

        let mut contract = contracts.get(target).ok_or(LinkerError::MissingTargetArtifact)?.clone();

        if let Some(unlinked) = link_output.unlinked.remove(link_id) {
            tracing::error!(factory_dependencies = ?unlinked.factory_dependencies, libraries = ?unlinked.linker_symbols, "unmet linking dependencies");

            if !unlinked.linker_symbols.is_empty() {
                return Err(ZkLinkerError::MissingLibraries(
                    unlinked.linker_symbols.into_iter().collect(),
                ));
            }
            return Err(ZkLinkerError::MissingFactoryDeps(
                unlinked.factory_dependencies.into_iter().collect(),
            ));
        }

        let linked_output =
            link_output.linked.remove(link_id).or_else(|| link_output.ignored.remove(link_id));

        // NOTE(zk): covers intermittent issue where fully linked bytecode was
        // not being returned in `ignored` (or `linked`).
        // The check above should catch if the bytecode remains unlinked
        let Some(linked) = linked_output else {
            return Ok(contract);
        };

        let mut compact_bytecode = CompactBytecode::empty();
        compact_bytecode.object = BytecodeObject::Bytecode(
            alloy_primitives::hex::decode(&linked.bytecode)
                .expect("malformed unlinked bytecode object")
                .into(),
        );

        let mut compact_deployed_bytecode = CompactDeployedBytecode::empty();
        compact_deployed_bytecode.bytecode.replace(compact_bytecode.clone());

        // TODO(zk): maybe return bytecode hash?
        contract.bytecode.replace(std::borrow::Cow::Owned(compact_bytecode));
        contract.deployed_bytecode.replace(std::borrow::Cow::Owned(compact_deployed_bytecode));

        Ok(contract)
    }

    pub fn get_linked_artifacts(
        &self,
        libraries: &Libraries,
    ) -> Result<ArtifactContracts, LinkerError> {
        self.contracts.keys().map(|id| Ok((id.clone(), self.link(id, libraries)?))).collect()
    }

    pub fn zk_get_linked_artifacts<'b>(
        &self,
        targets: impl IntoIterator<Item = &'b ArtifactId>,
        libraries: &Libraries,
    ) -> Result<ArtifactContracts, ZkLinkerError> {
        let mut targets = targets.into_iter().cloned().collect::<VecDeque<_>>();
        let mut contracts = self.contracts.clone();
        let mut linked_artifacts = vec![];

        // FIXME(zk): determine if this loop is still needed like this
        while let Some(id) = targets.pop_front() {
            match Self::zk_link(&contracts, &id, &libraries) {
                Ok(linked) => {
                    // persist linked contract for successive iterations
                    *contracts.entry(id.clone()).or_default() = linked.clone();

                    linked_artifacts.push((id.clone(), CompactContractBytecode::from(linked)));
                }
                Err(ZkLinkerError::MissingFactoryDeps(fdeps)) => {
                    // attempt linking again if some factory dep remains unlinked
                    // this is just in the case where a previously unlinked factory dep
                    // is linked with the same run as `id` would be linked
                    // and instead `id` remains unlinked
                    // FIXME(zk): might be unnecessary, observed when paths were wrong
                    let mut ids = fdeps
                        .into_iter()
                        .inspect(|fdep| {
                            dbg!(&fdep);
                        })
                        .flat_map(|fdep| {
                            contracts.iter().find(|(id, _)| {
                                id.source.as_path() == Path::new(fdep.filename.as_str()) &&
                                    &id.name == &fdep.library
                            })
                        })
                        .map(|(id, _)| id.clone())
                        .peekable();

                    // if we have no dep ids then we avoid
                    // queueing our own id to avoid infinite loop
                    // TODO(zk): find a better way to avoid issues later
                    if let Some(_) = ids.peek() {
                        targets.extend(ids); // queue factory deps for linking
                        targets.push_back(id); // reque original target
                    }
                }
                Err(err) => return Err(err),
            }
        }

        Ok(linked_artifacts.into_iter().collect())
    }

    pub fn get_linked_artifacts_cow(
        &self,
        libraries: &Libraries,
    ) -> Result<ArtifactContracts<CompactContractBytecodeCow<'a>>, LinkerError> {
        self.contracts.keys().map(|id| Ok((id.clone(), self.link(id, libraries)?))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{fixed_bytes, map::HashMap};
    use foundry_compilers::{
        multi::MultiCompiler,
        solc::{Solc, SolcCompiler},
        Project, ProjectCompileOutput, ProjectPathsConfig,
    };

    struct LinkerTest {
        project: Project,
        output: ProjectCompileOutput,
        dependency_assertions: HashMap<String, Vec<(String, Address)>>,
    }

    impl LinkerTest {
        fn new(path: impl Into<PathBuf>, strip_prefixes: bool) -> Self {
            let path = path.into();
            let paths = ProjectPathsConfig::builder()
                .root("../../testdata")
                .lib("../../testdata/lib")
                .sources(path.clone())
                .tests(path)
                .build()
                .unwrap();

            let solc = Solc::find_or_install(&Version::new(0, 8, 18)).unwrap();
            let project = Project::builder()
                .paths(paths)
                .ephemeral()
                .no_artifacts()
                .build(MultiCompiler { solc: Some(SolcCompiler::Specific(solc)), vyper: None })
                .unwrap();

            let mut output = project.compile().unwrap();

            if strip_prefixes {
                output = output.with_stripped_file_prefixes(project.root());
            }

            Self { project, output, dependency_assertions: HashMap::default() }
        }

        fn assert_dependencies(
            mut self,
            artifact_id: String,
            deps: Vec<(String, Address)>,
        ) -> Self {
            self.dependency_assertions.insert(artifact_id, deps);
            self
        }

        fn test_with_sender_and_nonce(self, sender: Address, initial_nonce: u64) {
            let linker = Linker::new(self.project.root(), self.output.artifact_ids().collect());
            for (id, identifier) in self.iter_linking_targets(&linker) {
                let output = linker
                    .link_with_nonce_or_address(Default::default(), sender, initial_nonce, [id])
                    .expect("Linking failed");
                self.validate_assertions(identifier, output);
            }
        }

        fn test_with_create2(self, sender: Address, salt: B256) {
            let linker = Linker::new(self.project.root(), self.output.artifact_ids().collect());
            for (id, identifier) in self.iter_linking_targets(&linker) {
                let output = linker
                    .link_with_create2(Default::default(), sender, salt, id)
                    .expect("Linking failed");
                self.validate_assertions(identifier, output);
            }
        }

        fn iter_linking_targets<'a>(
            &'a self,
            linker: &'a Linker<'_>,
        ) -> impl IntoIterator<Item = (&'a ArtifactId, String)> + 'a {
            linker.contracts.keys().filter_map(move |id| {
                // If we didn't strip paths, artifacts will have absolute paths.
                // That's expected and we want to ensure that only `libraries` object has relative
                // paths, artifacts should be kept as is.
                let source = id
                    .source
                    .strip_prefix(self.project.root())
                    .unwrap_or(&id.source)
                    .to_string_lossy();
                let identifier = format!("{source}:{}", id.name);

                // Skip ds-test as it always has no dependencies etc. (and the path is outside root
                // so is not sanitized)
                if identifier.contains("DSTest") {
                    return None;
                }

                Some((id, identifier))
            })
        }

        fn validate_assertions(&self, identifier: String, output: LinkOutput) {
            let LinkOutput { libs_to_deploy, libraries } = output;

            let assertions = self
                .dependency_assertions
                .get(&identifier)
                .unwrap_or_else(|| panic!("Unexpected artifact: {identifier}"));

            assert_eq!(
                libs_to_deploy.len(),
                assertions.len(),
                "artifact {identifier} has more/less dependencies than expected ({} vs {}): {:#?}",
                libs_to_deploy.len(),
                assertions.len(),
                libs_to_deploy
            );

            for (dep_identifier, address) in assertions {
                let (file, name) = dep_identifier.split_once(':').unwrap();
                if let Some(lib_address) =
                    libraries.libs.get(Path::new(file)).and_then(|libs| libs.get(name))
                {
                    assert_eq!(
                        *lib_address,
                        address.to_string(),
                        "incorrect library address for dependency {dep_identifier} of {identifier}"
                    );
                } else {
                    panic!("Library {dep_identifier} not found");
                }
            }
        }
    }

    fn link_test(path: impl Into<PathBuf>, test_fn: impl Fn(LinkerTest)) {
        let path = path.into();
        test_fn(LinkerTest::new(path.clone(), true));
        test_fn(LinkerTest::new(path, false));
    }

    #[test]
    fn link_simple() {
        link_test("../../testdata/default/linking/simple", |linker| {
            linker
                .assert_dependencies("default/linking/simple/Simple.t.sol:Lib".to_string(), vec![])
                .assert_dependencies(
                    "default/linking/simple/Simple.t.sol:LibraryConsumer".to_string(),
                    vec![(
                        "default/linking/simple/Simple.t.sol:Lib".to_string(),
                        Address::from_str("0x5a443704dd4b594b382c22a083e2bd3090a6fef3").unwrap(),
                    )],
                )
                .assert_dependencies(
                    "default/linking/simple/Simple.t.sol:SimpleLibraryLinkingTest".to_string(),
                    vec![(
                        "default/linking/simple/Simple.t.sol:Lib".to_string(),
                        Address::from_str("0x5a443704dd4b594b382c22a083e2bd3090a6fef3").unwrap(),
                    )],
                )
                .test_with_sender_and_nonce(Address::default(), 1);
        });
    }

    #[test]
    fn link_nested() {
        link_test("../../testdata/default/linking/nested", |linker| {
            linker
                .assert_dependencies("default/linking/nested/Nested.t.sol:Lib".to_string(), vec![])
                .assert_dependencies(
                    "default/linking/nested/Nested.t.sol:NestedLib".to_string(),
                    vec![(
                        "default/linking/nested/Nested.t.sol:Lib".to_string(),
                        Address::from_str("0x5a443704dd4b594b382c22a083e2bd3090a6fef3").unwrap(),
                    )],
                )
                .assert_dependencies(
                    "default/linking/nested/Nested.t.sol:LibraryConsumer".to_string(),
                    vec![
                        // Lib shows up here twice, because the linker sees it twice, but it should
                        // have the same address and nonce.
                        (
                            "default/linking/nested/Nested.t.sol:Lib".to_string(),
                            Address::from_str("0x5a443704dd4b594b382c22a083e2bd3090a6fef3")
                                .unwrap(),
                        ),
                        (
                            "default/linking/nested/Nested.t.sol:NestedLib".to_string(),
                            Address::from_str("0x47e9Fbef8C83A1714F1951F142132E6e90F5fa5D")
                                .unwrap(),
                        ),
                    ],
                )
                .assert_dependencies(
                    "default/linking/nested/Nested.t.sol:NestedLibraryLinkingTest".to_string(),
                    vec![
                        (
                            "default/linking/nested/Nested.t.sol:Lib".to_string(),
                            Address::from_str("0x5a443704dd4b594b382c22a083e2bd3090a6fef3")
                                .unwrap(),
                        ),
                        (
                            "default/linking/nested/Nested.t.sol:NestedLib".to_string(),
                            Address::from_str("0x47e9fbef8c83a1714f1951f142132e6e90f5fa5d")
                                .unwrap(),
                        ),
                    ],
                )
                .test_with_sender_and_nonce(Address::default(), 1);
        });
    }

    #[test]
    fn link_duplicate() {
        link_test("../../testdata/default/linking/duplicate", |linker| {
            linker
                .assert_dependencies(
                    "default/linking/duplicate/Duplicate.t.sol:A".to_string(),
                    vec![],
                )
                .assert_dependencies(
                    "default/linking/duplicate/Duplicate.t.sol:B".to_string(),
                    vec![],
                )
                .assert_dependencies(
                    "default/linking/duplicate/Duplicate.t.sol:C".to_string(),
                    vec![(
                        "default/linking/duplicate/Duplicate.t.sol:A".to_string(),
                        Address::from_str("0x5a443704dd4b594b382c22a083e2bd3090a6fef3").unwrap(),
                    )],
                )
                .assert_dependencies(
                    "default/linking/duplicate/Duplicate.t.sol:D".to_string(),
                    vec![(
                        "default/linking/duplicate/Duplicate.t.sol:B".to_string(),
                        Address::from_str("0x5a443704dd4b594b382c22a083e2bd3090a6fef3").unwrap(),
                    )],
                )
                .assert_dependencies(
                    "default/linking/duplicate/Duplicate.t.sol:E".to_string(),
                    vec![
                        (
                            "default/linking/duplicate/Duplicate.t.sol:A".to_string(),
                            Address::from_str("0x5a443704dd4b594b382c22a083e2bd3090a6fef3")
                                .unwrap(),
                        ),
                        (
                            "default/linking/duplicate/Duplicate.t.sol:C".to_string(),
                            Address::from_str("0x47e9fbef8c83a1714f1951f142132e6e90f5fa5d")
                                .unwrap(),
                        ),
                    ],
                )
                .assert_dependencies(
                    "default/linking/duplicate/Duplicate.t.sol:LibraryConsumer".to_string(),
                    vec![
                        (
                            "default/linking/duplicate/Duplicate.t.sol:A".to_string(),
                            Address::from_str("0x5a443704dd4b594b382c22a083e2bd3090a6fef3")
                                .unwrap(),
                        ),
                        (
                            "default/linking/duplicate/Duplicate.t.sol:B".to_string(),
                            Address::from_str("0x47e9fbef8c83a1714f1951f142132e6e90f5fa5d")
                                .unwrap(),
                        ),
                        (
                            "default/linking/duplicate/Duplicate.t.sol:C".to_string(),
                            Address::from_str("0x8be503bcded90ed42eff31f56199399b2b0154ca")
                                .unwrap(),
                        ),
                        (
                            "default/linking/duplicate/Duplicate.t.sol:D".to_string(),
                            Address::from_str("0x47c5e40890bce4a473a49d7501808b9633f29782")
                                .unwrap(),
                        ),
                        (
                            "default/linking/duplicate/Duplicate.t.sol:E".to_string(),
                            Address::from_str("0x29b2440db4a256b0c1e6d3b4cdcaa68e2440a08f")
                                .unwrap(),
                        ),
                    ],
                )
                .assert_dependencies(
                    "default/linking/duplicate/Duplicate.t.sol:DuplicateLibraryLinkingTest"
                        .to_string(),
                    vec![
                        (
                            "default/linking/duplicate/Duplicate.t.sol:A".to_string(),
                            Address::from_str("0x5a443704dd4b594b382c22a083e2bd3090a6fef3")
                                .unwrap(),
                        ),
                        (
                            "default/linking/duplicate/Duplicate.t.sol:B".to_string(),
                            Address::from_str("0x47e9fbef8c83a1714f1951f142132e6e90f5fa5d")
                                .unwrap(),
                        ),
                        (
                            "default/linking/duplicate/Duplicate.t.sol:C".to_string(),
                            Address::from_str("0x8be503bcded90ed42eff31f56199399b2b0154ca")
                                .unwrap(),
                        ),
                        (
                            "default/linking/duplicate/Duplicate.t.sol:D".to_string(),
                            Address::from_str("0x47c5e40890bce4a473a49d7501808b9633f29782")
                                .unwrap(),
                        ),
                        (
                            "default/linking/duplicate/Duplicate.t.sol:E".to_string(),
                            Address::from_str("0x29b2440db4a256b0c1e6d3b4cdcaa68e2440a08f")
                                .unwrap(),
                        ),
                    ],
                )
                .test_with_sender_and_nonce(Address::default(), 1);
        });
    }

    #[test]
    fn link_cycle() {
        link_test("../../testdata/default/linking/cycle", |linker| {
            linker
                .assert_dependencies(
                    "default/linking/cycle/Cycle.t.sol:Foo".to_string(),
                    vec![
                        (
                            "default/linking/cycle/Cycle.t.sol:Foo".to_string(),
                            Address::from_str("0x47e9Fbef8C83A1714F1951F142132E6e90F5fa5D")
                                .unwrap(),
                        ),
                        (
                            "default/linking/cycle/Cycle.t.sol:Bar".to_string(),
                            Address::from_str("0x5a443704dd4B594B382c22a083e2BD3090A6feF3")
                                .unwrap(),
                        ),
                    ],
                )
                .assert_dependencies(
                    "default/linking/cycle/Cycle.t.sol:Bar".to_string(),
                    vec![
                        (
                            "default/linking/cycle/Cycle.t.sol:Foo".to_string(),
                            Address::from_str("0x47e9Fbef8C83A1714F1951F142132E6e90F5fa5D")
                                .unwrap(),
                        ),
                        (
                            "default/linking/cycle/Cycle.t.sol:Bar".to_string(),
                            Address::from_str("0x5a443704dd4B594B382c22a083e2BD3090A6feF3")
                                .unwrap(),
                        ),
                    ],
                )
                .test_with_sender_and_nonce(Address::default(), 1);
        });
    }

    #[test]
    fn link_create2_nested() {
        link_test("../../testdata/default/linking/nested", |linker| {
            linker
                .assert_dependencies("default/linking/nested/Nested.t.sol:Lib".to_string(), vec![])
                .assert_dependencies(
                    "default/linking/nested/Nested.t.sol:NestedLib".to_string(),
                    vec![(
                        "default/linking/nested/Nested.t.sol:Lib".to_string(),
                        Address::from_str("0xddb1Cd2497000DAeA687CEa3dc34Af44084BEa74").unwrap(),
                    )],
                )
                .assert_dependencies(
                    "default/linking/nested/Nested.t.sol:LibraryConsumer".to_string(),
                    vec![
                        // Lib shows up here twice, because the linker sees it twice, but it should
                        // have the same address and nonce.
                        (
                            "default/linking/nested/Nested.t.sol:Lib".to_string(),
                            Address::from_str("0xddb1Cd2497000DAeA687CEa3dc34Af44084BEa74")
                                .unwrap(),
                        ),
                        (
                            "default/linking/nested/Nested.t.sol:NestedLib".to_string(),
                            Address::from_str("0xfebE2F30641170642f317Ff6F644Cee60E7Ac369")
                                .unwrap(),
                        ),
                    ],
                )
                .assert_dependencies(
                    "default/linking/nested/Nested.t.sol:NestedLibraryLinkingTest".to_string(),
                    vec![
                        (
                            "default/linking/nested/Nested.t.sol:Lib".to_string(),
                            Address::from_str("0xddb1Cd2497000DAeA687CEa3dc34Af44084BEa74")
                                .unwrap(),
                        ),
                        (
                            "default/linking/nested/Nested.t.sol:NestedLib".to_string(),
                            Address::from_str("0xfebE2F30641170642f317Ff6F644Cee60E7Ac369")
                                .unwrap(),
                        ),
                    ],
                )
                .test_with_create2(
                    Address::default(),
                    fixed_bytes!(
                        "19bf59b7b67ae8edcbc6e53616080f61fa99285c061450ad601b0bc40c9adfc9"
                    ),
                );
        });
    }
}
