use crate::{
    etherscan::EtherscanVerificationProvider,
    types::VerificationType,
    utils::{BytecodeType, JsonResult},
    VerifyBytecodeArgs,
};
use alloy_primitives::Address;
use alloy_provider::Provider;
use alloy_rpc_types::{BlockId, BlockNumberOrTag};
use eyre::{OptionExt, Result};
use foundry_block_explorers::{contract::Metadata, Response};
use foundry_cli::utils::{self, LoadConfig};
use foundry_common::{compile::ProjectCompiler, shell};
use foundry_config::Config;
use foundry_zksync_compilers::compilers::zksolc::settings::{BytecodeHash, ZkSettings};
use serde::{Deserialize, Deserializer, Serialize};
use yansi::Paint;

/// Run verify-bytecode for zksync. Since there's only runtime bytecode and that bytecode
/// cannot be modified at deploy time by the constructor, the check is much simpler than
/// in EVM as we do not need to simulate the deployment. We just compare the compiled bytecode
/// against the on-chain one.
pub async fn run(args: VerifyBytecodeArgs) -> Result<()> {
    let config = args.load_config()?;
    let provider = utils::get_provider(&config)?;

    let onchain_runtime_code = match args.block {
        Some(BlockId::Number(BlockNumberOrTag::Number(block))) => {
            provider.get_code_at(args.address).block_id(BlockId::number(block)).await?
        }
        Some(_) => eyre::bail!("Invalid block number"),
        None => provider.get_code_at(args.address).await?,
    };

    if onchain_runtime_code.is_empty() {
        eyre::bail!("No bytecode found at address {}", args.address);
    }

    if !shell::is_json() {
        sh_println!(
            "Verifying bytecode for contract {} at address {}",
            args.contract.name,
            args.address
        )?;
    }

    let mut json_results: Vec<JsonResult> = vec![];

    let onchain_settings = block_explorer_contract_source_code(
        &args.verifier.verifier_url.clone().unwrap(),
        args.address,
    )
    .await?;
    let onchain_bytecode_hash = onchain_settings
        .source_code
        .settings
        .metadata
        .map(|m| m.hash_type.unwrap_or_default())
        .unwrap_or_default();

    let project = foundry_config::zksync::config_create_project(&config, false, true)?;
    let compiler = ProjectCompiler::new();
    let mut output = compiler.zksync_compile(&project)?;

    let artifact = output
        .remove_contract(&args.contract)
        .ok_or_eyre("Build Error: Contract artifact not found locally")?;

    // Get local bytecode (creation code)
    let local_bytecode = artifact
        .bytecode
        .as_ref()
        .and_then(|b| b.to_owned().object().into_bytes())
        .ok_or_eyre("Unlinked bytecode is not supported for verification")?;

    // Compare the onchain runtime bytecode with the runtime code from the fork.
    let match_type = match_bytecodes(
        &local_bytecode,
        &onchain_runtime_code,
        config.zksync.hash_type().unwrap_or_default(),
        onchain_bytecode_hash,
    );

    print_result(&args, match_type, BytecodeType::Runtime, &mut json_results, &config).await;

    if shell::is_json() {
        sh_println!("{}", serde_json::to_string(&json_results)?)?;
    }

    Ok(())
}

fn match_bytecodes(
    local_bytecode: &[u8],
    onchain_bytecode: &[u8],
    local_bytecode_hash: BytecodeHash,
    onchain_bytecode_hash: BytecodeHash,
) -> Option<VerificationType> {
    // 1. Try full match
    if local_bytecode == onchain_bytecode {
        // If the bytecode_hash = 'none' in Config. Then it's always a partial match according to
        // sourcify definitions. Ref: https://docs.sourcify.dev/docs/full-vs-partial-match/.
        if local_bytecode_hash == BytecodeHash::None {
            return Some(VerificationType::Partial);
        }

        Some(VerificationType::Full)
    } else {
        extract_and_compare_bytecode(
            local_bytecode,
            onchain_bytecode,
            local_bytecode_hash,
            onchain_bytecode_hash,
        )
        .then_some(VerificationType::Partial)
    }
}

