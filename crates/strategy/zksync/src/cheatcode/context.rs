use std::collections::HashSet;

use alloy_primitives::{keccak256, map::HashMap, Address, Bytes, B256};
use alloy_sol_types::SolValue;
use foundry_cheatcodes::strategy::CheatcodeInspectorStrategyContext;
use foundry_compilers::info::ContractInfo;
use foundry_evm_core::constants::{CHEATCODE_ADDRESS, CHEATCODE_CONTRACT_HASH};
use foundry_zksync_compilers::dual_compiled_contracts::{
    DualCompiledContract, DualCompiledContracts,
};
use foundry_zksync_core::{vm::ZkEnv, ZkPaymasterData, H256};
use revm::primitives::Bytecode;

use super::types::ZkStartupMigration;

/// Context for [ZksyncCheatcodeInspectorStrategyRunner].
#[derive(Debug, Default, Clone)]
pub struct ZksyncCheatcodeInspectorStrategyContext {
    pub using_zk_vm: bool,

    /// When in zkEVM context, execute the next CALL or CREATE in the EVM instead.
    pub skip_zk_vm: bool,

    /// Any contracts that were deployed in `skip_zk_vm` step.
    /// This makes it easier to dispatch calls to any of these addresses in zkEVM context, directly
    /// to EVM. Alternatively, we'd need to add `vm.zkVmSkip()` to these calls manually.
    pub skip_zk_vm_addresses: HashSet<Address>,

    /// Records the next create address for `skip_zk_vm_addresses`.
    pub record_next_create_address: bool,

    /// Paymaster params
    pub paymaster_params: Option<ZkPaymasterData>,

    /// Dual compiled contracts
    pub dual_compiled_contracts: DualCompiledContracts,

    /// The migration status of the database to zkEVM storage, `None` if we start in EVM context.
    pub zk_startup_migration: ZkStartupMigration,

    /// Factory deps stored through `zkUseFactoryDep`. These factory deps are used in the next
    /// CREATE or CALL, and cleared after.
    pub zk_use_factory_deps: Vec<String>,

    /// The list of factory_deps seen so far during a test or script execution.
    /// Ideally these would be persisted in the storage, but since modifying [revm::JournaledState]
    /// would be a significant refactor, we maintain the factory_dep part in the [Cheatcodes].
    /// This can be done as each test runs with its own [Cheatcodes] instance, thereby
    /// providing the necessary level of isolation.
    pub persisted_factory_deps: HashMap<H256, Vec<u8>>,

    /// Stores the factory deps that were detected as part of CREATE2 deployer call.
    /// Must be cleared every call.
    pub set_deployer_call_input_factory_deps: Vec<Vec<u8>>,

    /// Era Vm environment
    pub zk_env: ZkEnv,

    /// Mark the last recorded account access for removal, on CALL/CREATE-end.
    /// This is a record inserted by revm's cheatcode inspector on CALL/CREATE-begin.
    pub remove_recorded_access_at: Option<usize>,
}

impl ZksyncCheatcodeInspectorStrategyContext {
    pub fn new(dual_compiled_contracts: DualCompiledContracts, zk_env: ZkEnv) -> Self {
        // We add the empty bytecode manually so it is correctly translated in zk mode.
        // This is used in many places in foundry, e.g. in cheatcode contract's account code.
        let empty_bytes = Bytes::from_static(&[0]);
        let zk_bytecode_hash = foundry_zksync_core::hash_bytecode(&foundry_zksync_core::EMPTY_CODE);
        let zk_deployed_bytecode = foundry_zksync_core::EMPTY_CODE.to_vec();

        let mut dual_compiled_contracts = dual_compiled_contracts;
        dual_compiled_contracts.insert(
            ContractInfo::new("EmptyEVMBytecode"),
            DualCompiledContract {
                zk_bytecode_hash,
                zk_deployed_bytecode: zk_deployed_bytecode.clone(),
                zk_factory_deps: Default::default(),
                evm_bytecode_hash: B256::from_slice(&keccak256(&empty_bytes)[..]),
                evm_deployed_bytecode: Bytecode::new_raw(empty_bytes.clone()).bytecode().to_vec(),
                evm_bytecode: Bytecode::new_raw(empty_bytes).bytecode().to_vec(),
            },
        );

        let cheatcodes_bytecode = {
            let mut bytecode = CHEATCODE_ADDRESS.abi_encode_packed();
            bytecode.append(&mut [0; 12].to_vec());
            Bytes::from(bytecode)
        };
        dual_compiled_contracts.insert(
            ContractInfo::new("CheatcodeBytecode"),
            DualCompiledContract {
                // we put a different bytecode hash here so when importing back to EVM
                // we avoid collision with EmptyEVMBytecode for the cheatcodes
                zk_bytecode_hash: foundry_zksync_core::hash_bytecode(
                    CHEATCODE_CONTRACT_HASH.as_ref(),
                ),
                zk_deployed_bytecode: cheatcodes_bytecode.to_vec(),
                zk_factory_deps: Default::default(),
                evm_bytecode_hash: CHEATCODE_CONTRACT_HASH,
                evm_deployed_bytecode: cheatcodes_bytecode.to_vec(),
                evm_bytecode: cheatcodes_bytecode.to_vec(),
            },
        );

        let mut persisted_factory_deps = HashMap::new();
        persisted_factory_deps.insert(zk_bytecode_hash, zk_deployed_bytecode);

        Self {
            using_zk_vm: false, // We need to migrate once on initialize_interp
            skip_zk_vm: false,
            skip_zk_vm_addresses: Default::default(),
            record_next_create_address: Default::default(),
            paymaster_params: Default::default(),
            dual_compiled_contracts,
            zk_startup_migration: ZkStartupMigration::Defer,
            zk_use_factory_deps: Default::default(),
            persisted_factory_deps: Default::default(),
            set_deployer_call_input_factory_deps: Default::default(),
            zk_env,
            remove_recorded_access_at: Default::default(),
        }
    }
}

impl CheatcodeInspectorStrategyContext for ZksyncCheatcodeInspectorStrategyContext {
    fn new_cloned(&self) -> Box<dyn CheatcodeInspectorStrategyContext> {
        Box::new(self.clone())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn as_any_ref(&self) -> &dyn std::any::Any {
        self
    }
}
