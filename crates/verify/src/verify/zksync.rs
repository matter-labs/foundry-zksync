use std::{collections::HashSet, path::PathBuf};

use alloy_provider::Provider;
use eyre::Result;
use foundry_cli::utils::{self, LoadConfig};
use foundry_common::{compile::ProjectCompiler, ContractsByArtifact};
use foundry_compilers::solc::Solc;
use foundry_config::SolcReq;
use itertools::Itertools;

use crate::zk_provider::ZkVerificationContext;

use super::VerifyArgs;

impl VerifyArgs {
    /// Resolves [`ZkVerificationContext`] object either from entered contract name or
    /// by trying to match bytecode located at given address.
    ///
    /// Will assume configured compiler is zksolc
    pub(super) async fn zk_resolve_context(&self) -> Result<ZkVerificationContext> {
        let mut config = self.load_config()?;
        config.libraries.extend(self.libraries.clone());

        let project = foundry_config::zksync::config_create_project(&config, config.cache, false)?;

        if let Some(ref contract) = self.contract {
            let contract_path = if let Some(ref path) = contract.path {
                project.root().join(PathBuf::from(path))
            } else {
                project.find_contract_path(&contract.name)?
            };

            let version = if let Some(ref version) = self.compiler_version {
                version.trim_start_matches('v').parse()?
            } else if let Some(ref solc) = config.solc {
                match solc {
                    SolcReq::Version(version) => version.to_owned(),
                    SolcReq::Local(solc) => Solc::new(solc)?.version,
                }
            } else if let Some(entry) = project
                .read_cache_file()
                .ok()
                .and_then(|mut cache| cache.files.remove(&contract_path))
            {
                let unique_versions = entry
                    .artifacts
                    .get(&contract.name)
                    .map(|artifacts| artifacts.keys().collect::<HashSet<_>>())
                    .unwrap_or_default();

                if unique_versions.is_empty() {
                    eyre::bail!("No matching artifact found for {}", contract.name);
                } else if unique_versions.len() > 1 {
                    warn!(
                        "Ambiguous compiler versions found in cache: {}",
                        unique_versions.iter().join(", ")
                    );
                    eyre::bail!("Compiler version has to be set in `foundry.toml`. If the project was not deployed with foundry, specify the version through `--compiler-version` flag.")
                }

                unique_versions.into_iter().next().unwrap().to_owned()
            } else {
                eyre::bail!("If cache is disabled, compiler version must be either provided with `--compiler-version` option or set in foundry.toml")
            };

            ZkVerificationContext::new(contract_path, contract.name.clone(), version, config)
        } else {
            if config.get_rpc_url().is_none() {
                eyre::bail!("You have to provide a contract name or a valid RPC URL")
            }
            let provider = utils::get_provider(&config)?;
            let code = provider.get_code_at(self.address).await?;

            let output = ProjectCompiler::new().zksync_compile(&project)?;
            let contracts = ContractsByArtifact::new(
                output.artifact_ids().map(|(id, artifact)| (id, artifact.clone().into())),
            );

            let Some((artifact_id, _)) = contracts.find_by_deployed_code_exact(&code) else {
                eyre::bail!(format!(
                    "Bytecode at {} does not match any local contracts",
                    self.address
                ))
            };

            ZkVerificationContext::new(
                artifact_id.source.clone(),
                artifact_id.name.split('.').next().unwrap().to_owned(),
                artifact_id.version.clone(),
                config,
            )
        }
    }
}
