use foundry_compilers_artifacts_solc::{
    BytecodeObject, CompactContractRef, FileToContractsMap, SourceFile, SourceFiles,
};

use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

pub mod contract;
pub mod error;
pub mod output_selection;

use self::{contract::Contract, error::Error};

/// file -> (contract name -> Contract)
pub type Contracts = FileToContractsMap<Contract>;

/// Output type `zksolc` produces
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct CompilerOutput {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<Error>,
    #[serde(default)]
    pub sources: BTreeMap<PathBuf, SourceFile>,
    #[serde(default)]
    pub contracts: FileToContractsMap<Contract>,
    /// The `solc` compiler version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The `solc` compiler long version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_version: Option<String>,
    /// The `zksolc` compiler version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zk_version: Option<String>,
    /// The ZKsync solc compiler version (if it was used). This field is
    /// inserted by this crate and not an actual part of the compiler output
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zksync_solc_version: Option<Version>,
}

impl CompilerOutput {
    /// Whether the output contains a compiler error
    pub fn has_error(&self) -> bool {
        self.errors.iter().any(|err| err.severity.is_error())
    }

    /// Returns the output's source files and contracts separately, wrapped in helper types that
    /// provide several helper methods
    pub fn split(self) -> (SourceFiles, OutputContracts) {
        (SourceFiles(self.sources), OutputContracts(self.contracts))
    }

    /// Retains only those files the given iterator yields
    ///
    /// In other words, removes all contracts for files not included in the iterator
    pub fn retain_files<'a, I>(&mut self, files: I)
    where
        I: IntoIterator<Item = &'a Path>,
    {
        // Note: use `to_lowercase` here because solc not necessarily emits the exact file name,
        // e.g. `src/utils/upgradeProxy.sol` is emitted as `src/utils/UpgradeProxy.sol`
        let files: HashSet<_> =
            files.into_iter().map(|s| s.to_string_lossy().to_lowercase()).collect();
        self.contracts.retain(|f, _| files.contains(&f.to_string_lossy().to_lowercase()));
        self.sources.retain(|f, _| files.contains(&f.to_string_lossy().to_lowercase()));
    }

    pub fn merge(&mut self, other: Self) {
        self.errors.extend(other.errors);
        self.contracts.extend(other.contracts);
        self.sources.extend(other.sources);
    }

    pub fn join_all(&mut self, root: impl AsRef<Path>) {
        let root = root.as_ref();
        self.contracts = std::mem::take(&mut self.contracts)
            .into_iter()
            .map(|(path, contracts)| (root.join(path), contracts))
            .collect();
        self.sources = std::mem::take(&mut self.sources)
            .into_iter()
            .map(|(path, source)| (root.join(path), source))
            .collect();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EraVM {
    /// The contract EraVM assembly code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<String>,
    /// The contract bytecode.
    /// Is reset by that of EraVM before yielding the compiled project artifacts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bytecode: Option<BytecodeObject>,
}

impl EraVM {
    pub fn bytecode(&self, should_be_unlinked: bool) -> Option<BytecodeObject> {
        self.bytecode.as_ref().map(|object| match (should_be_unlinked, object) {
            (true, BytecodeObject::Bytecode(bc)) => {
                // convert to unlinked
                let encoded = alloy_primitives::hex::encode(bc);
                BytecodeObject::Unlinked(encoded)
            }
            (false, BytecodeObject::Unlinked(bc)) => {
                // convert to linked
                let bytecode = alloy_primitives::hex::decode(bc).expect("valid bytecode");
                BytecodeObject::Bytecode(bytecode.into())
            }
            _ => object.to_owned(),
        })
    }

    // TODO: tmp to make compiler abstraction sample work, needs some thought on
    // how do transform linked/to unlinked
    pub fn bytecode_ref(&self) -> Option<&BytecodeObject> {
        self.bytecode.as_ref()
    }
}

///
/// The `solc --standard-json` output contract EVM extra metadata.
#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExtraMetadata {
    /// The list of recursive functions.
    #[serde(default = "Vec::new")]
    pub recursive_functions: Vec<RecursiveFunction>,
}

///
/// The `solc --standard-json` output contract EVM recursive function.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecursiveFunction {
    /// The function name.
    pub name: String,
    /// The creation code function block tag.
    pub creation_tag: Option<usize>,
    /// The runtime code function block tag.
    pub runtime_tag: Option<usize>,
    /// The number of input arguments.
    #[serde(rename = "totalParamSize")]
    pub input_size: usize,
    /// The number of output arguments.
    #[serde(rename = "totalRetParamSize")]
    pub output_size: usize,
}

/// A wrapper helper type for the `Contracts` type alias
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OutputContracts(pub Contracts);

impl OutputContracts {
    /// Returns an iterator over all contracts and their source names.
    pub fn into_contracts(self) -> impl Iterator<Item = (String, Contract)> {
        self.0.into_values().flatten()
    }

    /// Iterate over all contracts and their names
    pub fn contracts_iter(&self) -> impl Iterator<Item = (&String, &Contract)> {
        self.0.values().flatten()
    }

    /// Finds the _first_ contract with the given name
    pub fn find(&self, contract: impl AsRef<str>) -> Option<CompactContractRef<'_>> {
        let contract_name = contract.as_ref();
        self.contracts_iter().find_map(|(name, contract)| {
            (name == contract_name).then(|| CompactContractRef::from(contract))
        })
    }

    /// Finds the first contract with the given name and removes it from the set
    pub fn remove(&mut self, contract: impl AsRef<str>) -> Option<Contract> {
        let contract_name = contract.as_ref();
        self.0.values_mut().find_map(|c| c.remove(contract_name))
    }
}
