#![cfg(test)]

use revm::primitives::{AccountInfo, Address, Bytecode, B256, U256};
use std::collections::HashMap;

#[derive(Default)]
pub struct MockDatabase {
    pub basic: HashMap<Address, AccountInfo>,
}

impl revm::Database for MockDatabase {
    type Error = String;

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
