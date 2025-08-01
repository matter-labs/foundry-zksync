use crate::{
    ScriptArgs, ScriptConfig,
    build::LinkedBuildData,
    sequence::{ScriptSequenceKind, get_commit_hash},
};
use alloy_primitives::{Address, hex};
use eyre::{Result, eyre};
use forge_script_sequence::{AdditionalContract, ScriptSequence};
use forge_verify::{RetryArgs, VerifierArgs, VerifyArgs, provider::VerificationProviderType};
use foundry_cli::opts::{EtherscanOpts, ProjectPathOpts};
use foundry_common::ContractsByArtifact;
use foundry_compilers::{Project, artifacts::EvmVersion, info::ContractInfo};
use foundry_config::{Chain, Config, zksync::ZKSYNC_ARTIFACTS_DIR};
use semver::Version;

/// State after we have broadcasted the script.
/// It is assumed that at this point [BroadcastedState::sequence] contains receipts for all
/// broadcasted transactions.
pub struct BroadcastedState {
    pub args: ScriptArgs,
    pub script_config: ScriptConfig,
    pub build_data: LinkedBuildData,
    pub sequence: ScriptSequenceKind,
}

impl BroadcastedState {
    pub async fn verify(self) -> Result<()> {
        let Self { args, script_config, build_data, mut sequence, .. } = self;

        let verify = VerifyBundle::new(
            &script_config.config.project()?,
            &script_config.config,
            build_data.known_contracts,
            args.retry,
            args.verifier,
        );

        for sequence in sequence.sequences_mut() {
            verify_contracts(sequence, &script_config.config, verify.clone()).await?;
        }

        Ok(())
    }
}

/// Data struct to help `ScriptSequence` verify contracts on `etherscan`.
#[derive(Clone)]
pub struct VerifyBundle {
    pub num_of_optimizations: Option<usize>,
    pub known_contracts: ContractsByArtifact,
    pub project_paths: ProjectPathOpts,
    pub etherscan: EtherscanOpts,
    pub retry: RetryArgs,
    pub verifier: VerifierArgs,
    pub via_ir: bool,
    pub zksync: bool,
}

impl VerifyBundle {
    pub fn new(
        project: &Project,
        config: &Config,
        known_contracts: ContractsByArtifact,
        retry: RetryArgs,
        verifier: VerifierArgs,
    ) -> Self {
        let num_of_optimizations =
            if config.optimizer == Some(true) { config.optimizer_runs } else { None };

        let config_path = config.get_config_path();

        let project_paths = ProjectPathOpts {
            root: Some(project.paths.root.clone()),
            contracts: Some(project.paths.sources.clone()),
            remappings: project.paths.remappings.clone(),
            remappings_env: None,
            cache_path: Some(project.paths.cache.clone()),
            lib_paths: project.paths.libraries.clone(),
            hardhat: config.profile == Config::HARDHAT_PROFILE,
            config_path: if config_path.exists() { Some(config_path) } else { None },
        };

        let via_ir = config.via_ir;
        let zksync = config.zksync.should_compile();

        Self {
            num_of_optimizations,
            known_contracts,
            etherscan: Default::default(),
            project_paths,
            retry,
            verifier,
            via_ir,
            zksync,
        }
    }

    /// Configures the chain and sets the etherscan key, if available
    pub fn set_chain(&mut self, config: &Config, chain: Chain) {
        // If dealing with multiple chains, we need to be able to change in between the config
        // chain_id.
        self.etherscan.key = config.get_etherscan_api_key(Some(chain));
        self.etherscan.chain = Some(chain);
    }

