use crate::{
    backend::{Backend, DatabaseError, DatabaseExt, LocalForkId},
    fork::{CreateFork, ForkId},
};

use crate::backend::RevertDiagnostic;
use ethers_core::utils::GenesisAccount;
use revm::{
    db::DatabaseRef,
    primitives::{AccountInfo, Address, Bytecode, EVMResult, Env, ResultAndState, B256, U256},
    Database, Inspector, JournaledState,
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
    fn snapshot(&mut self, journaled_state: &JournaledState, env: &Env) -> U256 {
        todo!()
    }

    fn revert(
        &mut self,
        id: U256,
        journaled_state: &JournaledState,
        env: &mut Env,
    ) -> Option<JournaledState> {
        todo!()
    }

    fn create_fork(&mut self, fork: CreateFork) -> eyre::Result<LocalForkId> {
        todo!()
    }

    fn create_fork_at_transaction(
        &mut self,
        fork: CreateFork,
        transaction: B256,
    ) -> eyre::Result<LocalForkId> {
        todo!()
    }

    fn select_fork(
        &mut self,
        id: LocalForkId,
        env: &mut Env,
        journaled_state: &mut JournaledState,
    ) -> eyre::Result<()> {
        todo!()
    }

    fn roll_fork(
        &mut self,
        id: Option<LocalForkId>,
        block_number: U256,
        env: &mut Env,
        journaled_state: &mut JournaledState,
    ) -> eyre::Result<()> {
        todo!()
    }

    fn roll_fork_to_transaction(
        &mut self,
        id: Option<LocalForkId>,
        transaction: B256,
        env: &mut Env,
        journaled_state: &mut JournaledState,
    ) -> eyre::Result<()> {
        todo!()
    }

    fn transact<I: Inspector<Backend>>(
        &mut self,
        id: Option<LocalForkId>,
        transaction: B256,
        env: &mut Env,
        journaled_state: &mut JournaledState,
        inspector: &mut I,
    ) -> eyre::Result<()> {
        todo!()
    }

    fn active_fork_id(&self) -> Option<LocalForkId> {
        todo!()
    }

    fn active_fork_url(&self) -> Option<String> {
        todo!()
    }

    fn ensure_fork(&self, id: Option<LocalForkId>) -> eyre::Result<LocalForkId> {
        todo!()
    }

    fn ensure_fork_id(&self, id: LocalForkId) -> eyre::Result<&ForkId> {
        todo!()
    }

    fn diagnose_revert(
        &self,
        callee: Address,
        journaled_state: &JournaledState,
    ) -> Option<RevertDiagnostic> {
        todo!()
    }

    fn load_allocs(
        &mut self,
        allocs: &HashMap<Address, GenesisAccount>,
        journaled_state: &mut JournaledState,
    ) -> Result<(), DatabaseError> {
        todo!()
    }

    fn is_persistent(&self, acc: &Address) -> bool {
        todo!()
    }

    fn remove_persistent_account(&mut self, account: &Address) -> bool {
        todo!()
    }

    #[doc = " Marks the given account as persistent."]
    fn add_persistent_account(&mut self, account: Address) -> bool {
        todo!()
    }

    fn allow_cheatcode_access(&mut self, account: Address) -> bool {
        todo!()
    }

    fn revoke_cheatcode_access(&mut self, account: Address) -> bool {
        todo!()
    }

    fn has_cheatcode_access(&self, account: Address) -> bool {
        todo!()
    }
}
