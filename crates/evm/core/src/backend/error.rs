use alloy_primitives::{Address, B256, U256};
use alloy_rpc_types::BlockId;
pub use foundry_fork_db::{DatabaseError, DatabaseResult};
use futures::channel::mpsc::SendError;
use revm::primitives::EVMError;
use std::{
    convert::Infallible,
    sync::{mpsc::RecvError, Arc},
};

pub type BackendResult<T> = Result<T, BackendError>;

/// Errors that can happen when working with [`revm::Database`]
#[derive(Debug, thiserror::Error)]
#[allow(missing_docs)]
pub enum BackendError {
    #[error("{0}")]
    Message(String),
    #[error("cheatcodes are not enabled for {0}; see `vm.allowCheatcodes(address)`")]
    NoCheats(Address),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error("failed to fetch account info for {0}")]
    MissingAccount(Address),
    #[error("missing bytecode for code hash {0}")]
    MissingCode(B256),
    #[error(transparent)]
    Recv(#[from] RecvError),
    #[error(transparent)]
    Send(#[from] SendError),
    #[error("failed to get account for {0}: {1}")]
    GetAccount(Address, Arc<eyre::Error>),
    #[error("failed to get storage for {0} at {1}: {2}")]
    GetStorage(Address, U256, Arc<eyre::Error>),
    #[error("failed to get block hash for {0}: {1}")]
    GetBlockHash(u64, Arc<eyre::Error>),
    #[error("failed to get full block for {0:?}: {1}")]
    GetFullBlock(BlockId, Arc<eyre::Error>),
    #[error("block {0:?} does not exist")]
    BlockNotFound(BlockId),
    #[error("failed to get transaction {0}: {1}")]
    GetTransaction(B256, Arc<eyre::Error>),
    #[error("transaction {0} not found")]
    TransactionNotFound(B256),
    #[error(
        "CREATE2 Deployer (0x4e59b44847b379578588920ca78fbf26c0b4956c) not present on this chain.\n\
         For a production environment, you can deploy it using the pre-signed transaction from \
         https://github.com/Arachnid/deterministic-deployment-proxy.\n\
         For a test environment, you can use `etch` to place the required bytecode at that address."
    )]
    MissingCreate2Deployer,
    #[error("failed to get bytecode for {0:?}: {1}")]
    GetBytecode(B256, Arc<eyre::Error>),
    #[error("{0}")]
    Other(String),
}

impl BackendError {
    /// Create a new error with a message
    pub fn msg(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }

    /// Create a new error with a message
    pub fn display(msg: impl std::fmt::Display) -> Self {
        Self::Message(msg.to_string())
    }
}

impl From<tokio::task::JoinError> for BackendError {
    fn from(value: tokio::task::JoinError) -> Self {
        Self::display(value)
    }
}

impl From<Infallible> for BackendError {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

// Note: this is mostly necessary to use some revm internals that return an [EVMError]
impl<T: Into<Self>> From<EVMError<T>> for BackendError {
    fn from(err: EVMError<T>) -> Self {
        match err {
            EVMError::Database(err) => err.into(),
            EVMError::Custom(err) => Self::msg(err),
            EVMError::Header(err) => Self::msg(err.to_string()),
            EVMError::Precompile(err) => Self::msg(err),
            EVMError::Transaction(err) => Self::msg(err.to_string()),
        }
    }
}