    /// Given a `VerifyBundle` and contract details, it tries to generate a valid `VerifyArgs` to
    /// use against the `contract_address`.
    pub fn get_verify_args(
        &self,
        contract_address: Address,
        create2_offset: usize,
        data: &[u8],
        libraries: &[String],
        evm_version: EvmVersion,
    ) -> Option<VerifyArgs> {
        for (artifact, contract) in self.known_contracts.iter() {
            let Some(bytecode) = contract.bytecode() else { continue };
            // If it's a CREATE2, the tx.data comes with a 32-byte salt in the beginning
            // of the transaction
            if data.split_at(create2_offset).1.starts_with(bytecode) {
                let constructor_args = data.split_at(create2_offset + bytecode.len()).1.to_vec();

                if artifact.source.extension().is_some_and(|e| e.to_str() == Some("vy")) {
                    warn!("Skipping verification of Vyper contract: {}", artifact.name);
                }

                // Strip artifact profile from contract name when creating contract info.
                let contract = ContractInfo {
                    path: Some(artifact.source.to_string_lossy().to_string()),
                    name: artifact
                        .name
                        .strip_suffix(&format!(".{}", &artifact.profile))
                        .unwrap_or_else(|| &artifact.name)
                        .to_string(),
                };

                // We strip the build metadadata information, since it can lead to
                // etherscan not identifying it correctly. eg:
                // `v0.8.10+commit.fc410830.Linux.gcc` != `v0.8.10+commit.fc410830`
                let version = Version::new(
                    artifact.version.major,
                    artifact.version.minor,
                    artifact.version.patch,
                );

                let verify = VerifyArgs {
                    address: contract_address,
                    contract: Some(contract),
                    compiler_version: Some(version.to_string()),
                    constructor_args: Some(hex::encode(constructor_args)),
                    constructor_args_path: None,
                    num_of_optimizations: self.num_of_optimizations,
                    etherscan: self.etherscan.clone(),
                    rpc: Default::default(),
                    flatten: false,
                    force: false,
                    skip_is_verified_check: true,
                    watch: true,
                    retry: self.retry,
                    libraries: libraries.to_vec(),
                    root: None,
                    verifier: self.verifier.clone(),
                    via_ir: self.via_ir,
                    evm_version: Some(evm_version),
                    show_standard_json_input: false,
                    guess_constructor_args: false,
                    compilation_profile: Some(artifact.profile.to_string()),
                    language: None,
                    zksync: self.zksync,
                };

                return Some(verify);
            }
        }
        None
    }

    /// Given a `VerifyBundle` and contract details, it tries to generate a valid `VerifyArgs` to
    /// use against the `contract_address` in the ZKsync context.
    pub fn get_verify_args_zk(
        &self,
        contract_address: Address,
        data: &[u8],
        libraries: &[String],
        evm_version: EvmVersion,
    ) -> Option<VerifyArgs> {
        if data.len() < 4 {
            warn!(
                "failed decoding verify input data, invalid data length, require minimum of 4 bytes"
            );
            return None;
        }
        let selector = hex::encode(&data[..4]);
        let is_create2 = match selector.as_str() {
            foundry_zksync_core::SELECTOR_CONTRACT_DEPLOYER_CREATE => false,
            foundry_zksync_core::SELECTOR_CONTRACT_DEPLOYER_CREATE2 => true,
            _ => {
                warn!("failed decoding verify input data, invalid selector {selector:?}");
                return None;
            }
        };
        let (bytecode_hash, constructor_args) = if is_create2 {
            let (_salt, bytecode_hash, constructor_args) =
                foundry_zksync_core::try_decode_create2(data)
                    .inspect_err(|err| warn!("failed parsing create2 verify data: {err:?}"))
                    .ok()?;
            (bytecode_hash, constructor_args)
        } else {
            foundry_zksync_core::try_decode_create(data)
                .inspect_err(|err| warn!("failed parsing create verify data: {err:?}"))
                .ok()?
        };

        let known_zksync_contracts = self
            .known_contracts
            .iter()
            .filter(|(artifact, _)| artifact.path.to_string_lossy().contains(ZKSYNC_ARTIFACTS_DIR));
        for (artifact, contract) in known_zksync_contracts {
            let Some(bytecode) = contract.bytecode() else { continue };

            let contract_bytecode_hash = foundry_zksync_core::hash_bytecode(bytecode);
            if bytecode_hash == contract_bytecode_hash {
                if artifact.source.extension().is_some_and(|e| e.to_str() == Some("vy")) {
                    warn!("Skipping verification of Vyper contract: {}", artifact.name);
                }

                let contract = ContractInfo {
                    path: Some(artifact.source.to_string_lossy().to_string()),
                    name: artifact.name.clone(),
                };

                // We strip the build metadadata information, since it can lead to
                // etherscan not identifying it correctly. eg:
                // `v0.8.10+commit.fc410830.Linux.gcc` != `v0.8.10+commit.fc410830`
                let version = Version::new(
                    artifact.version.major,
                    artifact.version.minor,
                    artifact.version.patch,
                );

                let verify = VerifyArgs {
                    address: contract_address,
                    contract: Some(contract),
                    compiler_version: Some(version.to_string()),
                    constructor_args: Some(hex::encode(constructor_args)),
                    constructor_args_path: None,
                    num_of_optimizations: self.num_of_optimizations,
                    etherscan: self.etherscan.clone(),
                    rpc: Default::default(),
                    flatten: false,
                    force: false,
                    skip_is_verified_check: true,
                    watch: true,
                    retry: self.retry,
                    libraries: libraries.to_vec(),
                    root: None,
                    verifier: self.verifier.clone(),
                    via_ir: self.via_ir,
                    evm_version: Some(evm_version),
                    show_standard_json_input: false,
                    guess_constructor_args: false,
                    compilation_profile: None, //TODO(zk): get compilation profile
                    language: None,            // TODO(zk): vyper is not supported right now
                    zksync: self.zksync,
                };

                return Some(verify);
            }
        }
        None
    }
}

