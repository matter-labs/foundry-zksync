//! Gas related tests

use crate::utils::ethers_http_provider;
use alloy_primitives::U256;
use anvil::{eth::fees::INITIAL_BASE_FEE, spawn, NodeConfig};
use ethers::{
    prelude::Middleware,
    types::{
        transaction::eip2718::TypedTransaction, Address, BlockNumber, Eip1559TransactionRequest,
        TransactionRequest,
    },
};
use foundry_common::types::ToAlloy;

const GAS_TRANSFER: u64 = 21_000u64;

#[tokio::test(flavor = "multi_thread")]
async fn test_basefee_full_block() {
    let (_api, handle) = spawn(
        NodeConfig::test()
            .with_base_fee(Some(INITIAL_BASE_FEE.to_alloy()))
            .with_gas_limit(Some(GAS_TRANSFER.to_alloy())),
    )
    .await;
    let provider = ethers_http_provider(&handle.http_endpoint());
    let tx = TransactionRequest::new().to(Address::random()).value(1337u64);
    provider.send_transaction(tx.clone(), None).await.unwrap().await.unwrap().unwrap();
    let base_fee =
        provider.get_block(BlockNumber::Latest).await.unwrap().unwrap().base_fee_per_gas.unwrap();
    let tx = TransactionRequest::new().to(Address::random()).value(1337u64);
    provider.send_transaction(tx.clone(), None).await.unwrap().await.unwrap().unwrap();
    let next_base_fee =
        provider.get_block(BlockNumber::Latest).await.unwrap().unwrap().base_fee_per_gas.unwrap();

    assert!(next_base_fee > base_fee);
    // max increase, full block
    assert_eq!(next_base_fee.as_u64(), INITIAL_BASE_FEE + 125_000_000);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_basefee_half_block() {
    let (_api, handle) = spawn(
        NodeConfig::test()
            .with_base_fee(Some(INITIAL_BASE_FEE.to_alloy()))
            .with_gas_limit(Some(GAS_TRANSFER.to_alloy() * U256::from(2))),
    )
    .await;
    let provider = ethers_http_provider(&handle.http_endpoint());
    let tx = TransactionRequest::new().to(Address::random()).value(1337u64);
    provider.send_transaction(tx.clone(), None).await.unwrap().await.unwrap().unwrap();
    let tx = TransactionRequest::new().to(Address::random()).value(1337u64);
    provider.send_transaction(tx.clone(), None).await.unwrap().await.unwrap().unwrap();
    let next_base_fee =
        provider.get_block(BlockNumber::Latest).await.unwrap().unwrap().base_fee_per_gas.unwrap();

    // unchanged, half block
    assert_eq!(next_base_fee.as_u64(), INITIAL_BASE_FEE);
}
#[tokio::test(flavor = "multi_thread")]
async fn test_basefee_empty_block() {
    let (api, handle) =
        spawn(NodeConfig::test().with_base_fee(Some(INITIAL_BASE_FEE.to_alloy()))).await;

    let provider = ethers_http_provider(&handle.http_endpoint());
    let tx = TransactionRequest::new().to(Address::random()).value(1337u64);
    provider.send_transaction(tx, None).await.unwrap().await.unwrap().unwrap();
    let base_fee =
        provider.get_block(BlockNumber::Latest).await.unwrap().unwrap().base_fee_per_gas.unwrap();

    // mine empty block
    api.mine_one().await;

    let next_base_fee =
        provider.get_block(BlockNumber::Latest).await.unwrap().unwrap().base_fee_per_gas.unwrap();

    // empty block, decreased base fee
    assert!(next_base_fee < base_fee);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_respect_base_fee() {
    let base_fee = 50u64;
    let (_api, handle) = spawn(NodeConfig::test().with_base_fee(Some(base_fee.to_alloy()))).await;
    let provider = ethers_http_provider(&handle.http_endpoint());
    let mut tx = TypedTransaction::default();
    tx.set_value(100u64);
    tx.set_to(Address::random());

    let mut underpriced = tx.clone();
    underpriced.set_gas_price(base_fee - 1);
    let res = provider.send_transaction(underpriced, None).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("max fee per gas less than block base fee"));

    tx.set_gas_price(base_fee);
    let tx = provider.send_transaction(tx, None).await.unwrap().await.unwrap().unwrap();
    assert_eq!(tx.status, Some(1u64.into()));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tip_above_fee_cap() {
    let base_fee = 50u64;
    let (_api, handle) = spawn(NodeConfig::test().with_base_fee(Some(base_fee.to_alloy()))).await;
    let provider = ethers_http_provider(&handle.http_endpoint());
    let tx = TypedTransaction::Eip1559(
        Eip1559TransactionRequest::new()
            .max_fee_per_gas(base_fee)
            .max_priority_fee_per_gas(base_fee + 1)
            .to(Address::random())
            .value(100u64),
    );
    let res = provider.send_transaction(tx, None).await;
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("max priority fee per gas higher than max fee per gas"));
}
