use super::{VerifyArgs, VerifyCheckArgs, provider::VerificationProvider};
use crate::zk_provider::{CompilerVerificationContext, ZkVerificationContext};
use alloy_json_abi::Function;
use alloy_primitives::hex;
use eyre::{Result, eyre};
use foundry_cli::opts::EtherscanOpts;
use foundry_common::{abi::encode_function_args, retry::Retry};
use foundry_zksync_compilers::compilers::zksolc::{
    ZKSOLC_FIRST_VERSION_SUPPORTS_CBOR, ZKSYNC_SOLC_REVISIONS, input::StandardJsonCompilerInput,
};
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, thread::sleep, time::Duration};
use zksync_types::{
    H160,
    contract_verification::etherscan::{
        EtherscanBoolean, EtherscanCodeFormat, EtherscanVerificationRequest,
    },
};

pub mod standard_json;

// Etherscan-compatible API structures
#[derive(Debug, Deserialize, Serialize)]
pub struct EtherscanResponse {
    pub status: String,
    pub message: String,
    pub result: String,
}

#[derive(Debug, Serialize)]
pub struct EtherscanRequest {
    pub module: String,
    pub action: String,
    #[serde(flatten)]
    pub verification_request: EtherscanVerificationRequest,
}

#[derive(Debug, Serialize)]
pub struct EtherscanCheckStatusRequest {
    pub module: String,
    pub action: String,
    pub guid: String,
}

#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct ZkVerificationProvider;

pub trait ZksyncSourceProvider: Send + Sync + Debug {
    fn zk_source(
        &self,
        context: &ZkVerificationContext,
    ) -> Result<(StandardJsonCompilerInput, String)>;
}

#[async_trait::async_trait]
impl VerificationProvider for ZkVerificationProvider {
    async fn preflight_verify_check(
        &mut self,
        args: VerifyArgs,
        context: CompilerVerificationContext,
    ) -> Result<()> {
        let _ = self.prepare_request(&args, &context).await?;
        Ok(())
    }

    async fn verify(
        &mut self,
        args: VerifyArgs,
        context: CompilerVerificationContext,
    ) -> Result<()> {
        trace!("ZkVerificationProvider::verify");
        let verification_request = self.prepare_request(&args, &context).await?;

        let request = EtherscanRequest {
            module: "contract".to_string(),
            action: "verifysourcecode".to_string(),
            verification_request: verification_request.clone(),
        };

        let fq_name = args
            .contract
            .as_ref()
            .map(|ci| {
                ci.path
                    .as_ref()
                    .map(|p| format!("{}:{}", p, ci.name))
                    .unwrap_or_else(|| ci.name.clone())
            })
            .unwrap_or_else(|| verification_request.contract_name.clone());

        let client = reqwest::Client::new();
        let retry: Retry = args.retry.into_retry();

        let maybe_id: Option<String> = retry
            .run_async(|| {
                async {
                    sh_println!(
                        "\nSubmitting verification for [{}] at address {}.",
                        fq_name,
                        verification_request.contract_address
                    )?;

                    let verifier_url = args
                        .verifier
                        .verifier_url
                        .as_deref()
                        .ok_or_else(|| eyre::eyre!("verifier_url must be specified"))?;

                    let form_data = serde_urlencoded::to_string(&request)
                        .map_err(|e| eyre::eyre!("Failed to serialize request as form data: {}", e))?;



                    let resp = client
                        .post(verifier_url)
                        .header("Content-Type", "application/x-www-form-urlencoded")
                        .body(form_data)
                        .send()
                        .await?;

                    let status = resp.status();
                    let body = resp.text().await?;

                    if !status.is_success() {
                        eyre::bail!(
                            "Verification request for address ({}) failed with status {}.\nDetails: {}",
                            args.address,
                            status,
                            body,
                        );
                    }

                    let etherscan_response: EtherscanResponse = serde_json::from_str(&body)
                        .map_err(|e| eyre::eyre!("Failed to parse Etherscan response: {}", e))?;

                    if etherscan_response.result.contains("already verified") {
                        sh_println!(
                            "Contract [{}] \"{}\" is already verified. Skipping verification.",
                            fq_name,
                            verification_request.contract_address
                        )?;
                        return Ok(None);
                    }

                    if etherscan_response.status == "1" {
                        Ok(Some(etherscan_response.result))
                    } else {
                        eyre::bail!(
                            "Verification failed: {}",
                            etherscan_response.result
                        );
                    }
                }
                .boxed()
            })
            .await?;

        if let Some(verification_id) = maybe_id {
            sh_println!(
                "Verification submitted successfully. Verification ID: {}",
                verification_id
            )?;

            self.check(VerifyCheckArgs {
                id: verification_id,
                verifier: args.verifier.clone(),
                retry: args.retry,
                etherscan: EtherscanOpts::default(),
            })
            .await?;
        } else {
            // Already verified → skip entirely
            return Ok(());
        }

        Ok(())
    }

