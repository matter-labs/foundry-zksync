//! Implementations of [`Filesystem`](spec::Group::Filesystem) cheatcodes.

use super::string::parse;
use crate::{Cheatcode, Cheatcodes, CheatcodesExecutor, CheatsCtxt, Result, Vm::*};
use alloy_dyn_abi::DynSolType;
use alloy_json_abi::ContractObject;
use alloy_network::AnyTransactionReceipt;
use alloy_primitives::{Bytes, U256, hex, map::Entry};
use alloy_provider::network::ReceiptResponse;
use alloy_sol_types::SolValue;
use dialoguer::{Input, Password};
use forge_script_sequence::{BroadcastReader, TransactionWithMetadata};
use foundry_common::fs;
use foundry_config::fs_permissions::FsAccessKind;
use revm::{context::CreateScheme, interpreter::CreateInputs};
use revm_inspectors::tracing::types::CallKind;
use semver::Version;
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};
use walkdir::WalkDir;

impl Cheatcode for existsCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path } = self;
        let path = state.config.ensure_path_allowed(path, FsAccessKind::Read)?;
        Ok(path.exists().abi_encode())
    }
}

impl Cheatcode for fsMetadataCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path } = self;
        let path = state.config.ensure_path_allowed(path, FsAccessKind::Read)?;

        let metadata = path.metadata()?;

        // These fields not available on all platforms; default to 0
        let [modified, accessed, created] =
            [metadata.modified(), metadata.accessed(), metadata.created()].map(|time| {
                time.unwrap_or(UNIX_EPOCH).duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
            });

        Ok(FsMetadata {
            isDir: metadata.is_dir(),
            isSymlink: metadata.is_symlink(),
            length: U256::from(metadata.len()),
            readOnly: metadata.permissions().readonly(),
            modified: U256::from(modified),
            accessed: U256::from(accessed),
            created: U256::from(created),
        }
        .abi_encode())
    }
}

impl Cheatcode for isDirCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path } = self;
        let path = state.config.ensure_path_allowed(path, FsAccessKind::Read)?;
        Ok(path.is_dir().abi_encode())
    }
}

impl Cheatcode for isFileCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path } = self;
        let path = state.config.ensure_path_allowed(path, FsAccessKind::Read)?;
        Ok(path.is_file().abi_encode())
    }
}

impl Cheatcode for projectRootCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self {} = self;
        Ok(state.config.root.display().to_string().abi_encode())
    }
}

impl Cheatcode for unixTimeCall {
    fn apply(&self, _state: &mut Cheatcodes) -> Result {
        let Self {} = self;
        let difference = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| fmt_err!("failed getting Unix timestamp: {e}"))?;
        Ok(difference.as_millis().abi_encode())
    }
}

impl Cheatcode for closeFileCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path } = self;
        let path = state.config.ensure_path_allowed(path, FsAccessKind::Read)?;

        state.test_context.opened_read_files.remove(&path);

        Ok(Default::default())
    }
}

impl Cheatcode for copyFileCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { from, to } = self;
        let from = state.config.ensure_path_allowed(from, FsAccessKind::Read)?;
        let to = state.config.ensure_path_allowed(to, FsAccessKind::Write)?;
        state.config.ensure_not_foundry_toml(&to)?;

        let n = fs::copy(from, to)?;
        Ok(n.abi_encode())
    }
}

impl Cheatcode for createDirCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path, recursive } = self;
        let path = state.config.ensure_path_allowed(path, FsAccessKind::Write)?;
        if *recursive { fs::create_dir_all(path) } else { fs::create_dir(path) }?;
        Ok(Default::default())
    }
}

impl Cheatcode for readDir_0Call {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path } = self;
        read_dir(state, path.as_ref(), 1, false)
    }
}

impl Cheatcode for readDir_1Call {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path, maxDepth } = self;
        read_dir(state, path.as_ref(), *maxDepth, false)
    }
}

impl Cheatcode for readDir_2Call {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path, maxDepth, followLinks } = self;
        read_dir(state, path.as_ref(), *maxDepth, *followLinks)
    }
}

impl Cheatcode for readFileCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path } = self;
        let path = state.config.ensure_path_allowed(path, FsAccessKind::Read)?;
        Ok(fs::read_to_string(path)?.abi_encode())
    }
}

impl Cheatcode for readFileBinaryCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path } = self;
        let path = state.config.ensure_path_allowed(path, FsAccessKind::Read)?;
        Ok(fs::read(path)?.abi_encode())
    }
}

