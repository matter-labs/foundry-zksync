use alloy_primitives::{Address, Bytes, hex};
use clap::Parser;

fn parse_hex_bytes(s: &str) -> Result<Bytes, String> {
    hex::decode(s).map(Bytes::from).map_err(|e| format!("Invalid hex string: {e}"))
}

#[derive(Clone, Debug, Default, Parser)]
#[command(next_help_heading = "ZKSync transaction options")]
pub struct ZkTransactionOpts {
    /// Paymaster address for the ZKSync transaction
    #[arg(long = "zk-paymaster-address", visible_alias = "paymaster-address", requires = "paymaster_input")]
    pub paymaster_address: Option<Address>,

    /// Paymaster input for the ZKSync transaction
    #[arg(long = "zk-paymaster-input", visible_alias = "paymaster-input", requires = "paymaster_address", value_parser = parse_hex_bytes)]
    pub paymaster_input: Option<Bytes>,

    /// Custom signature for the ZKSync transaction
    #[arg(long = "zk-custom-signature", value_parser = parse_hex_bytes)]
    pub custom_signature: Option<Bytes>,

    /// Factory dependencies for the ZKSync transaction
    #[arg(long = "zk-factory-deps", value_parser = parse_hex_bytes, value_delimiter = ',')]
    pub factory_deps: Vec<Bytes>,

    /// Gas per pubdata for the ZKSync transaction
    #[arg(long = "zk-gas-per-pubdata")]
    pub gas_per_pubdata: Option<u64>,
}

impl ZkTransactionOpts {
    pub fn has_zksync_args(&self) -> bool {
        self.paymaster_address.is_some()
            || self.custom_signature.is_some()
            || !self.factory_deps.is_empty()
            || self.gas_per_pubdata.is_some()
    }
}