/// Given the broadcast log, it matches transactions with receipts, and tries to verify any
/// created contract on etherscan.
async fn verify_contracts(
    sequence: &mut ScriptSequence,
    config: &Config,
    mut verify: VerifyBundle,
) -> Result<()> {
    trace!(target: "script", "verifying {} contracts [{}]", verify.known_contracts.len(), sequence.chain);

    verify.set_chain(config, sequence.chain.into());

    if verify.etherscan.has_key() || verify.verifier.verifier != VerificationProviderType::Etherscan
    {
        trace!(target: "script", "prepare future verifications");

        let mut future_verifications = Vec::with_capacity(sequence.receipts.len());
        let mut unverifiable_contracts = vec![];

        // Make sure the receipts have the right order first.
        sequence.sort_receipts();

        for (receipt, tx) in sequence.receipts.iter_mut().zip(sequence.transactions.iter()) {
            // create2 hash offset
            let mut offset = 0;

            if tx.is_create2() {
                receipt.contract_address = tx.contract_address;
                offset = 32;
            }

            // Verify contract created directly from the transaction
            if let (Some(address), Some(data)) = (receipt.contract_address, tx.tx().input()) {
                if config.zksync.run_in_zk_mode() {
                    match verify.get_verify_args_zk(
                        address,
                        data,
                        &sequence.libraries,
                        config.evm_version,
                    ) {
                        Some(verify) => future_verifications.push(verify.run()),
                        None => unverifiable_contracts.push(address),
                    };
                } else {
                    match verify.get_verify_args(
                        address,
                        offset,
                        data,
                        &sequence.libraries,
                        config.evm_version,
                    ) {
                        Some(verify) => future_verifications.push(verify.run()),
                        None => unverifiable_contracts.push(address),
                    };
                }
            }

            // Verify potential contracts created during the transaction execution
            // This will fail in ZKsync context due to `init_code` usage.
            for AdditionalContract { address, init_code, .. } in &tx.additional_contracts {
                match verify.get_verify_args(
                    *address,
                    0,
                    init_code.as_ref(),
                    &sequence.libraries,
                    config.evm_version,
                ) {
                    Some(verify) => future_verifications.push(verify.run()),
                    None => unverifiable_contracts.push(*address),
                };
            }
        }

        trace!(target: "script", "collected {} verification jobs and {} unverifiable contracts", future_verifications.len(), unverifiable_contracts.len());

        check_unverified(sequence, unverifiable_contracts, verify);

        let num_verifications = future_verifications.len();
        let mut num_of_successful_verifications = 0;
        sh_println!("##\nStart verification for ({num_verifications}) contracts")?;
        for verification in future_verifications {
            match verification.await {
                Ok(_) => {
                    num_of_successful_verifications += 1;
                }
                Err(err) => {
                    sh_err!("Failed to verify contract: {err:#}")?;
                }
            }
        }

        if num_of_successful_verifications < num_verifications {
            return Err(eyre!(
                "Not all ({num_of_successful_verifications} / {num_verifications}) contracts were verified!"
            ));
        }

        sh_println!("All ({num_verifications}) contracts were verified!")?;
    }

    Ok(())
}

fn check_unverified(
    sequence: &ScriptSequence,
    unverifiable_contracts: Vec<Address>,
    verify: VerifyBundle,
) {
    if !unverifiable_contracts.is_empty() {
        let _ = sh_warn!(
            "We haven't found any matching bytecode for the following contracts: {:?}.\n\nThis may occur when resuming a verification, but the underlying source code or compiler version has changed.",
            unverifiable_contracts
        );

        if let Some(commit) = &sequence.commit {
            let current_commit = verify
                .project_paths
                .root
                .map(|root| get_commit_hash(&root).unwrap_or_default())
                .unwrap_or_default();

            if &current_commit != commit {
                let _ = sh_warn!(
                    "Script was broadcasted on commit `{commit}`, but we are at `{current_commit}`."
                );
            }
        }
    }
}