impl Cheatcode for readLineCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path } = self;
        let path = state.config.ensure_path_allowed(path, FsAccessKind::Read)?;

        // Get reader for previously opened file to continue reading OR initialize new reader
        let reader = match state.test_context.opened_read_files.entry(path.clone()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(BufReader::new(fs::open(path)?)),
        };

        let mut line: String = String::new();
        reader.read_line(&mut line)?;

        // Remove trailing newline character, preserving others for cases where it may be important
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }

        Ok(line.abi_encode())
    }
}

impl Cheatcode for readLinkCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { linkPath: path } = self;
        let path = state.config.ensure_path_allowed(path, FsAccessKind::Read)?;
        let target = fs::read_link(path)?;
        Ok(target.display().to_string().abi_encode())
    }
}

impl Cheatcode for removeDirCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path, recursive } = self;
        let path = state.config.ensure_path_allowed(path, FsAccessKind::Write)?;
        if *recursive { fs::remove_dir_all(path) } else { fs::remove_dir(path) }?;
        Ok(Default::default())
    }
}

impl Cheatcode for removeFileCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path } = self;
        let path = state.config.ensure_path_allowed(path, FsAccessKind::Write)?;
        state.config.ensure_not_foundry_toml(&path)?;

        // also remove from the set if opened previously
        state.test_context.opened_read_files.remove(&path);

        if state.fs_commit {
            fs::remove_file(&path)?;
        }

        Ok(Default::default())
    }
}

impl Cheatcode for writeFileCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path, data } = self;
        write_file(state, path.as_ref(), data.as_bytes())
    }
}

impl Cheatcode for writeFileBinaryCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path, data } = self;
        write_file(state, path.as_ref(), data)
    }
}

impl Cheatcode for writeLineCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { path, data: line } = self;
        let path = state.config.ensure_path_allowed(path, FsAccessKind::Write)?;
        state.config.ensure_not_foundry_toml(&path)?;

        if state.fs_commit {
            let mut file = std::fs::OpenOptions::new().append(true).create(true).open(path)?;

            writeln!(file, "{line}")?;
        }

        Ok(Default::default())
    }
}

impl Cheatcode for getArtifactPathByCodeCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { code } = self;
        let (artifact_id, _) = state
            .config
            .available_artifacts
            .as_ref()
            .and_then(|artifacts| artifacts.find_by_creation_code(code))
            .ok_or_else(|| fmt_err!("no matching artifact found"))?;

        Ok(artifact_id.path.to_string_lossy().abi_encode())
    }
}

impl Cheatcode for getArtifactPathByDeployedCodeCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { deployedCode } = self;
        let (artifact_id, _) = state
            .config
            .available_artifacts
            .as_ref()
            .and_then(|artifacts| artifacts.find_by_deployed_code(deployedCode))
            .ok_or_else(|| fmt_err!("no matching artifact found"))?;

        Ok(artifact_id.path.to_string_lossy().abi_encode())
    }
}

impl Cheatcode for getCodeCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { artifactPath: path } = self;
        Ok(get_artifact_code(state, path, false)?.abi_encode())
    }
}

impl Cheatcode for getDeployedCodeCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { artifactPath: path } = self;
        Ok(get_artifact_code(state, path, true)?.abi_encode())
    }
}

impl Cheatcode for deployCode_0Call {
    fn apply_full(&self, ccx: &mut CheatsCtxt, executor: &mut dyn CheatcodesExecutor) -> Result {
        let Self { artifactPath: path } = self;
        deploy_code(ccx, executor, path, None, None, None)
    }
}

impl Cheatcode for deployCode_1Call {
    fn apply_full(&self, ccx: &mut CheatsCtxt, executor: &mut dyn CheatcodesExecutor) -> Result {
        let Self { artifactPath: path, constructorArgs: args } = self;
        deploy_code(ccx, executor, path, Some(args), None, None)
    }
}

impl Cheatcode for deployCode_2Call {
    fn apply_full(&self, ccx: &mut CheatsCtxt, executor: &mut dyn CheatcodesExecutor) -> Result {
        let Self { artifactPath: path, value } = self;
        deploy_code(ccx, executor, path, None, Some(*value), None)
    }
}

impl Cheatcode for deployCode_3Call {
    fn apply_full(&self, ccx: &mut CheatsCtxt, executor: &mut dyn CheatcodesExecutor) -> Result {
        let Self { artifactPath: path, constructorArgs: args, value } = self;
        deploy_code(ccx, executor, path, Some(args), Some(*value), None)
    }
}

