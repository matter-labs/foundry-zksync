//! Contains various definitions and items related to deploy-time linking

use std::{borrow::Borrow, collections::BTreeMap, path::Path};

use alloy_json_abi::JsonAbi;
use alloy_primitives::{Address, Bytes, TxKind, B256, U256};
use eyre::Context;
use foundry_common::{ContractsByArtifact, TestFunctionExt, TransactionMaybeSigned};
use foundry_compilers::{
    artifacts::Libraries, contracts::ArtifactContracts, Artifact, ArtifactId, ProjectCompileOutput,
};
use foundry_evm_core::decode::RevertDecoder;
use foundry_linking::{Linker, LinkerError};

use crate::executors::{DeployResult, EvmError, Executor};

use super::{EvmExecutorStrategyRunner, ExecutorStrategyRunner};

pub struct LinkOutput {
    pub deployable_contracts: BTreeMap<ArtifactId, (JsonAbi, Bytes)>,
    pub revert_decoder: RevertDecoder,
    pub linked_contracts: ArtifactContracts,
    pub known_contracts: ContractsByArtifact,
    pub libs_to_deploy: Vec<Bytes>,
    pub libraries: Libraries,
}

/// Type of library deployment
#[derive(Debug, Clone)]
pub enum DeployLibKind {
    /// CREATE(bytecode)
    Create(Bytes),

    /// CREATE2(salt, bytecode)
    Create2(B256, Bytes),
}

/// Represents the result of a library deployment
#[derive(Debug)]
pub struct DeployLibResult {
    /// Result of the deployment
    pub result: DeployResult,
    /// Equivalent transaction to deploy the given library
    pub tx: Option<TransactionMaybeSigned>,
}

impl EvmExecutorStrategyRunner {
    pub(super) fn link_impl(
        &self,
        root: &Path,
        input: &ProjectCompileOutput,
        deployer: Address,
    ) -> Result<LinkOutput, LinkerError> {
        let contracts =
            input.artifact_ids().map(|(id, v)| (id.with_stripped_file_prefixes(root), v)).collect();
        let linker = Linker::new(root, contracts);

        // Build revert decoder from ABIs of all artifacts.
        let abis = linker
            .contracts
            .iter()
            .filter_map(|(_, contract)| contract.abi.as_ref().map(|abi| abi.borrow()));
        let revert_decoder = RevertDecoder::new().with_abis(abis);

        let foundry_linking::LinkOutput { libraries, libs_to_deploy } = linker
            .link_with_nonce_or_address(Default::default(), deployer, 0, linker.contracts.keys())?;

        let linked_contracts = linker.get_linked_artifacts(&libraries)?;

        // Create a mapping of name => (abi, deployment code, Vec<library deployment code>)
        let mut deployable_contracts = BTreeMap::default();
        for (id, contract) in linked_contracts.iter() {
            let Some(abi) = &contract.abi else { continue };

            // if it's a test, link it and add to deployable contracts
            if abi.constructor.as_ref().map(|c| c.inputs.is_empty()).unwrap_or(true) &&
                abi.functions().any(|func| func.name.is_any_test())
            {
                let Some(bytecode) =
                    contract.get_bytecode_bytes().map(|b| b.into_owned()).filter(|b| !b.is_empty())
                else {
                    continue;
                };

                deployable_contracts.insert(id.clone(), (abi.clone(), bytecode));
            }
        }

        let known_contracts = ContractsByArtifact::new(linked_contracts.clone());

        Ok(LinkOutput {
            deployable_contracts,
            revert_decoder,
            linked_contracts,
            known_contracts,
            libs_to_deploy,
            libraries,
        })
    }

    pub(super) fn deploy_library_impl(
        &self,
        executor: &mut Executor,
        from: Address,
        kind: DeployLibKind,
        value: U256,
        rd: Option<&RevertDecoder>,
    ) -> Result<Vec<DeployLibResult>, EvmError> {
        let nonce = self.get_nonce(executor, from).context("retrieving sender nonce")?;

        match kind {
            DeployLibKind::Create(code) => {
                executor.deploy(from, code.clone(), value, rd).map(|dr| {
                    let mut request = TransactionMaybeSigned::new(Default::default());
                    let unsigned = request.as_unsigned_mut().unwrap();
                    unsigned.from = Some(from);
                    unsigned.input = code.into();
                    unsigned.nonce = Some(nonce);

                    vec![DeployLibResult { result: dr, tx: Some(request) }]
                })
            }
            DeployLibKind::Create2(salt, code) => {
                let create2_deployer = executor.create2_deployer();

                let calldata: Bytes = [salt.as_ref(), code.as_ref()].concat().into();
                let result =
                    executor.transact_raw(from, create2_deployer, calldata.clone(), value)?;
                let result = result.into_result(rd)?;

                let address = result
                    .out
                    .as_ref()
                    .and_then(|out| out.address().cloned())
                    .unwrap_or_else(|| create2_deployer.create2_from_code(salt, code.as_ref()));
                debug!(%address, "deployed contract with create2");

                let mut request = TransactionMaybeSigned::new(Default::default());
                let unsigned = request.as_unsigned_mut().unwrap();
                unsigned.from = Some(from);
                unsigned.input = calldata.into();
                unsigned.nonce = Some(nonce);
                unsigned.to = Some(TxKind::Call(create2_deployer));

                // manually increase nonce when performing CALLs
                executor
                    .set_nonce(from, nonce + 1)
                    .context("increasing nonce after CREATE2 deployment")?;

                Ok(vec![DeployLibResult {
                    result: DeployResult { raw: result, address },
                    tx: Some(request),
                }])
            }
        }
    }
}
