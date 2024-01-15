use alloy_primitives::B256;
use ethers_providers::Middleware;
use revm::primitives::Bytecode;

#[async_trait::async_trait]
pub trait ZkSyncMiddleware: Middleware {
    async fn get_bytecode_by_hash(&self, hash: B256) -> Result<Option<Bytecode>, Self::Error>;
}