impl Cheatcode for deployCode_4Call {
    fn apply_full(&self, ccx: &mut CheatsCtxt, executor: &mut dyn CheatcodesExecutor) -> Result {
        let Self { artifactPath: path, salt } = self;
        deploy_code(ccx, executor, path, None, None, Some((*salt).into()))
    }
}

impl Cheatcode for deployCode_5Call {
    fn apply_full(&self, ccx: &mut CheatsCtxt, executor: &mut dyn CheatcodesExecutor) -> Result {
        let Self { artifactPath: path, constructorArgs: args, salt } = self;
        deploy_code(ccx, executor, path, Some(args), None, Some((*salt).into()))
    }
}

impl Cheatcode for deployCode_6Call {
    fn apply_full(&self, ccx: &mut CheatsCtxt, executor: &mut dyn CheatcodesExecutor) -> Result {
        let Self { artifactPath: path, value, salt } = self;
        deploy_code(ccx, executor, path, None, Some(*value), Some((*salt).into()))
    }
}

impl Cheatcode for deployCode_7Call {
    fn apply_full(&self, ccx: &mut CheatsCtxt, executor: &mut dyn CheatcodesExecutor) -> Result {
        let Self { artifactPath: path, constructorArgs: args, value, salt } = self;
        deploy_code(ccx, executor, path, Some(args), Some(*value), Some((*salt).into()))
    }
}

/// Helper function to deploy contract from artifact code.
/// Uses CREATE2 scheme if salt specified.
fn deploy_code(
    ccx: &mut CheatsCtxt,
    executor: &mut dyn CheatcodesExecutor,
    path: &str,
    constructor_args: Option<&Bytes>,
    value: Option<U256>,
    salt: Option<U256>,
) -> Result {
    let mut bytecode = get_artifact_code(ccx.state, path, false)?.to_vec();
    if let Some(args) = constructor_args {
        bytecode.extend_from_slice(args);
    }

    let scheme =
        if let Some(salt) = salt { CreateScheme::Create2 { salt } } else { CreateScheme::Create };

    let outcome = executor.exec_create(
        CreateInputs {
            caller: ccx.caller,
            scheme,
            value: value.unwrap_or(U256::ZERO),
            init_code: bytecode.into(),
            gas_limit: ccx.gas_limit,
        },
        ccx,
    )?;

    if !outcome.result.result.is_ok() {
        return Err(crate::Error::from(outcome.result.output));
    }

    let address = outcome.address.ok_or_else(|| fmt_err!("contract creation failed"))?;

    Ok(address.abi_encode())
}

