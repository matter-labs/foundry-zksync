//! Contains zksync specific logic for foundry's `cast` functionality

use alloy_network::AnyNetwork;
use alloy_provider::{PendingTransactionBuilder, Provider};
use alloy_zksync::network::{
    transaction_request::TransactionRequest as ZkTransactionRequest, Zksync,
};
use eyre::Result;

use crate::Cast;

pub struct ZkCast<P, Z> {
    provider: Z,
    inner: Cast<P>,
}

impl<P, Z> AsRef<Cast<P>> for ZkCast<P, Z>
where
    P: Provider<AnyNetwork>,
    Z: Provider<Zksync>,
{
    fn as_ref(&self) -> &Cast<P> {
        &self.inner
    }
}

impl<P, Z> ZkCast<P, Z>
where
    P: Provider<AnyNetwork>,
    Z: Provider<Zksync>,
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
    pub fn new(provider: Z, cast: Cast<P>) -> Self {
        Self { provider, inner: cast }
    }

    pub async fn send_zk(
        &self,
        tx: ZkTransactionRequest,
    ) -> Result<PendingTransactionBuilder<Zksync>> {
        let res = self.provider.send_transaction(tx).await?;

        Ok(res)
    }
}
