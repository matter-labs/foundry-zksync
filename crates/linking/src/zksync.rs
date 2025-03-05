use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
};

use alloy_primitives::{
    hex::FromHex,
    map::{HashMap, HashSet},
    Address, B256,
};
use foundry_compilers::{
    artifacts::{
        BytecodeObject, CompactBytecode, CompactContractBytecode, CompactContractBytecodeCow,
        CompactDeployedBytecode, Libraries,
    },
    contracts::ArtifactContracts,
    Artifact, ArtifactId, ProjectCompileOutput,
};
use foundry_zksync_compilers::{
    compilers::{
        artifact_output::zk::{ZkArtifactOutput, ZkContractArtifact},
        zksolc::ZkSolcCompiler,
    },
    link::{self as zk_link, MissingLibrary},
};
use foundry_zksync_core::{hash_bytecode, H256};
use semver::Version;

use crate::{LinkOutput, Linker, LinkerError};

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

pub const DEPLOY_TIME_LINKING_ZKSOLC_MIN_VERSION: Version = Version::new(1, 5, 9);

#[derive(Debug)]
pub struct ZkLinker<'a> {
    pub linker: Linker<'a>,
    pub compiler: PathBuf,
    pub compiler_output: &'a ProjectCompileOutput<ZkSolcCompiler, ZkArtifactOutput>,
}

