//! Contains items and functions to link via zksolc

use std::{
    path::Path,
    process::{Command, Stdio},
};

use alloy_primitives::{
    map::{HashMap, HashSet},
    Address, Bytes,
};
use foundry_compilers::error::SolcError;
use serde::{Deserialize, Serialize};

type LinkId = String;

/// A library that zksolc will link against
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
#[serde(into = "String")]
pub struct Library {
    /// Path to the library source
    pub filename: String,
    /// Name of the library
    pub name: String,
    /// Address of the library
    pub address: Address,
}

impl From<Library> for String {
    fn from(val: Library) -> Self {
        format!("{}:{}={}", val.filename, val.name, val.address)
    }
}

#[derive(Debug, Clone, Serialize)]
/// JSON Input for `zksolc link`
pub struct LinkJsonInput {
    /// List of input bytecodes (linked or unlinked)
    pub bytecodes: HashMap<LinkId, Bytes>,
    /// List of libraries to link against
    pub libraries: HashSet<Library>,
}

/// Representation of a linked object given by zksolc
#[derive(Debug, Clone, Deserialize)]
pub struct LinkedObject {
    // TODO(zk): obtain factoryDeps from output
    // might come in handy to have the libraries used as well
    /// Fully linked bytecode
    pub bytecode: String,
    /// Bytecode hash of the fully linked object
    pub hash: String,
}

/// Representation of a linked object given by zksolc
#[derive(Debug, Clone, Deserialize)]
pub struct UnlinkedObject {
    /// List of unlinked libraries
    pub linker_symbols: HashSet<MissingLibrary>,
    /// List of factory dependencies missing from input
    pub factory_dependencies: HashSet<MissingLibrary>,
}

/// Represent a missing library returned by the compiler
///
/// Deserialized from: "<path>:<name>"
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "String")]
pub struct MissingLibrary {
    /// Source path of the contract
    pub filename: String,
    /// Name of the contract
    pub library: String,
}

impl TryFrom<String> for MissingLibrary {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let mut split = value.split(':');
        let path = split.next().ok_or("failed to parse unlinked library filename")?.to_string();
        let name = split.next().ok_or("failed to parse unlinked library name")?.to_string();

        Ok(Self { filename: path, library: name })
    }
}

/// JSON Output for `zksolc link`
#[derive(Debug, Clone, Deserialize)]
pub struct LinkJsonOutput {
    /// Fully linked bytecodes resulting from given input
    #[serde(default)]
    pub linked: HashMap<LinkId, LinkedObject>,
    /// Not fully linked bytecodes
    #[serde(default)]
    pub unlinked: HashMap<LinkId, UnlinkedObject>,
    /// List of fully linked bytecodes in input
    #[serde(default)]
    pub ignored: HashMap<LinkId, LinkedObject>,
}

// taken from compilers
fn map_io_err(zksolc_path: &Path) -> impl FnOnce(std::io::Error) -> SolcError + '_ {
    move |err| SolcError::io(err, zksolc_path)
}

/// Invoke `zksolc link` given the `zksolc` binary and json input to use
#[tracing::instrument(level = tracing::Level::TRACE, ret)]
pub fn zksolc_link(zksolc_path: &Path, input: LinkJsonInput) -> Result<LinkJsonOutput, SolcError> {
    let mut cmd = Command::new(zksolc_path);

    cmd.arg("--standard-json")
        .arg("--link")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped());

    let mut child = cmd.spawn().map_err(map_io_err(zksolc_path))?;

    let stdin = child.stdin.as_mut().unwrap();
    let _ = serde_json::to_writer(stdin, &input);

    let output = child.wait_with_output().map_err(map_io_err(zksolc_path))?;
    tracing::trace!(?output);

    if output.status.success() {
        serde_json::from_slice(&output.stdout).map_err(Into::into)
    } else {
        Err(SolcError::solc_output(&output))
    }
}
