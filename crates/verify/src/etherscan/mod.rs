use crate::{
    VerifierArgs,
    provider::{VerificationContext, VerificationProvider},
    retry::RETRY_CHECK_ON_VERIFY,
    verify::{ContractLanguage, VerifyArgs, VerifyCheckArgs},
    zk_provider::CompilerVerificationContext,
};
use alloy_json_abi::Function;
use alloy_primitives::hex;
use alloy_provider::Provider;
use alloy_rpc_types::TransactionTrait;
use eyre::{Context, OptionExt, Result, eyre};
use foundry_block_explorers::{
    Client, EtherscanApiVersion,
    errors::EtherscanError,
    utils::lookup_compiler_version,
    verify::{CodeFormat, VerifyContract},
};
use foundry_cli::{
    opts::EtherscanOpts,
    utils::{LoadConfig, get_provider, read_constructor_args_file},
};
use foundry_common::{abi::encode_function_args, retry::RetryError};
use foundry_config::Config;
use foundry_evm::constants::DEFAULT_CREATE2_DEPLOYER;
use regex::Regex;
use semver::{BuildMetadata, Version};
use std::{fmt::Debug, sync::LazyLock};

mod zksync;
use zksync::EtherscanZksyncSourceProvider;

mod flatten;

mod standard_json;

pub static RE_BUILD_COMMIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?P<commit>commit\.[0-9,a-f]{8})").unwrap());

#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct EtherscanVerificationProvider;

/// The contract source provider for [EtherscanVerificationProvider]
///
/// Returns source, contract_name and the source [CodeFormat]
trait EtherscanSourceProvider: Send + Sync + Debug + EtherscanZksyncSourceProvider {
    fn source(
        &self,
        args: &VerifyArgs,
        context: &VerificationContext,
    ) -> Result<(String, String, CodeFormat)>;
}

#[async_trait::async_trait]
impl VerificationProvider for EtherscanVerificationProvider {
    async fn preflight_verify_check(
        &mut self,
        args: VerifyArgs,
        context: CompilerVerificationContext,
    ) -> Result<()> {
        let _ = self.prepare_verify_request(&args, &context).await?;
        Ok(())
    }

    async fn verify(
        &mut self,
        args: VerifyArgs,
        context: CompilerVerificationContext,
    ) -> Result<()> {
        let (etherscan, verify_args) = self.prepare_verify_request(&args, &context).await?;

        if !args.skip_is_verified_check
            && self.is_contract_verified(&etherscan, &verify_args).await?
        {
            sh_println!(
                "\nContract [{}] {:?} is already verified. Skipping verification.",
                verify_args.contract_name,
                verify_args.address.to_checksum(None)
            )?;

            return Ok(());
        }

        trace!(?verify_args, "submitting verification request");

        let resp = args
            .retry
            .into_retry()
            .run_async(|| async {
                sh_println!(
                    "\nSubmitting verification for [{}] {}.",
                    verify_args.contract_name,
                    verify_args.address
                )?;
                let resp = etherscan
                    .submit_contract_verification(&verify_args)
                    .await
                    .wrap_err_with(|| {
                        // valid json
                        let args = serde_json::to_string(&verify_args).unwrap();
                        error!(?args, "Failed to submit verification");
                        format!("Failed to submit contract verification, payload:\n{args}")
                    })?;

                trace!(?resp, "Received verification response");

                if resp.status == "0" {
                    if resp.result == "Contract source code already verified"
                        // specific for blockscout response
                        || resp.result == "Smart-contract already verified."
                    {
                        return Ok(None);
                    }

                    if resp.result.starts_with("Unable to locate ContractCode at")
                        || resp.result.starts_with("The address is not a smart contract")
                    {
                        warn!("{}", resp.result);
                        return Err(eyre!("Could not detect the deployment."));
                    }

                    warn!("Failed verify submission: {:?}", resp);
                    sh_err!(
                        "Encountered an error verifying this contract:\nResponse: `{}`\nDetails:
                        `{}`",
                        resp.message,
                        resp.result
                    )?;
                    std::process::exit(1);
                }

                Ok(Some(resp))
            })
            .await?;

        if let Some(resp) = resp {
            sh_println!(
                "Submitted contract for verification:\n\tResponse: `{}`\n\tGUID: `{}`\n\tURL: {}",
                resp.message,
                resp.result,
                etherscan.address_url(args.address)
            )?;

            if args.watch {
                let check_args = VerifyCheckArgs {
                    id: resp.result,
                    etherscan: args.etherscan,
                    retry: RETRY_CHECK_ON_VERIFY,
                    verifier: args.verifier,
                };
                return self.check(check_args).await;
            }
        } else {
            sh_println!("Contract source code already verified")?;
        }

        Ok(())
    }