impl<'a> ZkLinker<'a> {
    fn zk_artifacts(&'a self) -> impl Iterator<Item = (ArtifactId, &'a ZkContractArtifact)> + 'a {
        self.compiler_output.artifact_ids()
    }

    /// Construct a new `ZkLinker`
    pub fn new(
        root: impl Into<PathBuf>,
        contracts: ArtifactContracts<CompactContractBytecodeCow<'a>>,
        compiler: PathBuf,
        compiler_output: &'a ProjectCompileOutput<ZkSolcCompiler, ZkArtifactOutput>,
    ) -> Self {
        Self { linker: Linker::new(root, contracts), compiler, compiler_output }
    }

    /// Collect the factory dependencies of the `target` artifact
    ///
    /// Will call itself recursively for nested dependencies
    fn zk_collect_factory_deps(
        &'a self,
        target: &'a ArtifactId,
        factory_deps: &mut BTreeSet<&'a ArtifactId>,
    ) -> Result<(), LinkerError> {
        let (_, artifact) = self
            .zk_artifacts()
            .find(|(id, _)| id.source == target.source && id.name == target.name)
            .ok_or(LinkerError::MissingTargetArtifact)?;

        let already_linked = artifact
            .factory_dependencies
            .as_ref()
            .iter()
            .flat_map(|map| map.values())
            .collect::<Vec<_>>();

        let unlinked_deps_of_target = artifact
            .factory_dependencies_unlinked
            .iter()
            .flatten()
            // remove already linked deps
            .filter(|dep| !already_linked.contains(dep))
            .map(|dep| {
                let mut split = dep.split(':');
                let path = split.next().expect("malformed factory dep path");
                let name = split.next().expect("malformed factory dep name");

                (path.to_string(), name.to_string())
            });

        for (file, name) in unlinked_deps_of_target {
            let id = self
                .linker
                .find_artifact_id_by_library_path(&file, &name, None)
                .ok_or(LinkerError::MissingLibraryArtifact { file, name })?;

            if factory_deps.insert(id) {
                self.zk_collect_factory_deps(id, factory_deps)?;
            }
        }

        Ok(())
    }

    /// Performs DFS on the graph of link references, and populates `deps` with all found libraries,
    /// including ones of factory deps.
    pub fn zk_collect_dependencies(
        &'a self,
        target: &'a ArtifactId,
        libraries: &mut BTreeSet<&'a ArtifactId>,
        factory_deps: Option<&mut BTreeSet<&'a ArtifactId>>,
    ) -> Result<(), LinkerError> {
        let (_, artifact) = self
            .zk_artifacts()
            .find(|(id, _)| id.source == target.source && id.name == target.name)
            .ok_or(LinkerError::MissingTargetArtifact)?;

        let mut references = BTreeMap::new();
        if let Some(bytecode) = &artifact.bytecode {
            references.extend(bytecode.link_references());
        }

        // find all nested factory deps's link references
        let mut fdeps_default = BTreeSet::new();
        let factory_deps = factory_deps.unwrap_or(&mut fdeps_default);
        self.zk_collect_factory_deps(target, factory_deps)?;

        for (_, fdep) in factory_deps.iter().filter_map(|target| {
            self.zk_artifacts().find(|(id, _)| id.source == target.source && id.name == target.name)
        }) {
            if let Some(bytecode) = &fdep.bytecode {
                references.extend(bytecode.link_references());
            }
        }

        for (file, libs) in &references {
            for contract in libs.keys() {
                let id = self
                    .linker
                    .find_artifact_id_by_library_path(file, contract, None)
                    .ok_or_else(|| LinkerError::MissingLibraryArtifact {
                        file: file.to_string(),
                        name: contract.to_string(),
                    })?;
                if libraries.insert(id) {
                    self.zk_collect_dependencies(id, libraries, Some(factory_deps))?;
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
    pub fn zk_link_with_nonce_or_address(
        &'a self,
        libraries: Libraries,
        sender: Address,
        mut nonce: u64,
        targets: impl IntoIterator<Item = &'a ArtifactId>,
    ) -> Result<LinkOutput, ZkLinkerError> {
        // Library paths in `link_references` keys are always stripped, so we have to strip
        // user-provided paths to be able to match them correctly.
        let mut libraries = libraries.with_stripped_file_prefixes(self.linker.root.as_path());

        let mut needed_libraries = BTreeSet::new();
        for target in targets {
            self.zk_collect_dependencies(target, &mut needed_libraries, None)?;
        }

        let mut libs_to_deploy = Vec::new();

        // If `libraries` does not contain needed dependency, compute its address and add to
        // `libs_to_deploy`.
        for id in needed_libraries {
            let (lib_path, lib_name) = self.linker.convert_artifact_id_to_lib_path(id);

            libraries.libs.entry(lib_path).or_default().entry(lib_name).or_insert_with(|| {
                let address = foundry_zksync_core::compute_create_address(sender, nonce as u32);
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

    pub fn zk_link_with_create2(
        &'a self,
        libraries: Libraries,
        sender: Address,
        salt: B256,
        target: &'a ArtifactId,
    ) -> Result<LinkOutput, LinkerError> {
        // Library paths in `link_references` keys are always stripped, so we have to strip
        // user-provided paths to be able to match them correctly.
        let mut libraries = libraries.with_stripped_file_prefixes(self.linker.root.as_path());

        let mut contracts = self.linker.contracts.clone();

        let mut needed_libraries = BTreeSet::new();
        self.zk_collect_dependencies(target, &mut needed_libraries, None)?;

        let attempt_link = |contracts: &mut ArtifactContracts<CompactContractBytecodeCow<'a>>,
                            id,
                            libraries: &Libraries,
                            zksolc| {
            let original = contracts.get(id).expect("library present in list of contracts");
            // Link library with provided libs and extract bytecode object (possibly unlinked).
            match Self::zk_link(contracts, id, libraries, zksolc) {
                Ok(linked) => {
                    // persist linked contract for successive iterations
                    *contracts.entry(id.clone()).or_default() = linked.clone();
                    linked.bytecode.expect("library should have bytecode")
                }
                // the library remains unlinked at this time
                Err(_) => original.bytecode.as_ref().expect("library should have bytecode").clone(),
            }
        };

        let mut needed_libraries = needed_libraries
            .into_iter()
            .filter(|id| {
                // Filter out already provided libraries.
                let (file, name) = self.linker.convert_artifact_id_to_lib_path(id);
                !libraries.libs.contains_key(&file) || !libraries.libs[&file].contains_key(&name)
            })
            .map(|id| (id, attempt_link(&mut contracts, id, &libraries, &self.compiler)))
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
            let (_, library_bytecode) = needed_libraries.swap_remove(index);

            let code = library_bytecode.bytes().expect("fully linked bytecode");
            let bytecode_hash = hash_bytecode(code);

            let address = foundry_zksync_core::compute_create2_address(
                sender,
                bytecode_hash,
                H256::from_slice(&salt.0),
                &[],
            );

            let (file, name) = self.linker.convert_artifact_id_to_lib_path(id);

            // NOTE(zk): doesn't really matter since we use the EVM
            // bytecode to determine what EraVM bytecode to deploy
            libs_to_deploy.push(code.clone());
            libraries.libs.entry(file).or_default().insert(name, address.to_checksum(None));

            for (id, bytecode) in &mut needed_libraries {
                *bytecode = attempt_link(&mut contracts, id, &libraries, &self.compiler)
            }
        }

        Ok(LinkOutput { libraries, libs_to_deploy })
    }

    /// Links given artifact with given libraries.
    // TODO(zk): improve interface to reflect batching operation (all bytecodes in all bytecodes
    // out)
    pub fn zk_link(
        contracts: &ArtifactContracts<CompactContractBytecodeCow<'a>>,
        target: &ArtifactId,
        libraries: &Libraries,
        zksolc_path: &Path,
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

        let mut link_output =
            zk_link::zksolc_link(zksolc_path, zk_link::LinkJsonInput { bytecodes, libraries })
                .expect("able to call zksolc --link"); // TODO(zk): proper error check

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

    pub fn zk_get_linked_artifacts<'b>(
        &self,
        targets: impl IntoIterator<Item = &'b ArtifactId>,
        libraries: &Libraries,
    ) -> Result<ArtifactContracts, ZkLinkerError> {
        let mut contracts = self
            .linker
            .contracts
            .clone()
            .into_iter()
            // we strip these here because the file references are also relative
            // and the linker wouldn't be able to properly detect matching factory deps
            // (libraries are given separately and already stripped)
            .map(|(id, v)| (id.with_stripped_file_prefixes(&self.linker.root), v))
            .collect();
        let mut targets = targets.into_iter().cloned().collect::<VecDeque<_>>();
        let mut linked_artifacts = vec![];

        // explanation below
        while let Some(id) = targets.pop_front() {
            if linked_artifacts.iter().any(|(linked, _)| linked == &id) {
                // skip already linked
                continue;
            }

            match Self::zk_link(
                &contracts,
                // we strip here _only_ so that the target matches what's in `contracts`
                // but we want to return the full id in the `linked_artifacts`
                &id.clone().with_stripped_file_prefixes(&self.linker.root),
                libraries,
                &self.compiler,
            ) {
                Ok(linked) => {
                    *contracts.entry(id.clone()).or_default() = linked.clone();

                    // persist linked contract for successive iterations
                    linked_artifacts.push((id, CompactContractBytecode::from(linked)));
                }
                // contract was ignored, no need to add it to the list of linked contracts
                Err(ZkLinkerError::MissingFactoryDeps(fdeps)) => {
                    // attempt linking again if some factory dep remains unlinked
                    // this is just in the case where a previously unlinked factory dep
                    // is linked with the same run as `id` would be linked
                    // and instead `id` remains unlinked
                    // TODO(zk): might be unnecessary, observed when paths were wrong
                    let mut ids = fdeps
                        .iter()
                        .flat_map(|fdep| {
                            contracts.iter().find(|(id, _)| {
                                // strip here to match against the fdep which is stripped
                                let id =
                                    (*id).clone().with_stripped_file_prefixes(&self.linker.root);
                                id.source.as_path() == Path::new(fdep.filename.as_str()) &&
                                    id.name == fdep.library
                            })
                        })
                        // we want to keep the non-stripped
                        .map(|(id, _)| id.clone())
                        .peekable();

                    // if we have no dep ids then we avoid
                    // queueing our own id to avoid infinite loop
                    // TODO(zk): find a better way to avoid issues later
                    if let Some(sample_dep) = ids.peek() {
                        // ensure that the sample_dep is in `contracts`
                        if contracts.get(sample_dep).is_none() {
                            return Err(ZkLinkerError::MissingFactoryDeps(fdeps));
                        }

                        targets.extend(ids); // queue factory deps for linking
                        targets.push_back(id); // requeue original target
                    }
                }
                Err(err) => return Err(err),
            }
        }

        Ok(linked_artifacts.into_iter().collect())
    }
}
