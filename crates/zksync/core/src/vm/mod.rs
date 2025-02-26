mod db;
mod env;
mod farcall;
mod inspect;
mod runner;
mod storage_recorder;
mod storage_view;
mod tracers;

use alloy_primitives::{address, Address};
pub use env::ZkEnv;
pub use farcall::{SELECTOR_CONTRACT_DEPLOYER_CREATE, SELECTOR_CONTRACT_DEPLOYER_CREATE2};
pub use inspect::{
    batch_factory_dependencies, inspect, inspect_as_batch, ZKVMExecutionResult, ZKVMResult,
};
pub use runner::{
    balance, call, code_hash, create, deploy_nonce, encode_create_params, transact, tx_nonce,
    ZkCreateInputs,
};
pub use storage_recorder::{AccountAccess, AccountAccessKind, StorageAccess};
pub use tracers::cheatcode::CheatcodeTracerContext;

/// The Hardhat console address.
///
/// See: <https://github.com/nomiclabs/hardhat/blob/master/packages/hardhat-core/console.sol>
pub const HARDHAT_CONSOLE_ADDRESS: Address = address!("000000000000000000636F6e736F6c652e6c6f67");