/// Returns the path to the json artifact depending on the input
///
/// Can parse following input formats:
/// - `path/to/artifact.json`
/// - `path/to/contract.sol`
/// - `path/to/contract.sol:ContractName`
/// - `path/to/contract.sol:ContractName:0.8.23`
/// - `path/to/contract.sol:0.8.23`
/// - `ContractName`
/// - `ContractName:0.8.23`
pub fn get_artifact_code(state: &Cheatcodes, path: &str, deployed: bool) -> Result<Bytes> {
    let path = if path.ends_with(".json") {
        PathBuf::from(path)
    } else {
        let mut parts = path.split(':');

        let mut file = None;
        let mut contract_name = None;
        let mut version = None;

        let path_or_name = parts.next().unwrap();
        if path_or_name.contains('.') {
            file = Some(PathBuf::from(path_or_name));
            if let Some(name_or_version) = parts.next() {
                if name_or_version.contains('.') {
                    version = Some(name_or_version);
                } else {
                    contract_name = Some(name_or_version);
                    version = parts.next();
                }
            }
        } else {
            contract_name = Some(path_or_name);
            version = parts.next();
        }

        let version = if let Some(version) = version {
            Some(Version::parse(version).map_err(|e| fmt_err!("failed parsing version: {e}"))?)
        } else {
            None
        };

        // Use available artifacts list if present
        if let Some(artifacts) = &state.config.available_artifacts {
            let filtered = artifacts
                .iter()
                .filter(|(id, _)| {
                    // name might be in the form of "Counter.0.8.23"
                    let id_name = id.name.split('.').next().unwrap();

                    if let Some(path) = &file
                        && !id.source.ends_with(path)
                    {
                        return false;
                    }
                    if let Some(name) = contract_name
                        && id_name != name
                    {
                        return false;
                    }
                    if let Some(ref version) = version
                        && (id.version.minor != version.minor
                            || id.version.major != version.major
                            || id.version.patch != version.patch)
                    {
                        return false;
                    }
                    true
                })
                .collect::<Vec<_>>();

            let artifact = match &filtered[..] {
                [] => Err(fmt_err!("no matching artifact found")),
                [artifact] => Ok(*artifact),
                filtered => {
                    let mut filtered = filtered.to_vec();
                    // If we know the current script/test contract solc version, try to filter by it
                    state
                        .config
                        .running_artifact
                        .as_ref()
                        .and_then(|running| {
                            // Firstly filter by version
                            filtered.retain(|(id, _)| id.version == running.version);

                            // Return artifact if only one matched
                            if filtered.len() == 1 {
                                return Some(filtered[0]);
                            }

                            // Try filtering by profile as well
                            filtered.retain(|(id, _)| id.profile == running.profile);

                            if filtered.len() == 1 { Some(filtered[0]) } else { None }
                        })
                        .ok_or_else(|| fmt_err!("multiple matching artifacts found"))
                }
            }?;

            let maybe_bytecode = if deployed {
                artifact.1.deployed_bytecode().cloned()
            } else {
                artifact.1.bytecode().cloned()
            };

            return maybe_bytecode
                .ok_or_else(|| fmt_err!("no bytecode for contract; is it abstract or unlinked?"));
        } else {
            let path_in_artifacts =
                match (file.map(|f| f.to_string_lossy().to_string()), contract_name) {
                    (Some(file), Some(contract_name)) => {
                        PathBuf::from(format!("{file}/{contract_name}.json"))
                    }
                    (None, Some(contract_name)) => {
                        PathBuf::from(format!("{contract_name}.sol/{contract_name}.json"))
                    }
                    (Some(file), None) => {
                        let name = file.replace(".sol", "");
                        PathBuf::from(format!("{file}/{name}.json"))
                    }
                    _ => bail!("invalid artifact path"),
                };

            state.config.paths.artifacts.join(path_in_artifacts)
        }
    };

    let path = state.config.ensure_path_allowed(path, FsAccessKind::Read)?;
    let data = fs::read_to_string(path)?;
    let artifact = serde_json::from_str::<ContractObject>(&data)?;
    let maybe_bytecode = if deployed { artifact.deployed_bytecode } else { artifact.bytecode };
    maybe_bytecode.ok_or_else(|| fmt_err!("no bytecode for contract; is it abstract or unlinked?"))
}

impl Cheatcode for ffiCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { commandInput: input } = self;

        let output = ffi(state, input)?;
        // TODO: check exit code?
        if !output.stderr.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(target: "cheatcodes", ?input, ?stderr, "non-empty stderr");
        }
        // we already hex-decoded the stdout in `ffi`
        Ok(output.stdout.abi_encode())
    }
}

impl Cheatcode for tryFfiCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { commandInput: input } = self;
        ffi(state, input).map(|res| res.abi_encode())
    }
}

impl Cheatcode for promptCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { promptText: text } = self;
        prompt(state, text, prompt_input).map(|res| res.abi_encode())
    }
}

impl Cheatcode for promptSecretCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { promptText: text } = self;
        prompt(state, text, prompt_password).map(|res| res.abi_encode())
    }
}

impl Cheatcode for promptSecretUintCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { promptText: text } = self;
        parse(&prompt(state, text, prompt_password)?, &DynSolType::Uint(256))
    }
}

impl Cheatcode for promptAddressCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { promptText: text } = self;
        parse(&prompt(state, text, prompt_input)?, &DynSolType::Address)
    }
}

impl Cheatcode for promptUintCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { promptText: text } = self;
        parse(&prompt(state, text, prompt_input)?, &DynSolType::Uint(256))
    }
}

pub(super) fn write_file(state: &Cheatcodes, path: &Path, contents: &[u8]) -> Result {
    let path = state.config.ensure_path_allowed(path, FsAccessKind::Write)?;
    // write access to foundry.toml is not allowed
    state.config.ensure_not_foundry_toml(&path)?;

    if state.fs_commit {
        fs::write(path, contents)?;
    }

    Ok(Default::default())
}

