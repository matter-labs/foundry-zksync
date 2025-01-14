use std::{
    collections::{BTreeSet, VecDeque},
    path::{Path, PathBuf},
};

use alloy_primitives::{
    hex::FromHex,
    map::{HashMap, HashSet},
    Address,
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
    compilers::zksolc::ZkSolcCompiler,
    link::{self as zk_link, MissingLibrary},
};

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

#[derive(Debug)]
pub struct ZkLinker<'a> {
    pub linker: Linker<'a>,
    pub compiler: ZkSolcCompiler,
}

impl<'a> ZkLinker<'a> {
    pub fn new(
        root: impl Into<PathBuf>,
        contracts: ArtifactContracts<CompactContractBytecodeCow<'a>>,
        compiler: ZkSolcCompiler,
    ) -> Self {
        Self { linker: Linker::new(root, contracts), compiler }
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
            self.linker.collect_dependencies(target, &mut needed_libraries)?;
        }

        let mut libs_to_deploy = Vec::new();

        // If `libraries` does not contain needed dependency, compute its address and add to
        // `libs_to_deploy`.
        for id in needed_libraries {
            let (lib_path, lib_name) = self.linker.convert_artifact_id_to_lib_path(id);

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

    /// Links given artifact with given libraries.
    // TODO(zk): improve interface to reflect batching operation (all bytecodes in all bytecodes
    // out)
    pub fn zk_link(
        contracts: &ArtifactContracts<CompactContractBytecodeCow<'a>>,
        target: &ArtifactId,
        libraries: &Libraries,
        zksolc: &ZkSolcCompiler,
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
            zk_link::zksolc_link(zksolc, zk_link::LinkJsonInput { bytecodes, libraries })
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
        let mut targets = targets.into_iter().cloned().collect::<VecDeque<_>>();
        let mut contracts = self.linker.contracts.clone();
        let mut linked_artifacts = vec![];

        // TODO(zk): determine if this loop is still needed like this
        // explanation below
        while let Some(id) = targets.pop_front() {
            match Self::zk_link(&contracts, &id, libraries, &self.compiler) {
                Ok(linked) => {
                    // persist linked contract for successive iterations
                    *contracts.entry(id.clone()).or_default() = linked.clone();

                    linked_artifacts.push((id.clone(), CompactContractBytecode::from(linked)));
                }
                // contract was ignored, no need to add it to the list of linked contracts
                Err(ZkLinkerError::MissingFactoryDeps(fdeps)) => {
                    // attempt linking again if some factory dep remains unlinked
                    // this is just in the case where a previously unlinked factory dep
                    // is linked with the same run as `id` would be linked
                    // and instead `id` remains unlinked
                    // TODO(zk): might be unnecessary, observed when paths were wrong
                    let mut ids = fdeps
                        .into_iter()
                        .flat_map(|fdep| {
                            contracts.iter().find(|(id, _)| {
                                id.source.as_path() == Path::new(fdep.filename.as_str()) &&
                                    id.name == fdep.library
                            })
                        })
                        .map(|(id, _)| id.clone())
                        .peekable();

                    // if we have no dep ids then we avoid
                    // queueing our own id to avoid infinite loop
                    // TODO(zk): find a better way to avoid issues later
                    if ids.peek().is_some() {
                        targets.extend(ids); // queue factory deps for linking
                        targets.push_back(id); // reque original target
                    }
                }
                Err(err) => return Err(err),
            }
        }

        Ok(linked_artifacts.into_iter().collect())
    }
}
