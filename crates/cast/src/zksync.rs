//! Contains zksync specific logic for foundry's `cast` functionality

use alloy_network::{AnyNetwork, TransactionBuilder};
use alloy_primitives::{hex, Address, Bytes, TxKind, U256};
use alloy_provider::{PendingTransactionBuilder, Provider};
use alloy_rpc_types::TransactionRequest;
use alloy_serde::WithOtherFields;
use alloy_transport::Transport;
use alloy_zksync::network::{
    transaction_request::TransactionRequest as ZkTransactionRequest,
    unsigned_tx::eip712::PaymasterParams, Zksync,
};
use clap::{command, Parser};
use eyre::Result;

use crate::Cast;

#[derive(Clone, Debug, Parser)]
#[command(next_help_heading = "Transaction options")]
pub struct ZkTransactionOpts {
    /// Paymaster address for the ZKSync transaction
    #[arg(long = "zk-paymaster-address", requires = "paymaster_input")]
    pub paymaster_address: Option<Address>,

    /// Paymaster input for the ZKSync transaction
    #[arg(long = "zk-paymaster-input", requires = "paymaster_address", value_parser = parse_hex_bytes)]
    pub paymaster_input: Option<Bytes>,

    /// Factory dependencies for the ZKSync transaction
    #[arg(long = "zk-factory-deps", value_parser = parse_hex_bytes, value_delimiter = ',')]
    pub factory_deps: Vec<Bytes>,

    /// Gas per pubdata for the ZKSync transaction
    #[arg(long = "zk-gas-per-pubdata")]
    pub gas_per_pubdata: Option<U256>,
}

fn parse_hex_bytes(s: &str) -> Result<Bytes, String> {
    hex::decode(s).map(Bytes::from).map_err(|e| format!("Invalid hex string: {e}"))
}

impl ZkTransactionOpts {
    pub fn has_zksync_args(&self) -> bool {
        self.paymaster_address.is_some() ||
            !self.factory_deps.is_empty() ||
            self.gas_per_pubdata.is_some()
    }

    /// Builds a base ZkSync transaction request from the common parameters
    pub fn build_base_tx(
        &self,
        evm_tx: WithOtherFields<TransactionRequest>,
        zk_code: Option<String>,
    ) -> Result<ZkTransactionRequest> {
        let is_create = evm_tx.to == Some(TxKind::Create);
        let mut tx: ZkTransactionRequest = evm_tx.inner.into();

        if let Some(gas_per_pubdata) = self.gas_per_pubdata {
            tx.set_gas_per_pubdata(gas_per_pubdata);
        }

        if let (Some(paymaster), Some(paymaster_input)) =
            (self.paymaster_address, self.paymaster_input.clone())
        {
            tx.set_paymaster_params(PaymasterParams { paymaster, paymaster_input });
        }

        if is_create {
            let input_data = tx.input().cloned().unwrap_or_default().to_vec();
            let zk_code = zk_code
                .ok_or_else(|| eyre::eyre!("ZkSync code is required for contract creation"))?;
            let zk_code_bytes = hex::decode(zk_code)?;
            let constructor_args = &input_data[zk_code_bytes.len()..];

            tx = tx.with_create_params(
                zk_code_bytes,
                constructor_args.to_vec(),
                self.factory_deps.iter().map(|b| b.to_vec()).collect(),
            )?;
        } else {
            tx.set_factory_deps(self.factory_deps.clone());
        }

        tx.prep_for_submission();
        Ok(tx)
    }
}

pub struct ZkCast<P, T, Z> {
    provider: Z,
    inner: Cast<P, T>,
}

impl<P, T, Z> AsRef<Cast<P, T>> for ZkCast<P, T, Z>
where
    P: Provider<T, AnyNetwork>,
    T: Transport + Clone,
    Z: Provider<T, Zksync>,
{
    fn as_ref(&self) -> &Cast<P, T> {
        &self.inner
    }
}

impl<P, T, Z> ZkCast<P, T, Z>
where
    P: Provider<T, AnyNetwork>,
    T: Transport + Clone,
    Z: Provider<T, Zksync>,
{
    /// Creates a new ZkCast instance from the provided client and Cast instance
    ///
    /// # Example
    ///
    /// ```
    /// use alloy_provider::{network::AnyNetwork, ProviderBuilder, RootProvider};
    /// use cast::Cast;
    ///
    /// # async fn foo() -> eyre::Result<()> {
    /// let provider =
    ///     ProviderBuilder::<_, _, AnyNetwork>::default().on_builtin("http://localhost:8545").await?;
    /// let cast = Cast::new(provider);
    /// let zk_provider =
    ///     ProviderBuilder::<_, _, Zksync>::default().on_builtin("http://localhost:8011").await?;
    /// let zk_cast = ZkCast::new(provider, cast);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(provider: Z, cast: Cast<P, T>) -> Self {
        Self { provider, inner: cast }
    }

    pub async fn send_zk(
        &self,
        tx: ZkTransactionRequest,
    ) -> Result<PendingTransactionBuilder<T, Zksync>> {
        let res = self.provider.send_transaction(tx).await?;

        Ok(res)
    }
}
