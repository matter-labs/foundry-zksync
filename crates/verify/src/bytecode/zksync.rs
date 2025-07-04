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
use foundry_block_explorers::{errors::EtherscanError, Response, ResponseData};
use foundry_cli::utils::{self, LoadConfig};
use foundry_common::{compile::ProjectCompiler, shell};
use foundry_compilers::artifacts::{serde_helpers, EvmVersion};
use foundry_zksync_compilers::compilers::zksolc::settings::{
    BytecodeHash, Codegen, Optimizer, ZkSettings,
};
use reqwest::Url;
use serde::{Deserialize, Deserializer, Serialize};
use yansi::Paint;

/// Run verify-bytecode for zksync. Since there's only runtime bytecode and that bytecode
/// cannot be modified at deploy time by the constructor, the check is much simpler than
/// in EVM as we do not need to simulate the deployment. We just compare the compiled bytecode
/// against the on-chain one.
pub async fn run(args: VerifyBytecodeArgs) -> Result<()> {
    let config = args.load_config()?;
    let provider = utils::get_provider(&config)?;
    // If chain is not set, we try to get it from the RPC.
    // If RPC is not set, the default chain is used.
    let chain = match config.get_rpc_url() {
        Some(_) => utils::get_chain(config.chain, &provider).await?,
        None => config.chain.unwrap_or_default(),
    };

    // Get etherscan values.
    let etherscan_config = config.get_etherscan_config_with_chain(Some(chain))?;
    let etherscan_key = etherscan_config.as_ref().map(|c| c.key.as_str());
    // TODO: we just create etherscan client to get the api url which has some
    // resolution logic baked in. We cannot reuse the client for now as most primitive methods
    // are private and we need zksync specific deserialization.
    let etherscan =
        EtherscanVerificationProvider.client(&args.etherscan, &args.verifier, &config)?;
    let etherscan_api_url = etherscan.etherscan_api_url();

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

    let block_explorer_metadata =
        block_explorer_contract_source_code(etherscan_api_url.clone(), etherscan_key, args.address)
            .await?;

    let onchain_hash_type = block_explorer_metadata
        .source_code
        .settings
        .metadata
        .as_ref()
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
        onchain_hash_type,
    );

    print_result(
        match_type,
        BytecodeType::Runtime,
        &mut json_results,
        &block_explorer_metadata,
        &project.settings.settings,
    )
    .await;

    if shell::is_json() {
        sh_println!("{}", serde_json::to_string(&json_results)?)?;
    }

    Ok(())
}

fn match_bytecodes(
    local_bytecode: &[u8],
    onchain_bytecode: &[u8],
    local_hash_type: BytecodeHash,
    onchain_hash_type: BytecodeHash,
) -> Option<VerificationType> {
    // 1. Try full match
    if local_bytecode == onchain_bytecode {
        // If the bytecode_hash = 'none' in Config. Then it's always a partial match according to
        // sourcify definitions. Ref: https://docs.sourcify.dev/docs/full-vs-partial-match/.
        if local_hash_type == BytecodeHash::None {
            return Some(VerificationType::Partial);
        }

        Some(VerificationType::Full)
    } else {
        extract_and_compare_bytecode(
            local_bytecode,
            onchain_bytecode,
            local_hash_type,
            onchain_hash_type,
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
            let metadata_with_trailer_len: usize =
                (u16::from_be_bytes([metadata_len[0], metadata_len[1]]) + 2).into();
            if metadata_with_trailer_len <= bytecode.len() {
                // metadata will be 0 padded to be a multiple of 32
                metadata_with_trailer_len + (32 - metadata_with_trailer_len % 32)
            } else {
                0
            }
        }
    };

    &bytecode[..bytecode.len() - hash_len]
}