    /// Executes the command to check verification status on Etherscan
    async fn check(&self, args: VerifyCheckArgs) -> Result<()> {
        let config = args.load_config()?;
        let etherscan = self.client(&args.etherscan, &args.verifier, &config)?;
        args.retry
            .into_retry()
            .run_async_until_break(|| async {
                let resp = etherscan
                    .check_contract_verification_status(args.id.clone())
                    .await
                    .wrap_err("Failed to request verification status")
                    .map_err(RetryError::Retry)?;

                trace!(?resp, "Received verification response");

                let _ = sh_println!(
                    "Contract verification status:\nResponse: `{}`\nDetails: `{}`",
                    resp.message,
                    resp.result
                );

                if resp.result == "Pending in queue" {
                    return Err(RetryError::Retry(eyre!("Verification is still pending...")));
                }

                if resp.result == "Unable to verify" {
                    return Err(RetryError::Retry(eyre!("Unable to verify.")));
                }

                if resp.result == "Already Verified" {
                    let _ = sh_println!("Contract source code already verified");
                    return Ok(());
                }

                if resp.status == "0" {
                    return Err(RetryError::Break(eyre!("Contract failed to verify.")));
                }

                if resp.result == "Pass - Verified" {
                    let _ = sh_println!("Contract successfully verified");
                }

                Ok(())
            })
            .await
            .wrap_err("Checking verification result failed")
    }
}

impl EtherscanVerificationProvider {
    /// Create a source provider
    fn source_provider(&self, args: &VerifyArgs) -> Box<dyn EtherscanSourceProvider> {
        if args.flatten {
            Box::new(flatten::EtherscanFlattenedSource)
        } else {
            Box::new(standard_json::EtherscanStandardJsonSource)
        }
    }

    /// Configures the API request to the Etherscan API using the given [`VerifyArgs`].
    async fn prepare_verify_request(
        &mut self,
        args: &VerifyArgs,
        context: &CompilerVerificationContext,
    ) -> Result<(Client, VerifyContract)> {
        let config = args.load_config()?;
        let etherscan = self.client(&args.etherscan, &args.verifier, &config)?;
        let verify_args = self.create_verify_request(args, context).await?;

        Ok((etherscan, verify_args))
    }

    /// Queries the Etherscan API to verify if the contract is already verified.
    async fn is_contract_verified(
        &self,
        etherscan: &Client,
        verify_contract: &VerifyContract,
    ) -> Result<bool> {
        let check = etherscan.contract_abi(verify_contract.address).await;

        if let Err(err) = check {
            return match err {
                EtherscanError::ContractCodeNotVerified(_) => Ok(false),
                error => Err(error).wrap_err_with(|| {
                    format!("Failed to obtain contract ABI for {}", verify_contract.address)
                }),
            };
        }

        Ok(true)
    }