    async fn check(&self, args: VerifyCheckArgs) -> Result<()> {
        sh_println!(
            "Checking verification status for ID: {} using verifier: {} at URL: {}",
            args.id,
            args.verifier.verifier,
            args.verifier.verifier_url.as_deref().unwrap_or("URL not specified")
        )?;
        let max_retries = args.retry.retries;
        let delay_in_seconds = args.retry.delay;

        let client = reqwest::Client::new();
        let base_url = args.verifier.verifier_url.as_deref().ok_or_else(|| {
            eyre::eyre!("verifier_url must be specified either in the config or through the CLI")
        })?;

        let verification_status = self
            .retry_verification_status(&client, base_url, &args.id, max_retries, delay_in_seconds)
            .await?;

        self.process_status_response_etherscan(verification_status, base_url)
    }
}

impl ZkVerificationProvider {
    fn source_provider(&self) -> Box<dyn ZksyncSourceProvider> {
        Box::new(standard_json::ZksyncStandardJsonSource)
    }
    async fn prepare_request(
        &mut self,
        args: &VerifyArgs,
        context: &CompilerVerificationContext,
    ) -> Result<EtherscanVerificationRequest> {
        let (source, contract_name) = if let CompilerVerificationContext::ZkSolc(context) = context
        {
            self.source_provider().zk_source(context)?
        } else {
            eyre::bail!("Unsupported compiler context: only ZkSolc is supported");
        };

        let (solc_version, zk_compiler_version) = match context {
            CompilerVerificationContext::ZkSolc(zk_context) => {
                // Format solc_version as "zkVM-{compiler_version}-1.0.2"
                let solc_revision =
                    if zk_context.compiler_version.zksolc >= ZKSOLC_FIRST_VERSION_SUPPORTS_CBOR {
                        &ZKSYNC_SOLC_REVISIONS[1]
                    } else {
                        &ZKSYNC_SOLC_REVISIONS[0]
                    };
                let solc_version =
                    format!("zkVM-{}-{solc_revision}", zk_context.compiler_version.solc);
                let zk_compiler_version = format!("v{}", zk_context.compiler_version.zksolc);
                (solc_version, zk_compiler_version)
            }
            _ => {
                return Err(eyre::eyre!(
                    "Expected context to be of type ZkSolc, but received a different type."
                ));
            }
        };
        let optimization_used = source.settings.optimizer.enabled.unwrap_or(false);
        let _runs = args.num_of_optimizations.map(|n| n.to_string());
        let constructor_args = self.constructor_args(args, context).await?;

        let request = EtherscanVerificationRequest {
            contract_address: H160::from_slice(args.address.as_slice()),
            source_code: serde_json::to_string(&source)?,
            contract_name,
            compiler_version: solc_version,
            zksolc_version: Some(zk_compiler_version),
            constructor_arguments: constructor_args.unwrap_or_else(String::new),
            optimization_used: if optimization_used {
                Some(EtherscanBoolean::True)
            } else {
                Some(EtherscanBoolean::False)
            },
            code_format: EtherscanCodeFormat::StandardJsonInput,
            evm_version: None,
            runs: _runs,
            optimizer_mode: None,
            compiler_mode: None,
            force_evmla: None,
            is_system: None,
        };

        Ok(request)
    }

