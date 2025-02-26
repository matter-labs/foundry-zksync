use std::sync::Arc;

use alloy_network::AnyNetwork;
use alloy_primitives::FixedBytes;
use alloy_provider::{PendingTransactionBuilder, ProviderBuilder, RootProvider};
use alloy_rpc_types::TransactionRequest;
use alloy_serde::WithOtherFields;
use alloy_signer::Signer;
use alloy_zksync::{network::Zksync, provider::ZksyncProvider, wallet::ZksyncWallet};
use cast::{NoopWallet, ZkTransactionOpts};
use eyre::Result;
use foundry_cli::opts::EthereumOpts;

use crate::tx::{self, CastTxBuilder, InputState, SenderKind};

pub async fn send_zk_transaction(
    zk_provider: RootProvider<Zksync>,
    builder: CastTxBuilder<&RootProvider<AnyNetwork>, InputState>,
    eth_opts: &EthereumOpts,
    zk_tx_opts: ZkTransactionOpts,
    zk_code: Option<String>,
) -> Result<FixedBytes<32>> {
    if zk_tx_opts.custom_signature.is_none() {
        let signer = eth_opts.wallet.signer().await?;
        let from = signer.address();
        tx::validate_from_address(eth_opts.wallet.from, from)?;

        let (tx, _) = builder.build_raw(&signer).await?;
        let signer = Arc::new(signer);

        let zk_wallet = ZksyncWallet::from(signer.clone());
        let zk_provider = ProviderBuilder::<_, _, Zksync>::default()
            .wallet(zk_wallet.clone())
            .on_provider(&zk_provider);
        send_transaction_internal(zk_provider, tx, zk_tx_opts, zk_code).await
    } else if let Some(from) = eth_opts.wallet.from {
        let (tx, _) = builder.build_raw(SenderKind::Address(from)).await?;

        let zk_wallet = NoopWallet { address: from };
        let zk_provider = ProviderBuilder::<_, _, Zksync>::default()
            .wallet(zk_wallet.clone())
            .on_provider(&zk_provider);
        send_transaction_internal(zk_provider, tx, zk_tx_opts, zk_code).await
    } else {
        eyre::bail!("expected address via --from option to be used for custom signature");
    }
}

async fn send_transaction_internal<Z>(
    zk_provider: Z,
    tx: WithOtherFields<TransactionRequest>,
    zk_tx_opts: ZkTransactionOpts,
    zk_code: Option<String>,
) -> Result<FixedBytes<32>>
where
    Z: ZksyncProvider,
{
    let mut tx = zk_tx_opts.build_base_tx(tx, zk_code)?;

    // Estimate fees
    foundry_zksync_core::estimate_fee(&mut tx, &zk_provider, 130, None).await?;

    let pending_tx: PendingTransactionBuilder<Zksync> = zk_provider.send_transaction(tx).await?;
    let tx_hash = pending_tx.inner().tx_hash();

    Ok(*tx_hash)
}