async fn print_result(
    res: Option<VerificationType>,
    bytecode_type: BytecodeType,
    json_results: &mut Vec<JsonResult>,
    block_explorer_metadata: &Metadata,
    zk_solc_settings: &ZkSettings,
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
        let mismatches = find_mismatch_in_settings(block_explorer_metadata, zk_solc_settings);
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

fn find_mismatch_in_settings(
    block_explorer_metadata: &Metadata,
    local_settings: &ZkSettings,
) -> Vec<String> {
    let block_explorer_settings = &block_explorer_metadata.source_code.settings;
    let mut mismatches: Vec<String> = vec![];

    if local_settings.evm_version != block_explorer_settings.evm_version {
        let str = format!(
            "EVM version mismatch: local={}, onchain={}",
            local_settings.evm_version.unwrap_or_default(),
            block_explorer_settings.evm_version.unwrap_or_default()
        );
        mismatches.push(str);
    }

    if local_settings.codegen != block_explorer_settings.codegen {
        let str = format!(
            "Codegen mismatch: local={:?}, onchain={:?}",
            local_settings.codegen, block_explorer_settings.codegen
        );
        mismatches.push(str);
    }

    let local_llvm_options = local_settings.llvm_options.clone().join(",");
    let block_explorer_llvm_options = block_explorer_settings.llvm_options.clone().join(",");
    if local_llvm_options != block_explorer_llvm_options {
        let str = format!(
            "LLVM options mismatch: local={local_llvm_options}, onchain={block_explorer_llvm_options}"
        );
        mismatches.push(str);
    }

    if local_settings.enable_eravm_extensions != block_explorer_settings.enable_eravm_extensions {
        let str = format!(
            "EraVM extensions mismatch: local={}, onchain={}",
            local_settings.enable_eravm_extensions, block_explorer_settings.enable_eravm_extensions
        );
        mismatches.push(str);
    }

    let local_optimizer = local_settings.optimizer.enabled.unwrap_or_default();
    let block_explorer_optimizer = block_explorer_settings.optimizer.enabled.unwrap_or_default();
    if block_explorer_optimizer != local_optimizer {
        let str = format!(
            "Optimizer mismatch: local={local_optimizer}, onchain={block_explorer_optimizer}"
        );
        mismatches.push(str);
    }

    let local_optimizer_mode = local_settings.optimizer.mode.unwrap_or('3');
    let block_explorer_optimizer_mode = block_explorer_settings.optimizer.mode.unwrap_or('3');
    if block_explorer_optimizer_mode != local_optimizer_mode {
        let str = format!(
            "Optimizer mode mismatch: local={local_optimizer_mode}, onchain={block_explorer_optimizer_mode}",
        );
        mismatches.push(str);
    }

    let local_foz = local_settings.optimizer.fallback_to_optimizing_for_size.unwrap_or_default();
    let block_explorer_foz =
        block_explorer_settings.optimizer.fallback_to_optimizing_for_size.unwrap_or_default();
    if local_foz != block_explorer_foz {
        let str = format!(
            "Optimizer fallback_to_optimizing_for_size mismatch: local={local_foz}, onchain={block_explorer_foz}"
        );
        mismatches.push(str);
    }

    mismatches
}

// TODO: structs and methods below are all adaptations of foundry-block-explorers's
// clients to be able to deserialize into zksync specific values. Maybe it is
// worth submitting a PR to block explorers to generalize methods or make certain
// internals public in order to be able to reuse more of that code.

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Metadata {
    #[serde(deserialize_with = "deserialize_source_code")]
    pub source_code: SourceCodeMetadata,
}

/// Contains metadata and path mapped source code.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SourceCodeMetadata {
    pub settings: BEZkSettings,
}

// TODO: We need a dedicated struct because deserializing directly into ZkSettings fails
// for etherscan. We could potentially make ZkSettings compatible with this somehow, worse
// case scenario via a `flatten` that has this fields
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BEZkSettings {
    pub codegen: Codegen,
    #[serde(default, with = "serde_helpers::display_from_str_opt")]
    pub evm_version: Option<EvmVersion>,
    #[serde(default, alias = "LLVMOptions")]
    pub llvm_options: Vec<String>,
    #[serde(default, rename = "enableEraVMExtensions")]
    pub enable_eravm_extensions: bool,
    pub optimizer: Optimizer,
    #[serde(default)]
    pub metadata: Option<foundry_zksync_compilers::compilers::zksolc::settings::SettingsMetadata>,
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
) -> std::result::Result<SourceCodeMetadata, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum SourceCode {
        String(String),
        Obj(SourceCodeMetadata),
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
    verifier_url: Url,
    api_key: Option<&str>,
    address: Address,
) -> Result<Metadata> {
    let client = reqwest::Client::new();

    let addr_str = &address.to_string();

    let mut query_params =
        vec![("module", "contract"), ("action", "getsourcecode"), ("address", addr_str)];
    if let Some(key) = api_key {
        query_params.push(("apikey", key));
    }

    let response = client.get(verifier_url).query(&query_params).send().await?.text().await?;
    if response.contains("Contract source code not verified") {
        return Err(EtherscanError::ContractCodeNotVerified(address).into());
    }

    let response = sanitize_response(&response)?;
    Ok(response.result.into_iter().next().unwrap())
}

fn sanitize_response(res: impl AsRef<str>) -> Result<Response<Vec<Metadata>>> {
    let res = res.as_ref();
    let res: ResponseData<Vec<Metadata>> = serde_json::from_str(res).map_err(|error| {
        error!(target: "etherscan", ?res, "Failed to deserialize response: {}", error);
        if res == "Page not found" {
            EtherscanError::PageNotFound
        } else {
            EtherscanError::Serde { error, content: res.to_string() }
        }
    })?;

    match res {
        ResponseData::Error { result, message, status } => {
            if let Some(ref result) = result {
                if result.starts_with("Max rate limit reached") {
                    return Err(EtherscanError::RateLimitExceeded.into());
                } else if result.to_lowercase().contains("invalid api key") {
                    return Err(EtherscanError::InvalidApiKey.into());
                }
            }
            Err(EtherscanError::ErrorResponse { status, message, result }.into())
        }
        ResponseData::Success(res) => Ok(res),
    }
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
    fn test_zk_extract_and_compare_bytecodes_none_ipfs() {
        let local = &[1; 32];
        let ipfs = hex::decode("0000000000000000000000000000000000000000a164697066735822122071ce7744d916cec3c85e1da3504c3491bd01991069dba309504d2d578dfcb8f4002a").unwrap();
        let remote = &[&[1; 32][..], &ipfs[..]].concat();

        assert!(extract_and_compare_bytecode(
            local,
            remote,
            BytecodeHash::None,
            BytecodeHash::Ipfs
        ));
    }
}
