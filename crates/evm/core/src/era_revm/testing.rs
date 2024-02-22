use crate::{
    backend::{Backend, DatabaseError, DatabaseExt, ForkInfo, LocalForkId},
    fork::{CreateFork, ForkId},
};

use crate::backend::RevertDiagnostic;
use ethers_core::utils::GenesisAccount;
use revm::{
    primitives::{AccountInfo, Address, Bytecode, Env, ResultAndState, B256, U256},
    Inspector, JournaledState,
};
use std::collections::HashMap;

#[derive(Default)]
pub struct MockDatabase {
    pub basic: HashMap<Address, AccountInfo>,
}

impl revm::Database for MockDatabase {
    type Error = DatabaseError;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        Ok(self.basic.get(&address).cloned())
    }

    fn code_by_hash(&mut self, _code_hash: B256) -> Result<Bytecode, Self::Error> {
        todo!()
    }

    fn storage(&mut self, _address: Address, _index: U256) -> Result<U256, Self::Error> {
        Ok(U256::ZERO)
    }

    fn block_hash(&mut self, _number: U256) -> Result<B256, Self::Error> {
        todo!()
    }
}

impl DatabaseExt for MockDatabase {
    fn call_with_evm(&mut self, _env: Env) -> eyre::Result<ResultAndState> {
        todo!()
    }

    fn get_fork_info(&mut self, _id: LocalForkId) -> eyre::Result<ForkInfo> {
        todo!()
    }

    fn snapshot(&mut self, _journaled_state: &JournaledState, _env: &Env) -> U256 {
        todo!()
    }

    fn revert(
        &mut self,
        _id: U256,
        _journaled_state: &JournaledState,
        _env: &mut Env,
    ) -> Option<JournaledState> {
        todo!()
    }

    fn create_fork(&mut self, _fork: CreateFork) -> eyre::Result<LocalForkId> {
        todo!()
    }

    fn create_fork_at_transaction(
        &mut self,
        _fork: CreateFork,
        _transaction: B256,
    ) -> eyre::Result<LocalForkId> {
        todo!()
    }

    fn select_fork(
        &mut self,
        _id: LocalForkId,
        _env: &mut Env,
        _journaled_state: &mut JournaledState,
    ) -> eyre::Result<()> {
        todo!()
    }

    fn roll_fork(
        &mut self,
        _id: Option<LocalForkId>,
        _block_number: U256,
        _env: &mut Env,
        _journaled_state: &mut JournaledState,
    ) -> eyre::Result<()> {
        todo!()
    }

    fn roll_fork_to_transaction(
        &mut self,
        _id: Option<LocalForkId>,
        _transaction: B256,
        _env: &mut Env,
        _journaled_state: &mut JournaledState,
    ) -> eyre::Result<()> {
        todo!()
    }

    fn transact<I: Inspector<Backend>>(
        &mut self,
        _id: Option<LocalForkId>,
        _transaction: B256,
        _env: &mut Env,
        _journaled_state: &mut JournaledState,
        _inspector: &mut I,
    ) -> eyre::Result<()> {
        todo!()
    }

    fn active_fork_id(&self) -> Option<LocalForkId> {
        todo!()
    }

    fn active_fork_url(&self) -> Option<String> {
        todo!()
    }

    fn ensure_fork(&self, _id: Option<LocalForkId>) -> eyre::Result<LocalForkId> {
        todo!()
    }

    fn ensure_fork_id(&self, _id: LocalForkId) -> eyre::Result<&ForkId> {
        todo!()
    }

    fn diagnose_revert(
        &self,
        _callee: Address,
        _journaled_state: &JournaledState,
    ) -> Option<RevertDiagnostic> {
        todo!()
    }

    fn load_allocs(
        &mut self,
        _allocs: &HashMap<Address, GenesisAccount>,
        _journaled_state: &mut JournaledState,
    ) -> Result<(), DatabaseError> {
        todo!()
    }

    fn is_persistent(&self, _acc: &Address) -> bool {
        todo!()
    }

    fn remove_persistent_account(&mut self, _account: &Address) -> bool {
        todo!()
    }

    #[doc = " Marks the given account as persistent."]
    fn add_persistent_account(&mut self, _account: Address) -> bool {
        todo!()
    }

    fn allow_cheatcode_access(&mut self, _account: Address) -> bool {
        todo!()
    }

    fn revoke_cheatcode_access(&mut self, _account: Address) -> bool {
        todo!()
    }

    fn has_cheatcode_access(&self, _account: Address) -> bool {
        todo!()
    }
}
