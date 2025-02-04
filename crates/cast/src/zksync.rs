//! Contains zksync specific logic for foundry's `cast` functionality

use alloy_network::AnyNetwork;
use alloy_primitives::{Address, Bytes, U256};
use alloy_provider::{PendingTransactionBuilder, Provider};
use alloy_transport::Transport;
use alloy_zksync::network::{
    transaction_request::TransactionRequest as ZkTransactionRequest,
    unsigned_tx::eip712::PaymasterParams, Zksync,
};
use clap::Parser;
use eyre::Result;

use crate::Cast;

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

#[derive(Clone, Debug, Parser)]
#[command(next_help_heading = "Transaction options")]
pub struct ZkTransactionOpts {
    // /// Use ZKSync
    // #[arg(long, default_value_ifs([("paymaster_address", ArgPredicate::IsPresent,
    // "true"),("paymaster_input", ArgPredicate::IsPresent, "true")]))] pub zksync: bool,
    /// Paymaster address for the ZKSync transaction
    #[arg(long = "zk-paymaster-address", requires = "paymaster_input")]
    pub paymaster_address: Option<Address>,

    /// Paymaster input for the ZKSync transaction
    #[arg(long = "zk-paymaster-input", requires = "paymaster_address")]
    pub paymaster_input: Option<Bytes>,

    /// Factory dependencies for the ZKSync transaction
    #[arg(long = "zk-factory-deps")]
    pub factory_deps: Vec<Bytes>,

    /// Custom signature for the ZKSync transaction
    #[arg(long = "zk-custom-signature")]
    pub custom_signature: Option<Bytes>,

    /// Gas per pubdata for the ZKSync transaction
    #[arg(long = "zk-gas-per-pubdata")]
    pub gas_per_pubdata: Option<U256>,
}

impl ZkTransactionOpts {
    pub fn has_zksync_args(&self) -> bool {
        self.paymaster_address.is_some() ||
            !self.factory_deps.is_empty() ||
            self.custom_signature.is_some() ||
            self.gas_per_pubdata.is_some()
    }

    pub fn apply_to_tx(&self, tx: &mut ZkTransactionRequest) {
        if let Some(gas_per_pubdata) = self.gas_per_pubdata {
            tx.set_gas_per_pubdata(gas_per_pubdata)
        }

        if !self.factory_deps.is_empty() {
            tx.set_factory_deps(self.factory_deps.clone());
        }

        if let Some(custom_signature) = &self.custom_signature {
            tx.set_custom_signature(custom_signature.clone());
        }

        if let (Some(paymaster), Some(paymaster_input)) =
            (self.paymaster_address, self.paymaster_input.clone())
        {
            tx.set_paymaster_params(PaymasterParams { paymaster, paymaster_input });
        }
    }
}
