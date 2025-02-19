use alloy_network::{AnyNetwork, TransactionBuilder};
use alloy_primitives::hex;
use alloy_provider::Provider;
use alloy_rpc_types::TransactionRequest;
use alloy_serde::WithOtherFields;
use alloy_transport::Transport;
use alloy_zksync::{
    network::{
        transaction_request::TransactionRequest as ZkTransactionRequest,
        unsigned_tx::eip712::PaymasterParams,
    },
    provider::ZksyncProvider,
};
use cast::{Cast, ZkCast, ZkTransactionOpts};
use eyre::Result;

#[allow(clippy::too_many_arguments)]
pub async fn send_zk_transaction<P, Z, T>(
    provider: P,
    zk_provider: Z,
    tx: WithOtherFields<TransactionRequest>,
    zk_tx_opts: ZkTransactionOpts,
    zk_code: Option<String>,
    cast_async: bool,
    confs: u64,
    timeout: u64,
) -> Result<()>
where
    P: Provider<T, AnyNetwork>,
    Z: ZksyncProvider<T>,
    T: Transport + Clone,
{
    let mut tx = prepare_zk_transaction(tx, zk_tx_opts, zk_code)?;

    // Estimate fees
    foundry_zksync_core::estimate_fee(&mut tx, &zk_provider, 130, None).await?;

    let cast = ZkCast::new(zk_provider, Cast::new(provider));
    let pending_tx = cast.send_zk(tx).await?;
    let tx_hash = pending_tx.inner().tx_hash();

    if cast_async {
        sh_println!("{tx_hash:#x}")?;
    } else {
        let receipt = cast
            .as_ref()
            .receipt(format!("{tx_hash:#x}"), None, confs, Some(timeout), false)
            .await?;
        sh_println!("{receipt}")?;
    }

    Ok(())
}

fn prepare_zk_transaction(
    mut tx: WithOtherFields<TransactionRequest>,
    zk_tx_opts: ZkTransactionOpts,
    zk_code: Option<String>,
) -> Result<ZkTransactionRequest> {
    use alloy_primitives::TxKind;

    let is_create = tx.to == Some(TxKind::Create);
    let paymaster_params = zk_tx_opts
        .paymaster_address
        .and_then(|addr| zk_tx_opts.paymaster_input.map(|input| (addr, input)))
        .map(|(addr, input)| PaymasterParams { paymaster: addr, paymaster_input: input });

    tx.inner.transaction_type = Some(zksync_types::l2::TransactionType::EIP712Transaction as u8);

    let mut zk_tx: ZkTransactionRequest = tx.inner.into();

    if is_create {
        let input_data = zk_tx.input().unwrap_or_default().to_vec();
        let zk_code =
            zk_code.ok_or_else(|| eyre::eyre!("ZkSync code is required for contract creation"))?;
        let zk_code_bytes = hex::decode(zk_code)?;
        let constructor_args = &input_data[zk_code_bytes.len()..];

        zk_tx = zk_tx.with_create_params(
            zk_code_bytes,
            constructor_args.to_vec(),
            zk_tx_opts.factory_deps.iter().map(|b| b.to_vec()).collect(),
        )?;
    } else {
        zk_tx.set_factory_deps(zk_tx_opts.factory_deps);
    }

    if let Some(paymaster_params) = paymaster_params {
        zk_tx.set_paymaster_params(paymaster_params);
    }

    Ok(zk_tx)
}