fn extract_and_compare_bytecode(
    local_bytecode: &[u8],
    onchain_bytecode: &[u8],
    local_hash_type: BytecodeHash,
    onchain_hash_type: BytecodeHash,
) -> bool {
    // Only compare valid bytecodes
    if local_bytecode.len() % 64 != 32 || onchain_bytecode.len() % 64 != 32 {
        return false;
    }

    let local_bytecode_without_hash = extract_metadata_hash(local_bytecode, local_hash_type);
    let onchain_bytecode_without_hash = extract_metadata_hash(onchain_bytecode, onchain_hash_type);

    // In order to be a match, bytecodes need to either be equal or just differ on an extra word
    // of 0s added to achieve odd 32 byte word count
    let min_len =
        std::cmp::min(local_bytecode_without_hash.len(), onchain_bytecode_without_hash.len());

    let (local_bytecode_without_remainder, local_remainder) =
        local_bytecode_without_hash.split_at(min_len);
    let (onchain_bytecode_without_remainder, onchain_remainder) =
        onchain_bytecode_without_hash.split_at(min_len);

    let no_remainders = local_remainder.is_empty() && onchain_remainder.is_empty();
    let local_has_padding = local_remainder.len() == 32 && local_remainder.iter().all(|b| *b == 0);
    let onchain_has_padding =
        onchain_remainder.len() == 32 && onchain_remainder.iter().all(|b| *b == 0);
    if !(no_remainders || local_has_padding || onchain_has_padding) {
        return false;
    }

    local_bytecode_without_remainder == onchain_bytecode_without_remainder
}

fn extract_metadata_hash(bytecode: &[u8], hash_type: BytecodeHash) -> &[u8] {
    let hash_len = match hash_type {
        BytecodeHash::None => 0,
        BytecodeHash::Keccak256 => 32,
        BytecodeHash::Ipfs => {
            // Get the last two bytes of the bytecode to find the length of CBOR metadata
            let metadata_len = &bytecode[bytecode.len() - 2..];
            let metadata_len: usize =
                (u16::from_be_bytes([metadata_len[0], metadata_len[1]]) + 2).into();
            if metadata_len <= bytecode.len() {
                // metadata will be 0 padded to be a multiple of 32
                metadata_len + (32 - metadata_len % 32)
            } else {
                0
            }
        }
    };

    &bytecode[..bytecode.len() - hash_len]
}

async fn print_result(
    args: &VerifyBytecodeArgs,
    res: Option<VerificationType>,
    bytecode_type: BytecodeType,
    json_results: &mut Vec<JsonResult>,
    config: &Config,
) {
    if let Some(res) = res {
        if !shell::is_json() {
            let _ = sh_println!(
                "{} with status {}",
                format!("{bytecode_type:?} code matched").green().bold(),
                res.green().bold()
            );
        } else {
            let json_res = JsonResult { bytecode_type, match_type: Some(res), message: None };
            json_results.push(json_res);
        }
    } else if !shell::is_json() {
        let _ = sh_err!(
            "{bytecode_type:?} code did not match - this may be due to varying compiler settings"
        );
        let mismatches = match try_get_settings_and_find_mismatch(args, config).await {
            Ok(m) => m,
            Err(e) => {
                let _ = sh_err!("Failed getting settings from block explorers to compare: {e}");
                vec![]
            }
        };
        for mismatch in mismatches {
            let _ = sh_eprintln!("{}", mismatch.red().bold());
        }
    } else {
        let json_res = JsonResult {
            bytecode_type,
            match_type: res,
            message: Some(format!(
                "{bytecode_type:?} code did not match - this may be due to varying compiler settings"
            )),
        };
        json_results.push(json_res);
    }
}

async fn try_get_settings_and_find_mismatch(
    args: &VerifyBytecodeArgs,
    config: &Config,
) -> Result<Vec<String>> {
    let etherscan = EtherscanVerificationProvider.client(
        args.etherscan.chain.unwrap_or_default(),
        args.verifier.verifier_url.as_deref(),
        None,
        config,
    )?;

    let source_code = etherscan.contract_source_code(args.address).await?;
    let etherscan_config = source_code.items.first().unwrap();
    Ok(find_mismatch_in_settings(etherscan_config, config))
}

