mod build;
mod cmd;
mod config;
mod ext_integration;
mod inspect;
mod script;
mod verify;
mod verify_bytecode;

/// Maximum solc version supported by zksolc (era-solidity only supports up to 0.8.30).
/// Use with `--use` flag when running `--zksync` commands to ensure compatibility.
pub const ZK_MAX_SOLC: &str = "0.8.30";
