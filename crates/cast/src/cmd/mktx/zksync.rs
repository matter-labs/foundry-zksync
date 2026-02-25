use alloy_eips::Encodable2718;
use alloy_network::{AnyNetwork, TransactionBuilder};
use alloy_primitives::hex;
use alloy_provider::RootProvider;
use alloy_rpc_types::TransactionRequest;
use alloy_serde::WithOtherFields;
use alloy_signer::Signer;
use alloy_zksync::{
    network::transaction_request::TransactionRequest as ZkTransactionRequest,
    provider::ZksyncProvider,
    wallet::ZksyncWallet,
};
use eyre::Result;
use foundry_cli::{opts::EthereumOpts, utils};
use foundry_common::sh_println;
use foundry_config::Config;

use crate::{
    tx::{self, CastTxBuilder, InputState, SenderKind},
    zksync::{NoopWallet, ZkTransactionOpts},
};

/// Handle the full zksync mktx flow: signer resolution, tx building, signing, and output.
pub async fn run_zk_mktx(
    tx_builder: CastTxBuilder<&RootProvider<AnyNetwork>, InputState>,
    eth: &EthereumOpts,
    zk_tx: ZkTransactionOpts,
    zkcode: String,
    config: &Config,
) -> Result<()> {
    let (from, maybe_signer) = if zk_tx.custom_signature.is_some() {
        if let Some(from) = eth.wallet.from {
            (from, None)
        } else {
            eyre::bail!("expected address via --from option to be used for custom signature");
        }
    } else {
        let signer = eth.wallet.signer().await?;
        let from = signer.address();
        tx::validate_from_address(eth.wallet.from, from)?;
        (from, Some(signer))
    };

    let (tx, _) = if zk_tx.custom_signature.is_some() {
        tx_builder.build_raw(SenderKind::Address(from)).await?
    } else {
        tx_builder.build_raw(maybe_signer.as_ref().expect("No signer found")).await?
    };

    let zktx = build_tx(zk_tx, tx, zkcode, config).await?;

    let signed_tx = if zktx.custom_signature().is_some() {
        let zk_wallet = NoopWallet { address: from };
        zktx.build(&zk_wallet).await?.encoded_2718()
    } else {
        let zk_wallet = ZksyncWallet::new(maybe_signer.expect("No signer found"));
        zktx.build(&zk_wallet).await?.encoded_2718()
    };

    sh_println!("0x{}", hex::encode(signed_tx))?;

    Ok(())
}

/// Builds a complete ZkSync transaction request with fee estimation
async fn build_tx(
    zk_tx: ZkTransactionOpts,
    evm_tx: WithOtherFields<TransactionRequest>,
    zk_code: String,
    config: &Config,
) -> Result<ZkTransactionRequest> {
    let zk_provider = utils::get_provider_zksync(config)?;
    let mut tx = zk_tx.build_base_tx(evm_tx, Some(zk_code))?;

    let fee = ZksyncProvider::estimate_fee(&zk_provider, tx.clone()).await?;
    tx.set_max_fee_per_gas(fee.max_fee_per_gas);
    tx.set_max_priority_fee_per_gas(fee.max_priority_fee_per_gas);
    tx.set_gas_limit(fee.gas_limit);

    Ok(tx)
}