fn find_mismatch_in_settings(
    etherscan_settings: &Metadata,
    local_settings: &Config,
) -> Vec<String> {
    let mut mismatches: Vec<String> = vec![];
    if etherscan_settings.evm_version != local_settings.evm_version.to_string().to_lowercase() {
        let str = format!(
            "EVM version mismatch: local={}, onchain={}",
            local_settings.evm_version, etherscan_settings.evm_version
        );
        mismatches.push(str);
    }
    let local_optimizer: u64 = if local_settings.optimizer == Some(true) { 1 } else { 0 };
    if etherscan_settings.optimization_used != local_optimizer {
        let str = format!(
            "Optimizer mismatch: local={}, onchain={}",
            local_settings.optimizer.unwrap_or(false),
            etherscan_settings.optimization_used
        );
        mismatches.push(str);
    }
    if local_settings.optimizer_runs.is_some_and(|runs| etherscan_settings.runs != runs as u64) ||
        (local_settings.optimizer_runs.is_none() && etherscan_settings.runs > 0)
    {
        let str = format!(
            "Optimizer runs mismatch: local={}, onchain={}",
            local_settings.optimizer_runs.unwrap(),
            etherscan_settings.runs
        );
        mismatches.push(str);
    }

    mismatches
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SourceCode {
    #[serde(deserialize_with = "deserialize_source_code")]
    pub source_code: SCMetadata,
    pub contract_name: String,
    pub compiler_version: String,
    pub optimization_used: String,
    pub zk_compiler_version: String,
}

/// Contains metadata and path mapped source code.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SCMetadata {
    pub settings: ZkSettings,
}

/// Deserializes as JSON either:
///
/// - Object: `{ "SourceCode": { language: "Solidity", .. }, ..}`
/// - Stringified JSON object:
///     - `{ "SourceCode": "{{\r\n  \"language\": \"Solidity\", ..}}", ..}`
///     - `{ "SourceCode": "{ \"file.sol\": \"...\" }", ... }`
/// - Normal source code string: `{ "SourceCode": "// SPDX-License-Identifier: ...", .. }`
fn deserialize_source_code<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<SCMetadata, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum SourceCode {
        String(String), // this must come first
        Obj(SCMetadata),
    }
    let s = SourceCode::deserialize(deserializer)?;
    match s {
        SourceCode::String(s) => {
            if s.starts_with('{') && s.ends_with('}') {
                let mut s = s.as_str();
                // skip double braces
                if s.starts_with("{{") && s.ends_with("}}") {
                    s = &s[1..s.len() - 1];
                }
                serde_json::from_str(s).map_err(serde::de::Error::custom)
            } else {
                Err(serde::de::Error::custom("expected object for SourceCode field"))
            }
        }
        SourceCode::Obj(obj) => Ok(obj),
    }
}

async fn block_explorer_contract_source_code(
    verifier_url: &str,
    address: Address,
) -> Result<SourceCode> {
    let req_url = format!("{verifier_url}?module=contract&action=getsourcecode&address={address}");
    let res: Response<Vec<SourceCode>> = reqwest::get(req_url).await?.json().await?;
    Ok(res.result.into_iter().next().unwrap())
}

#[cfg(test)]
mod tests {
    use revm_primitives::hex;

    use super::*;

    #[test]
    fn test_zk_extract_and_compare_bytecodes_both_none() {
        let local = &[0; 32];
        let remote = &[0; 32];
        assert!(extract_and_compare_bytecode(
            local,
            remote,
            BytecodeHash::None,
            BytecodeHash::None
        ));
    }

    #[test]
    fn test_zk_extract_and_compare_bytecodes_none_keccak() {
        let local = &[1; 32];
        let remote = &[[1; 32], [0; 32], [2; 32]].concat();
        assert!(extract_and_compare_bytecode(
            local,
            remote,
            BytecodeHash::None,
            BytecodeHash::Keccak256
        ));
    }

    #[test]
    fn test_zk_extract_and_compare_bytecodes_none_keccak_not_equal() {
        let local = &[1; 32];
        // begins the same as local but has more bytes that are not padding
        let remote = &[[1; 32], [2; 32], [3; 32]].concat();
        assert!(!extract_and_compare_bytecode(
            local,
            remote,
            BytecodeHash::None,
            BytecodeHash::Keccak256
        ));
    }

    #[test]
    fn test_zk_extract_and_compare_bytecodes_none_ifps() {
        let local = &[1; 32];
        let ifps = hex::decode("0000000000000000000000000000000000000000a164697066735822122071ce7744d916cec3c85e1da3504c3491bd01991069dba309504d2d578dfcb8f4002a").unwrap();
        let remote = &[&[1; 32][..], &ifps[..]].concat();

        assert!(extract_and_compare_bytecode(
            local,
            remote,
            BytecodeHash::None,
            BytecodeHash::Ipfs
        ));
    }
}