fn read_dir(state: &Cheatcodes, path: &Path, max_depth: u64, follow_links: bool) -> Result {
    let root = state.config.ensure_path_allowed(path, FsAccessKind::Read)?;
    let paths: Vec<DirEntry> = WalkDir::new(root)
        .min_depth(1)
        .max_depth(max_depth.try_into().unwrap_or(usize::MAX))
        .follow_links(follow_links)
        .contents_first(false)
        .same_file_system(true)
        .sort_by_file_name()
        .into_iter()
        .map(|entry| match entry {
            Ok(entry) => DirEntry {
                errorMessage: String::new(),
                path: entry.path().display().to_string(),
                depth: entry.depth() as u64,
                isDir: entry.file_type().is_dir(),
                isSymlink: entry.path_is_symlink(),
            },
            Err(e) => DirEntry {
                errorMessage: e.to_string(),
                path: e.path().map(|p| p.display().to_string()).unwrap_or_default(),
                depth: e.depth() as u64,
                isDir: false,
                isSymlink: false,
            },
        })
        .collect();
    Ok(paths.abi_encode())
}

fn ffi(state: &Cheatcodes, input: &[String]) -> Result<FfiResult> {
    ensure!(
        state.config.ffi,
        "FFI is disabled; add the `--ffi` flag to allow tests to call external commands"
    );
    ensure!(!input.is_empty() && !input[0].is_empty(), "can't execute empty command");
    let mut cmd = Command::new(&input[0]);
    cmd.args(&input[1..]);

    debug!(target: "cheatcodes", ?cmd, "invoking ffi");

    let output = cmd
        .current_dir(&state.config.root)
        .output()
        .map_err(|err| fmt_err!("failed to execute command {cmd:?}: {err}"))?;

    // The stdout might be encoded on valid hex, or it might just be a string,
    // so we need to determine which it is to avoid improperly encoding later.
    let trimmed_stdout = String::from_utf8(output.stdout)?;
    let trimmed_stdout = trimmed_stdout.trim();
    let encoded_stdout = if let Ok(hex) = hex::decode(trimmed_stdout) {
        hex
    } else {
        trimmed_stdout.as_bytes().to_vec()
    };
    Ok(FfiResult {
        exitCode: output.status.code().unwrap_or(69),
        stdout: encoded_stdout.into(),
        stderr: output.stderr.into(),
    })
}

fn prompt_input(prompt_text: &str) -> Result<String, dialoguer::Error> {
    Input::new().allow_empty(true).with_prompt(prompt_text).interact_text()
}

fn prompt_password(prompt_text: &str) -> Result<String, dialoguer::Error> {
    Password::new().with_prompt(prompt_text).interact()
}

fn prompt(
    state: &Cheatcodes,
    prompt_text: &str,
    input: fn(&str) -> Result<String, dialoguer::Error>,
) -> Result<String> {
    let text_clone = prompt_text.to_string();
    let timeout = state.config.prompt_timeout;
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let _ = tx.send(input(&text_clone));
    });

    match rx.recv_timeout(timeout) {
        Ok(res) => res.map_err(|err| {
            let _ = sh_println!();
            err.to_string().into()
        }),
        Err(_) => {
            let _ = sh_eprintln!();
            Err("Prompt timed out".into())
        }
    }
}

impl Cheatcode for getBroadcastCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { contractName, chainId, txType } = self;

        let latest_broadcast = latest_broadcast(
            contractName,
            *chainId,
            &state.config.broadcast,
            vec![map_broadcast_tx_type(*txType)],
        )?;

        Ok(latest_broadcast.abi_encode())
    }
}

impl Cheatcode for getBroadcasts_0Call {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { contractName, chainId, txType } = self;

        let reader = BroadcastReader::new(contractName.clone(), *chainId, &state.config.broadcast)?
            .with_tx_type(map_broadcast_tx_type(*txType));

        let broadcasts = reader.read()?;

        let summaries = broadcasts
            .into_iter()
            .flat_map(|broadcast| {
                let results = reader.into_tx_receipts(broadcast);
                parse_broadcast_results(results)
            })
            .collect::<Vec<_>>();

        Ok(summaries.abi_encode())
    }
}

impl Cheatcode for getBroadcasts_1Call {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { contractName, chainId } = self;

        let reader = BroadcastReader::new(contractName.clone(), *chainId, &state.config.broadcast)?;

        let broadcasts = reader.read()?;

        let summaries = broadcasts
            .into_iter()
            .flat_map(|broadcast| {
                let results = reader.into_tx_receipts(broadcast);
                parse_broadcast_results(results)
            })
            .collect::<Vec<_>>();

        Ok(summaries.abi_encode())
    }
}