    /// Create an Etherscan client.
    pub(crate) fn client(
        &self,
        etherscan_opts: &EtherscanOpts,
        verifier_args: &VerifierArgs,
        config: &Config,
    ) -> Result<Client> {
        let chain = etherscan_opts.chain.unwrap_or_default();
        let etherscan_key = etherscan_opts.key();
        let verifier_type = &verifier_args.verifier;
        let verifier_url = verifier_args.verifier_url.as_deref();

        // Verifier is etherscan if explicitly set or if no verifier set (default sourcify) but
        // API key passed.
        let is_etherscan = verifier_type.is_etherscan()
            || (verifier_type.is_sourcify() && etherscan_key.is_some());
        let etherscan_config = config.get_etherscan_config_with_chain(Some(chain))?;

        let api_version = verifier_args.verifier_api_version.unwrap_or_else(|| {
            if is_etherscan {
                etherscan_config.as_ref().map(|c| c.api_version).unwrap_or_default()
            } else {
                EtherscanApiVersion::V1
            }
        });

        let etherscan_api_url = verifier_url
            .or_else(|| {
                if api_version == EtherscanApiVersion::V2 {
                    None
                } else {
                    etherscan_config.as_ref().map(|c| c.api_url.as_str())
                }
            })
            .map(str::to_owned);

        let api_url = etherscan_api_url.as_deref();
        let base_url = etherscan_config
            .as_ref()
            .and_then(|c| c.browser_url.as_deref())
            .or_else(|| chain.etherscan_urls().map(|(_, url)| url));
        let etherscan_key =
            etherscan_key.or_else(|| etherscan_config.as_ref().map(|c| c.key.clone()));

        let mut builder = Client::builder().with_api_version(api_version);

        builder = if let Some(api_url) = api_url {
            // we don't want any trailing slashes because this can cause cloudflare issues: <https://github.com/foundry-rs/foundry/pull/6079>
            let api_url = api_url.trim_end_matches('/');
            let base_url = if !is_etherscan {
                // If verifier is not Etherscan then set base url as api url without /api suffix.
                api_url.strip_prefix("/api").unwrap_or(api_url)
            } else {
                base_url.unwrap_or(api_url)
            };
            builder.with_chain_id(chain).with_api_url(api_url)?.with_url(base_url)?
        } else {
            builder.chain(chain)?
        };

        builder
            .with_api_key(etherscan_key.unwrap_or_default())
            .build()
            .wrap_err("Failed to create Etherscan client")
    }

    /// Creates the `VerifyContract` Etherscan request in order to verify the contract
    ///
    /// If `--flatten` is set to `true` then this will send with [`CodeFormat::SingleFile`]
    /// otherwise this will use the [`CodeFormat::StandardJsonInput`]
    pub async fn create_verify_request(
        &mut self,
        args: &VerifyArgs,
        context: &CompilerVerificationContext,
    ) -> Result<VerifyContract> {
        let (source, contract_name, code_format) = match context {
            CompilerVerificationContext::Solc(context) => {
                self.source_provider(args).source(args, context)?
            }
            CompilerVerificationContext::ZkSolc(context) => {
                // NOTE(zk): this is "source" but it really means the full
                // compiler input, so we need a different one for zksolc
                self.source_provider(args).zksync_source(args, context)?
            }
        };

        let lang = match context {
            CompilerVerificationContext::Solc(context) => args.detect_language(context),
            CompilerVerificationContext::ZkSolc(_context) => {
                // TODO(zk): Vyper is not supported right now
                ContractLanguage::Solidity
            }
        };

        let mut compiler_version = context.compiler_version().clone();
        compiler_version.build = match RE_BUILD_COMMIT.captures(compiler_version.build.as_str()) {
            Some(cap) => BuildMetadata::new(cap.name("commit").unwrap().as_str())?,
            _ => BuildMetadata::EMPTY,
        };

        let compiler_version = if matches!(lang, ContractLanguage::Vyper) {
            format!("vyper:{}", compiler_version.to_string().split('+').next().unwrap_or("0.0.0"))
        } else {
            format!("v{}", ensure_solc_build_metadata(compiler_version.clone()).await?)
        };

        let constructor_args = self.constructor_args(args, context).await?;
        let mut verify_args =
            VerifyContract::new(args.address, contract_name, source, compiler_version)
                .constructor_arguments(constructor_args)
                .code_format(code_format);

        // NOTE(zk): add zksync-specific items to the request
        self.zk_verify_args(context, &mut verify_args);

        if args.via_ir {
            // we explicitly set this __undocumented__ argument to true if provided by the user,
            // though this info is also available in the compiler settings of the standard json
            // object if standard json is used
            // unclear how Etherscan interprets this field in standard-json mode
            verify_args = verify_args.via_ir(true);
        }

        if code_format == CodeFormat::SingleFile {
            verify_args = if let Some(optimizations) = args.num_of_optimizations {
                verify_args.optimized().runs(optimizations as u32)
            } else if context.config().optimizer == Some(true) {
                verify_args
                    .optimized()
                    .runs(context.config().optimizer_runs.unwrap_or(200).try_into()?)
            } else {
                verify_args.not_optimized()
            };
        }

        if code_format == CodeFormat::VyperJson {
            verify_args =
                if args.num_of_optimizations.is_some() || context.config().optimizer == Some(true) {
                    verify_args.optimized().runs(1)
                } else {
                    verify_args.not_optimized().runs(0)
                }
        }

        Ok(verify_args)
    }

