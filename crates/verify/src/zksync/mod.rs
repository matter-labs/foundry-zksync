use super::{provider::VerificationProvider, VerifyArgs, VerifyCheckArgs};
use crate::zk_provider::{CompilerVerificationContext, ZkVerificationContext};
use alloy_json_abi::Function;
use alloy_primitives::hex;
use eyre::{eyre, Result};
use foundry_cli::opts::EtherscanOpts;
use foundry_common::{abi::encode_function_args, retry::Retry};
use foundry_compilers::zksolc::input::StandardJsonCompilerInput;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, thread::sleep, time::Duration};

pub mod standard_json;

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
        let request = self.prepare_request(&args, &context).await?;

        let client = reqwest::Client::new();

        let retry: Retry = args.retry.into();
        let verification_id: u64 = retry
            .run_async(|| {
                async {
                    println!(
                        "\nSubmitting verification for [{}] at address {}.",
                        request.contract_name, request.contract_address
                    );

                    let verifier_url = args
                    .verifier
                    .verifier_url
                    .as_deref()
                    .ok_or_else(|| eyre::eyre!("verifier_url must be specified either in the config or through the CLI"))?;

                    let response = client
                        .post(verifier_url)
                        .header("Content-Type", "application/json")
                        .json(&request)
                        .send()
                        .await?;

                    let status = response.status();
                    let text = response.text().await?;

                    if !status.is_success() {
                        eyre::bail!(
                            "Verification request for address ({}) failed with status code {}\nDetails: {}",
                            args.address,
                            status,
                            text,
                        );
                    }

                    let parsed_id = text.trim().parse().map_err(|e| {
                        eyre::eyre!("Failed to parse verification ID: {} - error: {}", text, e)
                    })?;

                    Ok(parsed_id)
                }
                .boxed()
            })
            .await?;

        println!("Verification submitted successfully. Verification ID: {}", verification_id);

        self.check(VerifyCheckArgs {
            id: verification_id.to_string(),
            verifier: args.verifier.clone(),
            retry: args.retry,
            etherscan: EtherscanOpts::default(),
        })
        .await?;

        Ok(())
    }

    async fn check(&self, args: VerifyCheckArgs) -> Result<()> {
        println!(
            "Checking verification status for ID: {} using verifier: {} at URL: {}",
            args.id,
            args.verifier.verifier,
            args.verifier.verifier_url.as_deref().unwrap_or("URL not specified")
        );
        let max_retries = args.retry.retries;
        let delay_in_seconds = args.retry.delay;

        let client = reqwest::Client::new();
        let base_url = args.verifier.verifier_url.as_deref().ok_or_else(|| {
            eyre::eyre!("verifier_url must be specified either in the config or through the CLI")
        })?;
        let url = format!("{}/{}", base_url, args.id);

        let verification_status =
            self.retry_verification_status(&client, &url, max_retries, delay_in_seconds).await?;

        self.process_status_response(Some(verification_status), &url)
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
    ) -> Result<VerifyContractRequest> {
        let (source, contract_name) = if let CompilerVerificationContext::ZkSolc(context) = context
        {
            self.source_provider().zk_source(context)?
        } else {
            eyre::bail!("Unsupported compiler context: only ZkSolc is supported");
        };

        let (solc_version, zk_compiler_version) = match context {
            CompilerVerificationContext::ZkSolc(zk_context) => {
                // Format solc_version as "zkVM-{compiler_version}-1.0.1"
                let solc_version = format!("zkVM-{}-1.0.1", zk_context.compiler_version.solc);
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
        // TODO: investigate runs better. Currently not included in the verification request.
        let _runs = args.num_of_optimizations.map(|n| n as u64);
        let constructor_args = self.constructor_args(args, context).await?.unwrap_or_default();

        let request = VerifyContractRequest {
            contract_address: args.address.to_string(),
            source_code: source,
            code_format: "solidity-standard-json-input".to_string(),
            contract_name,
            compiler_version: solc_version,
            zk_compiler_version,
            constructor_arguments: constructor_args,
            optimization_used,
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
            return Ok(Some(format!("0x{}", &encoded_args[8..])));
        }

        if let Some(ref args) = args.constructor_args {
            if args.starts_with("0x") {
                return Ok(Some(args.clone()));
            } else {
                return Ok(Some(format!("0x{args}")));
            }
        }

        Ok(Some("0x".to_string()))
    }
    /// Retry logic for checking the verification status
    async fn retry_verification_status(
        &self,
        client: &reqwest::Client,
        url: &str,
        max_retries: u32,
        delay_in_seconds: u32,
    ) -> Result<ContractVerificationStatusResponse> {
        let mut retries = 0;

        loop {
            let response = client.get(url).send().await?;
            let status = response.status();
            let text = response.text().await?;

            if !status.is_success() {
                eyre::bail!(
                    "Failed to request verification status with status code {}\nDetails: {}",
                    status,
                    text
                );
            }

            let resp: ContractVerificationStatusResponse = serde_json::from_str(&text)?;

            if resp.is_pending() || resp.is_queued() {
                if retries >= max_retries {
                    println!("Verification is still pending after {max_retries} retries.");
                    return Ok(resp);
                }

                retries += 1;

                let delay_in_ms = calculate_retry_delay(retries, delay_in_seconds);
                sleep(Duration::from_millis(delay_in_ms));
                continue;
            }

            if resp.is_verification_success() || resp.is_verification_failure() {
                return Ok(resp);
            }
        }
    }

    fn process_status_response(
        &self,
        response: Option<ContractVerificationStatusResponse>,
        verification_url: &str,
    ) -> Result<()> {
        trace!("Processing verification status response. {:?}", response);

        if let Some(resp) = response {
            match resp.status {
                VerificationStatusEnum::Successful => {
                    println!("Verification was successful.");
                }
                VerificationStatusEnum::Failed => {
                    let error_message = resp.get_error(verification_url);
                    eyre::bail!("Verification failed:\n\n{}", error_message);
                }
                VerificationStatusEnum::Queued => {
                    println!("Verification is queued.");
                }
                VerificationStatusEnum::InProgress => {
                    println!("Verification is in progress.");
                }
            }
        } else {
            eyre::bail!("Empty response from verification status endpoint");
        }
        Ok(())
    }
}

/// Calculate the delay for the next retry attempt
fn calculate_retry_delay(retries: u32, base_delay_in_seconds: u32) -> u64 {
    let base_delay_in_ms = (base_delay_in_seconds * 1000) as u64;
    base_delay_in_ms * (1 << retries.min(5))
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatusEnum {
    Successful,
    Failed,
    Queued,
    InProgress,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ContractVerificationStatusResponse {
    pub status: VerificationStatusEnum,
    pub error: Option<String>,
    #[serde(rename = "compilationErrors")]
    pub compilation_errors: Option<Vec<String>>,
}

impl ContractVerificationStatusResponse {
    pub fn get_error(&self, verification_url: &str) -> String {
        let mut error_message = String::new();

        if let Some(ref error) = self.error {
            error_message.push_str("Error:\n");
            error_message.push_str(error);
        }

        // Detailed compilation errors, if any
        if let Some(ref compilation_errors) = self.compilation_errors {
            if !compilation_errors.is_empty() {
                let detailed_errors = compilation_errors
                    .iter()
                    .map(|e| format!("- {e}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                error_message.push_str("\n\nError Details:\n");
                error_message.push_str(&detailed_errors);
            }
        }

        error_message.push_str("\n\nView verification response:\n");
        error_message.push_str(verification_url);

        error_message.trim_end().to_string()
    }
    pub fn is_pending(&self) -> bool {
        matches!(self.status, VerificationStatusEnum::InProgress)
    }
    pub fn is_verification_failure(&self) -> bool {
        matches!(self.status, VerificationStatusEnum::Failed)
    }
    pub fn is_queued(&self) -> bool {
        matches!(self.status, VerificationStatusEnum::Queued)
    }
    pub fn is_verification_success(&self) -> bool {
        matches!(self.status, VerificationStatusEnum::Successful)
    }
}

#[derive(Debug, Serialize)]
pub struct VerifyContractRequest {
    #[serde(rename = "contractAddress")]
    contract_address: String,
    #[serde(rename = "sourceCode")]
    source_code: StandardJsonCompilerInput,
    #[serde(rename = "codeFormat")]
    code_format: String,
    #[serde(rename = "contractName")]
    contract_name: String,
    #[serde(rename = "compilerSolcVersion")]
    compiler_version: String,
    #[serde(rename = "compilerZksolcVersion")]
    zk_compiler_version: String,
    #[serde(rename = "constructorArguments")]
    constructor_arguments: String,
    #[serde(rename = "optimizationUsed")]
    optimization_used: bool,
}