impl Cheatcode for getDeployment_0Call {
    fn apply_stateful(&self, ccx: &mut CheatsCtxt) -> Result {
        let Self { contractName } = self;
        let chain_id = ccx.ecx.cfg.chain_id;

        let latest_broadcast = latest_broadcast(
            contractName,
            chain_id,
            &ccx.state.config.broadcast,
            vec![CallKind::Create, CallKind::Create2],
        )?;

        Ok(latest_broadcast.contractAddress.abi_encode())
    }
}

impl Cheatcode for getDeployment_1Call {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { contractName, chainId } = self;

        let latest_broadcast = latest_broadcast(
            contractName,
            *chainId,
            &state.config.broadcast,
            vec![CallKind::Create, CallKind::Create2],
        )?;

        Ok(latest_broadcast.contractAddress.abi_encode())
    }
}

impl Cheatcode for getDeploymentsCall {
    fn apply(&self, state: &mut Cheatcodes) -> Result {
        let Self { contractName, chainId } = self;

        let reader = BroadcastReader::new(contractName.clone(), *chainId, &state.config.broadcast)?
            .with_tx_type(CallKind::Create)
            .with_tx_type(CallKind::Create2);

        let broadcasts = reader.read()?;

        let summaries = broadcasts
            .into_iter()
            .flat_map(|broadcast| {
                let results = reader.into_tx_receipts(broadcast);
                parse_broadcast_results(results)
            })
            .collect::<Vec<_>>();

        let deployed_addresses =
            summaries.into_iter().map(|summary| summary.contractAddress).collect::<Vec<_>>();

        Ok(deployed_addresses.abi_encode())
    }
}

fn map_broadcast_tx_type(tx_type: BroadcastTxType) -> CallKind {
    match tx_type {
        BroadcastTxType::Call => CallKind::Call,
        BroadcastTxType::Create => CallKind::Create,
        BroadcastTxType::Create2 => CallKind::Create2,
        _ => unreachable!("invalid tx type"),
    }
}

fn parse_broadcast_results(
    results: Vec<(TransactionWithMetadata, AnyTransactionReceipt)>,
) -> Vec<BroadcastTxSummary> {
    results
        .into_iter()
        .map(|(tx, receipt)| BroadcastTxSummary {
            txHash: receipt.transaction_hash,
            blockNumber: receipt.block_number.unwrap_or_default(),
            txType: match tx.opcode {
                CallKind::Call => BroadcastTxType::Call,
                CallKind::Create => BroadcastTxType::Create,
                CallKind::Create2 => BroadcastTxType::Create2,
                _ => unreachable!("invalid tx type"),
            },
            contractAddress: tx.contract_address.unwrap_or_default(),
            success: receipt.status(),
        })
        .collect()
}

fn latest_broadcast(
    contract_name: &String,
    chain_id: u64,
    broadcast_path: &Path,
    filters: Vec<CallKind>,
) -> Result<BroadcastTxSummary> {
    let mut reader = BroadcastReader::new(contract_name.clone(), chain_id, broadcast_path)?;

    for filter in filters {
        reader = reader.with_tx_type(filter);
    }

    let broadcast = reader.read_latest()?;

    let results = reader.into_tx_receipts(broadcast);

    let summaries = parse_broadcast_results(results);

    summaries
        .first()
        .ok_or_else(|| fmt_err!("no deployment found for {contract_name} on chain {chain_id}"))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CheatsConfig;
    use std::sync::Arc;

    fn cheats() -> Cheatcodes {
        let config = CheatsConfig {
            ffi: true,
            root: PathBuf::from(&env!("CARGO_MANIFEST_DIR")),
            ..Default::default()
        };
        Cheatcodes::new(Arc::new(config))
    }

    #[test]
    fn test_ffi_hex() {
        let msg = b"gm";
        let cheats = cheats();
        let args = ["echo".to_string(), hex::encode(msg)];
        let output = ffi(&cheats, &args).unwrap();
        assert_eq!(output.stdout, Bytes::from(msg));
    }

    #[test]
    fn test_ffi_string() {
        let msg = "gm";
        let cheats = cheats();
        let args = ["echo".to_string(), msg.to_string()];
        let output = ffi(&cheats, &args).unwrap();
        assert_eq!(output.stdout, Bytes::from(msg.as_bytes()));
    }

    #[test]
    fn test_artifact_parsing() {
        let s = include_str!("../../evm/test-data/solc-obj.json");
        let artifact: ContractObject = serde_json::from_str(s).unwrap();
        assert!(artifact.bytecode.is_some());

        let artifact: ContractObject = serde_json::from_str(s).unwrap();
        assert!(artifact.deployed_bytecode.is_some());
    }
}