    /// Return the optional encoded constructor arguments. If the path to
    /// constructor arguments was provided, read them and encode. Otherwise,
    /// return whatever was set in the [VerifyArgs] args.
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
            let func = Function {
                name: "constructor".to_string(),
                inputs: constructor.inputs.clone(),
                outputs: vec![],
                state_mutability: alloy_json_abi::StateMutability::NonPayable,
            };
            let encoded_args = encode_function_args(
                &func,
                read_constructor_args_file(constructor_args_path.to_path_buf())?,
            )?;
            let encoded_args = hex::encode(encoded_args);
            return Ok(Some(encoded_args[8..].into()));
        }
        if args.guess_constructor_args {
            return Ok(Some(self.guess_constructor_args(args, context).await?));
        }

        Ok(args.constructor_args.clone())
    }

    /// Uses Etherscan API to fetch contract creation transaction.
    /// If transaction is a create transaction or a invocation of default CREATE2 deployer, tries to
    /// match provided creation code with local bytecode of the target contract.
    /// If bytecode match, returns latest bytes of on-chain creation code as constructor arguments.
    async fn guess_constructor_args(
        &mut self,
        args: &VerifyArgs,
        context: &CompilerVerificationContext,
    ) -> Result<String> {
        let provider = get_provider(context.config())?;
        let client = self.client(&args.etherscan, &args.verifier, context.config())?;

        //TODO(zk): EraVM support
        let creation_data = client.contract_creation_data(args.address).await?;
        let transaction = provider
            .get_transaction_by_hash(creation_data.transaction_hash)
            .await?
            .ok_or_eyre("Transaction not found")?;
        let receipt = provider
            .get_transaction_receipt(creation_data.transaction_hash)
            .await?
            .ok_or_eyre("Couldn't fetch transaction receipt from RPC")?;

        let maybe_creation_code = if receipt.contract_address == Some(args.address) {
            transaction.inner.inner.input()
        } else if transaction.to() == Some(DEFAULT_CREATE2_DEPLOYER) {
            &transaction.inner.inner.input()[32..]
        } else {
            eyre::bail!(
                "Fetching of constructor arguments is not supported for contracts created by contracts"
            )
        };

        let bytecode = context.get_target_bytecode()?;
        if maybe_creation_code.starts_with(bytecode.as_ref()) {
            let constructor_args = &maybe_creation_code[bytecode.len()..];
            let constructor_args = hex::encode(constructor_args);
            sh_println!("Identified constructor arguments: {constructor_args}")?;
            Ok(constructor_args)
        } else {
            eyre::bail!("Local bytecode doesn't match on-chain bytecode")
        }
    }
}