    async fn constructor_args(
        &mut self,
        args: &VerifyArgs,
        context: &CompilerVerificationContext,
    ) -> Result<Option<String>> {
        if let Some(ref constructor_args_path) = args.constructor_args_path {
            let abi = context.get_target_abi()?;
            let constructor = abi
                .constructor()
                .ok_or_else(|| eyre!("Can't retrieve constructor info from artifact ABI."))?;
            #[allow(deprecated)]
            let func = Function {
                name: "constructor".to_string(),
                inputs: constructor.inputs.clone(),
                outputs: vec![],
                state_mutability: alloy_json_abi::StateMutability::NonPayable,
            };
            let encoded_args = encode_function_args(
                &func,
                foundry_cli::utils::read_constructor_args_file(
                    constructor_args_path.to_path_buf(),
                )?,
            )?;
            let encoded_args = hex::encode(encoded_args);
            // Note(zk): Form-encoded API expects constructor args without "0x" prefix
            // Strip first 8 chars (function selector) and don't add "0x" prefix
            return Ok(Some(encoded_args[8..].to_string()));
        }

        if let Some(ref args) = args.constructor_args {
            // Note(zk): Form-encoded API expects constructor args without "0x" prefix
            // Strip "0x" prefix if present for form encoding compatibility

            if let Some(args) = args.strip_prefix("0x") {
                return Ok(Some(args.to_string()));
            } else {
                return Ok(Some(args.clone()));
            }
        }

        // Note(zk): Form-encoded API expects empty string for contracts with no constructor args,
        // not "0x". The JSON API used to expect "0x", but form encoding expects empty string.
        Ok(Some(String::new()))
    }
    /// Retry logic for checking the verification status
    async fn retry_verification_status(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        verification_id: &str,
        max_retries: u32,
        delay_in_seconds: u32,
    ) -> Result<EtherscanResponse> {
        let mut retries = 0;

        loop {
            let check_request = EtherscanCheckStatusRequest {
                module: "contract".to_string(),
                action: "checkverifystatus".to_string(),
                guid: verification_id.to_string(),
            };

            let form_data = serde_urlencoded::to_string(&check_request).map_err(|e| {
                eyre::eyre!("Failed to serialize check request as form data: {}", e)
            })?;

            let response = client
                .post(base_url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(form_data)
                .send()
                .await?;

            let status = response.status();
            let text = response.text().await?;

            if !status.is_success() {
                eyre::bail!(
                    "Failed to request verification status with status code {}\nDetails: {}",
                    status,
                    text
                );
            }

            let resp: EtherscanResponse = serde_json::from_str(&text)
                .map_err(|e| eyre::eyre!("Failed to parse Etherscan response: {}", e))?;

            if resp.status == "0" && resp.result.contains("Pending in queue") {
                if retries >= max_retries {
                    let _ =
                        sh_println!("Verification is still pending after {max_retries} retries.");
                    return Ok(resp);
                }

                retries += 1;

                let delay_in_ms = calculate_retry_delay(retries, delay_in_seconds);
                sleep(Duration::from_millis(delay_in_ms));
                continue;
            }

            return Ok(resp);
        }
    }

    fn process_status_response_etherscan(
        &self,
        response: EtherscanResponse,
        verification_url: &str,
    ) -> Result<()> {
        match response.status.as_str() {
            "1" => {
                if response.result.contains("Pass - Verified") {
                    let _ = sh_println!("Verification was successful.");
                } else {
                    let _ = sh_println!("Verification completed: {}", response.result);
                }
            }
            "0" => {
                if response.result.contains("Pending in queue") {
                    let _ = sh_println!("Verification is still pending.");
                } else if response.result.contains("already verified") {
                    let _ = sh_println!("Contract source code already verified.");
                } else {
                    eyre::bail!(
                        "Verification failed:\n\n{}\n\nView verification response:\n{}",
                        response.result,
                        verification_url
                    );
                }
            }
            _ => {
                eyre::bail!(
                    "Unknown verification status: {} - {}\n\nView verification response:\n{}",
                    response.status,
                    response.result,
                    verification_url
                );
            }
        }
        Ok(())
    }
}

/// Calculate the delay for the next retry attempt
fn calculate_retry_delay(retries: u32, base_delay_in_seconds: u32) -> u64 {
    let base_delay_in_ms = (base_delay_in_seconds * 1000) as u64;
    base_delay_in_ms * (1 << retries.min(5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zk_etherscan_response_parsing_success() {
        let provider = ZkVerificationProvider;
        let response = EtherscanResponse {
            status: "1".to_string(),
            message: "OK".to_string(),
            result: "Pass - Verified".to_string(),
        };

        let result = provider.process_status_response_etherscan(response, "http://test.com");
        assert!(result.is_ok(), "Success response should be processed correctly");
    }

    #[test]
    fn test_zk_etherscan_response_parsing_pending() {
        let provider = ZkVerificationProvider;
        let response = EtherscanResponse {
            status: "0".to_string(),
            message: "NOTOK".to_string(),
            result: "Pending in queue".to_string(),
        };

        let result = provider.process_status_response_etherscan(response, "http://test.com");
        assert!(result.is_ok(), "Pending response should be processed correctly");
    }

    #[test]
    fn test_zk_etherscan_check_status_request_serialization() {
        let request = EtherscanCheckStatusRequest {
            module: "contract".to_string(),
            action: "checkverifystatus".to_string(),
            guid: "test-guid-123".to_string(),
        };

        let form_data = serde_urlencoded::to_string(&request);
        assert!(form_data.is_ok(), "Check status serialization should succeed");

        let serialized = form_data.unwrap();
        assert!(serialized.contains("module=contract"));
        assert!(serialized.contains("action=checkverifystatus"));
        assert!(serialized.contains("guid=test-guid-123"));
    }
}