/// Given any solc [Version] return a [Version] with build metadata
///
/// # Example
///
/// ```ignore
/// use semver::{BuildMetadata, Version};
/// let version = Version::new(1, 2, 3);
/// let version = ensure_solc_build_metadata(version).await?;
/// assert_ne!(version.build, BuildMetadata::EMPTY);
/// ```
async fn ensure_solc_build_metadata(version: Version) -> Result<Version> {
    if version.build != BuildMetadata::EMPTY {
        Ok(version)
    } else {
        Ok(lookup_compiler_version(&version).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::VerificationProviderType;
    use clap::Parser;
    use foundry_common::fs;
    use foundry_test_utils::{forgetest_async, str};
    use tempfile::tempdir;

    #[test]
    fn can_extract_etherscan_verify_config() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        let config = r#"
                [profile.default]

                [etherscan]
                mumbai = { key = "dummykey", chain = 80001, url = "https://api-testnet.polygonscan.com/" }
            "#;

        let toml_file = root.join(Config::FILE_NAME);
        fs::write(toml_file, config).unwrap();

        let args: VerifyArgs = VerifyArgs::parse_from([
            "foundry-cli",
            "0xd8509bee9c9bf012282ad33aba0d87241baf5064",
            "src/Counter.sol:Counter",
            "--chain",
            "mumbai",
            "--root",
            root.as_os_str().to_str().unwrap(),
        ]);

        let config = args.load_config().unwrap();

        let etherscan = EtherscanVerificationProvider::default();
        let client = etherscan.client(&args.etherscan, &args.verifier, &config).unwrap();
        assert_eq!(client.etherscan_api_url().as_str(), "https://api-testnet.polygonscan.com/");

        assert!(format!("{client:?}").contains("dummykey"));

        let args: VerifyArgs = VerifyArgs::parse_from([
            "foundry-cli",
            "0xd8509bee9c9bf012282ad33aba0d87241baf5064",
            "src/Counter.sol:Counter",
            "--chain",
            "mumbai",
            "--verifier-url",
            "https://verifier-url.com/",
            "--root",
            root.as_os_str().to_str().unwrap(),
        ]);

        let config = args.load_config().unwrap();

        let etherscan = EtherscanVerificationProvider::default();
        let client = etherscan.client(&args.etherscan, &args.verifier, &config).unwrap();
        assert_eq!(client.etherscan_api_url().as_str(), "https://verifier-url.com/");
        assert!(format!("{client:?}").contains("dummykey"));
    }

    #[test]
    fn can_extract_etherscan_v2_verify_config() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        let config = r#"
                [profile.default]

                [etherscan]
                mumbai = { key = "dummykey", chain = 80001, url = "https://api-testnet.polygonscan.com/" }
            "#;

        let toml_file = root.join(Config::FILE_NAME);
        fs::write(toml_file, config).unwrap();

        let args: VerifyArgs = VerifyArgs::parse_from([
            "foundry-cli",
            "0xd8509bee9c9bf012282ad33aba0d87241baf5064",
            "src/Counter.sol:Counter",
            "--verifier",
            "etherscan",
            "--chain",
            "mumbai",
            "--root",
            root.as_os_str().to_str().unwrap(),
        ]);

        let config = args.load_config().unwrap();

        let etherscan = EtherscanVerificationProvider::default();

        let client = etherscan.client(&args.etherscan, &args.verifier, &config).unwrap();

        assert_eq!(client.etherscan_api_url().as_str(), "https://api.etherscan.io/v2/api");
        assert!(format!("{client:?}").contains("dummykey"));

        let args: VerifyArgs = VerifyArgs::parse_from([
            "foundry-cli",
            "0xd8509bee9c9bf012282ad33aba0d87241baf5064",
            "src/Counter.sol:Counter",
            "--verifier",
            "etherscan",
            "--chain",
            "mumbai",
            "--verifier-url",
            "https://verifier-url.com/",
            "--root",
            root.as_os_str().to_str().unwrap(),
        ]);

        let config = args.load_config().unwrap();

        assert_eq!(args.verifier.verifier, VerificationProviderType::Etherscan);

        let etherscan = EtherscanVerificationProvider::default();
        let client = etherscan.client(&args.etherscan, &args.verifier, &config).unwrap();
        assert_eq!(client.etherscan_api_url().as_str(), "https://verifier-url.com/");
        assert_eq!(*client.etherscan_api_version(), EtherscanApiVersion::V2);
        assert!(format!("{client:?}").contains("dummykey"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fails_on_disabled_cache_and_missing_info() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let root_path = root.as_os_str().to_str().unwrap();

        let config = r"
                [profile.default]
                cache = false
            ";

        let toml_file = root.join(Config::FILE_NAME);
        fs::write(toml_file, config).unwrap();

        let address = "0xd8509bee9c9bf012282ad33aba0d87241baf5064";
        let contract_name = "Counter";
        let src_dir = "src";
        fs::create_dir_all(root.join(src_dir)).unwrap();
        let contract_path = format!("{src_dir}/Counter.sol");
        fs::write(root.join(&contract_path), "").unwrap();

        // No compiler argument
        let args = VerifyArgs::parse_from([
            "foundry-cli",
            address,
            &format!("{contract_path}:{contract_name}"),
            "--root",
            root_path,
        ]);
        let result = args.resolve_either_context().await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "If cache is disabled, compiler version must be either provided with `--compiler-version` option or set in foundry.toml"
        );
    }

    forgetest_async!(respects_path_for_duplicate, |prj, cmd| {
        prj.add_source("Counter1", "contract Counter {}").unwrap();
        prj.add_source("Counter2", "contract Counter {}").unwrap();

        cmd.args(["build", "--force"]).assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
...
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

"#]]);

        let args = VerifyArgs::parse_from([
            "foundry-cli",
            "0x0000000000000000000000000000000000000000",
            "src/Counter1.sol:Counter",
            "--root",
            &prj.root().to_string_lossy(),
        ]);
        let context = args.resolve_either_context().await.unwrap();

        let mut etherscan = EtherscanVerificationProvider::default();
        etherscan.preflight_verify_check(args, context).await.unwrap();
    });
}
