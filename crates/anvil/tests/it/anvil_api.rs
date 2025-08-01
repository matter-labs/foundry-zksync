//! tests for custom anvil endpoints

use crate::{
    abi::{self, BUSD, Greeter, Multicall},
    fork::fork_config,
    utils::http_provider_with_signer,
};
use alloy_consensus::{SignableTransaction, TxEip1559};
use alloy_hardforks::EthereumHardfork;
use alloy_network::{EthereumWallet, TransactionBuilder, TxSignerSync};
use alloy_primitives::{Address, Bytes, TxKind, U256, address, fixed_bytes, utils::Unit};
use alloy_provider::{Provider, ext::TxPoolApi};
use alloy_rpc_types::{
    BlockId, BlockNumberOrTag, TransactionRequest,
    anvil::{
        ForkedNetwork, Forking, Metadata, MineOptions, NodeEnvironment, NodeForkConfig, NodeInfo,
    },
};
use alloy_serde::WithOtherFields;
use anvil::{
    NodeConfig,
    eth::{
        api::CLIENT_VERSION,
        backend::mem::{EXECUTOR, P256_DELEGATION_CONTRACT, P256_DELEGATION_RUNTIME_CODE},
    },
    spawn,
};
use anvil_core::{
    eth::{
        EthRequest,
        wallet::{Capabilities, DelegationCapability, WalletCapabilities},
    },
    types::{ReorgOptions, TransactionData},
};
use revm::primitives::hardfork::SpecId;
use std::{
    str::FromStr,
    time::{Duration, SystemTime},
};

#[tokio::test(flavor = "multi_thread")]
async fn can_set_gas_price() {
    let (api, handle) =
        spawn(NodeConfig::test().with_hardfork(Some(EthereumHardfork::Berlin.into()))).await;
    let provider = handle.http_provider();

    let gas_price = U256::from(1337);
    api.anvil_set_min_gas_price(gas_price).await.unwrap();
    assert_eq!(gas_price.to::<u128>(), provider.get_gas_price().await.unwrap());
}

#[tokio::test(flavor = "multi_thread")]
async fn can_set_block_gas_limit() {
    let (api, _) =
        spawn(NodeConfig::test().with_hardfork(Some(EthereumHardfork::Berlin.into()))).await;

    let block_gas_limit = U256::from(1337);
    assert!(api.evm_set_block_gas_limit(block_gas_limit).unwrap());
    // Mine a new block, and check the new block gas limit
    api.mine_one().await;
    let latest_block = api.block_by_number(BlockNumberOrTag::Latest).await.unwrap().unwrap();
    assert_eq!(block_gas_limit.to::<u64>(), latest_block.header.gas_limit);
}

// Ref <https://github.com/foundry-rs/foundry/issues/2341>
#[tokio::test(flavor = "multi_thread")]
async fn can_set_storage() {
    let (api, _handle) = spawn(NodeConfig::test()).await;
    let s = r#"{"jsonrpc": "2.0", "method": "hardhat_setStorageAt", "id": 1, "params": ["0xe9e7CEA3DedcA5984780Bafc599bD69ADd087D56", "0xa6eef7e35abe7026729641147f7915573c7e97b47efa546f5f6e3230263bcb49", "0x0000000000000000000000000000000000000000000000000000000000003039"]}"#;
    let req = serde_json::from_str::<EthRequest>(s).unwrap();
    let (addr, slot, val) = match req.clone() {
        EthRequest::SetStorageAt(addr, slot, val) => (addr, slot, val),
        _ => unreachable!(),
    };

    api.execute(req).await;

    let storage_value = api.storage_at(addr, slot, None).await.unwrap();
    assert_eq!(val, storage_value);
}

#[tokio::test(flavor = "multi_thread")]
async fn can_impersonate_account() {
    let (api, handle) = spawn(NodeConfig::test()).await;

    let provider = handle.http_provider();

    let impersonate = Address::random();
    let to = Address::random();
    let val = U256::from(1337);
    let funding = U256::from(1e18 as u64);
    // fund the impersonated account
    api.anvil_set_balance(impersonate, funding).await.unwrap();

    let balance = api.balance(impersonate, None).await.unwrap();
    assert_eq!(balance, funding);

    let tx = TransactionRequest::default().with_from(impersonate).with_to(to).with_value(val);
    let tx = WithOtherFields::new(tx);

    let res = provider.send_transaction(tx.clone()).await;
    res.unwrap_err();

    api.anvil_impersonate_account(impersonate).await.unwrap();
    assert!(api.accounts().unwrap().contains(&impersonate));

    let res = provider.send_transaction(tx.clone()).await.unwrap().get_receipt().await.unwrap();
    assert_eq!(res.from, impersonate);

    let nonce = provider.get_transaction_count(impersonate).await.unwrap();
    assert_eq!(nonce, 1);

    let balance = provider.get_balance(to).await.unwrap();
    assert_eq!(balance, val);

    api.anvil_stop_impersonating_account(impersonate).await.unwrap();
    let res = provider.send_transaction(tx).await;
    res.unwrap_err();
}

#[tokio::test(flavor = "multi_thread")]
async fn can_auto_impersonate_account() {
    let (api, handle) = spawn(NodeConfig::test()).await;

    let provider = handle.http_provider();

    let impersonate = Address::random();
    let to = Address::random();
    let val = U256::from(1337);
    let funding = U256::from(1e18 as u64);
    // fund the impersonated account
    api.anvil_set_balance(impersonate, funding).await.unwrap();

    let balance = api.balance(impersonate, None).await.unwrap();
    assert_eq!(balance, funding);

    let tx = TransactionRequest::default().with_from(impersonate).with_to(to).with_value(val);
    let tx = WithOtherFields::new(tx);

    let res = provider.send_transaction(tx.clone()).await;
    res.unwrap_err();

    api.anvil_auto_impersonate_account(true).await.unwrap();

    let res = provider.send_transaction(tx.clone()).await.unwrap().get_receipt().await.unwrap();
    assert_eq!(res.from, impersonate);

    let nonce = provider.get_transaction_count(impersonate).await.unwrap();
    assert_eq!(nonce, 1);

    let balance = provider.get_balance(to).await.unwrap();
    assert_eq!(balance, val);

    api.anvil_auto_impersonate_account(false).await.unwrap();
    let res = provider.send_transaction(tx).await;
    res.unwrap_err();

    // explicitly impersonated accounts get returned by `eth_accounts`
    api.anvil_impersonate_account(impersonate).await.unwrap();
    assert!(api.accounts().unwrap().contains(&impersonate));
}

#[tokio::test(flavor = "multi_thread")]
async fn can_impersonate_contract() {
    let (api, handle) = spawn(NodeConfig::test()).await;

    let provider = handle.http_provider();

    let greeter_contract = Greeter::deploy(&provider, "Hello World!".to_string()).await.unwrap();
    let impersonate = greeter_contract.address().to_owned();

    let to = Address::random();
    let val = U256::from(1337);

    // // fund the impersonated account
    api.anvil_set_balance(impersonate, U256::from(1e18 as u64)).await.unwrap();

    let tx = TransactionRequest::default().with_from(impersonate).to(to).with_value(val);
    let tx = WithOtherFields::new(tx);

    let res = provider.send_transaction(tx.clone()).await;
    res.unwrap_err();

    let greeting = greeter_contract.greet().call().await.unwrap();
    assert_eq!("Hello World!", greeting);

    api.anvil_impersonate_account(impersonate).await.unwrap();

    let res = provider.send_transaction(tx.clone()).await.unwrap().get_receipt().await.unwrap();
    assert_eq!(res.from, impersonate);

    let balance = provider.get_balance(to).await.unwrap();
    assert_eq!(balance, val);

    api.anvil_stop_impersonating_account(impersonate).await.unwrap();
    let res = provider.send_transaction(tx).await;
    res.unwrap_err();

    let greeting = greeter_contract.greet().call().await.unwrap();
    assert_eq!("Hello World!", greeting);
}

#[tokio::test(flavor = "multi_thread")]
async fn can_impersonate_gnosis_safe() {
    let (api, handle) = spawn(fork_config()).await;
    let provider = handle.http_provider();

    // <https://help.safe.global/en/articles/40824-i-don-t-remember-my-safe-address-where-can-i-find-it>
    let safe = address!("0xA063Cb7CFd8E57c30c788A0572CBbf2129ae56B6");

    let code = provider.get_code_at(safe).await.unwrap();
    assert!(!code.is_empty());

    api.anvil_impersonate_account(safe).await.unwrap();

    let code = provider.get_code_at(safe).await.unwrap();
    assert!(!code.is_empty());

    let balance = U256::from(1e18 as u64);
    // fund the impersonated account
    api.anvil_set_balance(safe, balance).await.unwrap();

    let on_chain_balance = provider.get_balance(safe).await.unwrap();
    assert_eq!(on_chain_balance, balance);

    api.anvil_stop_impersonating_account(safe).await.unwrap();

    let code = provider.get_code_at(safe).await.unwrap();
    // code is added back after stop impersonating
    assert!(!code.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn can_impersonate_multiple_accounts() {
    let (api, handle) = spawn(NodeConfig::test()).await;
    let provider = handle.http_provider();

    let impersonate0 = Address::random();
    let impersonate1 = Address::random();
    let to = Address::random();

    let val = U256::from(1337);
    let funding = U256::from(1e18 as u64);
    // fund the impersonated accounts
    api.anvil_set_balance(impersonate0, funding).await.unwrap();
    api.anvil_set_balance(impersonate1, funding).await.unwrap();

    let tx = TransactionRequest::default().with_from(impersonate0).to(to).with_value(val);
    let tx = WithOtherFields::new(tx);

    api.anvil_impersonate_account(impersonate0).await.unwrap();
    api.anvil_impersonate_account(impersonate1).await.unwrap();

    let res0 = provider.send_transaction(tx.clone()).await.unwrap().get_receipt().await.unwrap();
    assert_eq!(res0.from, impersonate0);

    let nonce = provider.get_transaction_count(impersonate0).await.unwrap();
    assert_eq!(nonce, 1);

    let receipt = provider.get_transaction_receipt(res0.transaction_hash).await.unwrap().unwrap();
    assert_eq!(res0.inner, receipt.inner);

    let res1 = provider
        .send_transaction(tx.with_from(impersonate1))
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();

    assert_eq!(res1.from, impersonate1);

    let nonce = provider.get_transaction_count(impersonate1).await.unwrap();
    assert_eq!(nonce, 1);

    let receipt = provider.get_transaction_receipt(res1.transaction_hash).await.unwrap().unwrap();
    assert_eq!(res1.inner, receipt.inner);

    assert_ne!(res0.inner, res1.inner);
}

#[tokio::test(flavor = "multi_thread")]
async fn can_mine_manually() {
    let (api, handle) = spawn(NodeConfig::test()).await;
    let provider = handle.http_provider();

    let start_num = provider.get_block_number().await.unwrap();

    for (idx, _) in std::iter::repeat_n((), 10).enumerate() {
        api.evm_mine(None).await.unwrap();
        let num = provider.get_block_number().await.unwrap();
        assert_eq!(num, start_num + idx as u64 + 1);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_set_next_timestamp() {
    let (api, handle) = spawn(NodeConfig::test()).await;
    let provider = handle.http_provider();

    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();

    let next_timestamp = now + Duration::from_secs(60);

    // mock timestamp
    api.evm_set_next_block_timestamp(next_timestamp.as_secs()).unwrap();

    api.evm_mine(None).await.unwrap();

    let block = provider.get_block(BlockId::default()).await.unwrap().unwrap();

    assert_eq!(block.header.number, 1);
    assert_eq!(block.header.timestamp, next_timestamp.as_secs());

    api.evm_mine(None).await.unwrap();

    let next = provider.get_block(BlockId::default()).await.unwrap().unwrap();
    assert_eq!(next.header.number, 2);

    assert!(next.header.timestamp >= block.header.timestamp);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_evm_set_time() {
    let (api, handle) = spawn(NodeConfig::test()).await;
    let provider = handle.http_provider();

    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();

    let timestamp = now + Duration::from_secs(120);

    // mock timestamp
    api.evm_set_time(timestamp.as_secs()).unwrap();

    // mine a block
    api.evm_mine(None).await.unwrap();
    let block = provider.get_block(BlockId::default()).await.unwrap().unwrap();

    assert!(block.header.timestamp >= timestamp.as_secs());

    api.evm_mine(None).await.unwrap();
    let next = provider.get_block(BlockId::default()).await.unwrap().unwrap();

    assert!(next.header.timestamp >= block.header.timestamp);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_evm_set_time_in_past() {
    let (api, handle) = spawn(NodeConfig::test()).await;
    let provider = handle.http_provider();

    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();

    let timestamp = now - Duration::from_secs(120);

    // mock timestamp
    api.evm_set_time(timestamp.as_secs()).unwrap();

    // mine a block
    api.evm_mine(None).await.unwrap();
    let block = provider.get_block(BlockId::default()).await.unwrap().unwrap();

    assert!(block.header.timestamp >= timestamp.as_secs());
    assert!(block.header.timestamp < now.as_secs());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_timestamp_interval() {
    let (api, handle) = spawn(NodeConfig::test()).await;
    let provider = handle.http_provider();

    api.evm_mine(None).await.unwrap();
    let interval = 10;

    for _ in 0..5 {
        let block = provider.get_block(BlockId::default()).await.unwrap().unwrap();

        // mock timestamp
        api.evm_set_block_timestamp_interval(interval).unwrap();
        api.evm_mine(None).await.unwrap();

        let new_block = provider.get_block(BlockId::default()).await.unwrap().unwrap();

        assert_eq!(new_block.header.timestamp, block.header.timestamp + interval);
    }

    let block = provider.get_block(BlockId::default()).await.unwrap().unwrap();

    let next_timestamp = block.header.timestamp + 50;
    api.evm_set_next_block_timestamp(next_timestamp).unwrap();

    api.evm_mine(None).await.unwrap();
    let block = provider.get_block(BlockId::default()).await.unwrap().unwrap();
    assert_eq!(block.header.timestamp, next_timestamp);

    api.evm_mine(None).await.unwrap();

    let block = provider.get_block(BlockId::default()).await.unwrap().unwrap();
    // interval also works after setting the next timestamp manually
    assert_eq!(block.header.timestamp, next_timestamp + interval);

    assert!(api.evm_remove_block_timestamp_interval().unwrap());

    api.evm_mine(None).await.unwrap();
    let new_block = provider.get_block(BlockId::default()).await.unwrap().unwrap();

    // offset is applied correctly after resetting the interval
    assert!(new_block.header.timestamp > block.header.timestamp);

    api.evm_mine(None).await.unwrap();
    let another_block = provider.get_block(BlockId::default()).await.unwrap().unwrap();
    // check interval is disabled
    assert!(another_block.header.timestamp - new_block.header.timestamp < interval);
}

// <https://github.com/foundry-rs/foundry/issues/2341>
#[tokio::test(flavor = "multi_thread")]
async fn test_can_set_storage_bsc_fork() {
    let (api, handle) =
        spawn(NodeConfig::test().with_eth_rpc_url(Some("https://bsc-dataseed.binance.org/"))).await;
    let provider = handle.http_provider();

    let busd_addr = address!("0xe9e7CEA3DedcA5984780Bafc599bD69ADd087D56");
    let idx = U256::from_str("0xa6eef7e35abe7026729641147f7915573c7e97b47efa546f5f6e3230263bcb49")
        .unwrap();
    let value = fixed_bytes!("0000000000000000000000000000000000000000000000000000000000003039");

    api.anvil_set_storage_at(busd_addr, idx, value).await.unwrap();
    let storage = api.storage_at(busd_addr, idx, None).await.unwrap();
    assert_eq!(storage, value);

    let busd_contract = BUSD::new(busd_addr, &provider);

    let balance = busd_contract
        .balanceOf(address!("0x0000000000000000000000000000000000000000"))
        .call()
        .await
        .unwrap();
    assert_eq!(balance, U256::from(12345u64));
}

#[tokio::test(flavor = "multi_thread")]
async fn can_get_node_info() {
    let (api, handle) = spawn(NodeConfig::test()).await;

    let node_info = api.anvil_node_info().await.unwrap();

    let provider = handle.http_provider();

    let block_number = provider.get_block_number().await.unwrap();
    let block = provider.get_block(BlockId::from(block_number)).await.unwrap().unwrap();
    let hard_fork: &str = SpecId::PRAGUE.into();

    let expected_node_info = NodeInfo {
        current_block_number: 0_u64,
        current_block_timestamp: 1,
        current_block_hash: block.header.hash,
        hard_fork: hard_fork.to_string(),
        transaction_order: "fees".to_owned(),
        environment: NodeEnvironment {
            base_fee: U256::from_str("0x3b9aca00").unwrap().to(),
            chain_id: 0x7a69,
            gas_limit: U256::from_str("0x1c9c380").unwrap().to(),
            gas_price: U256::from_str("0x77359400").unwrap().to(),
        },
        fork_config: NodeForkConfig {
            fork_url: None,
            fork_block_number: None,
            fork_retry_backoff: None,
        },
    };

    assert_eq!(node_info, expected_node_info);
}

#[tokio::test(flavor = "multi_thread")]
async fn can_get_metadata() {
    let (api, handle) = spawn(NodeConfig::test()).await;

    let metadata = api.anvil_metadata().await.unwrap();

    let provider = handle.http_provider();

    let block_number = provider.get_block_number().await.unwrap();
    let chain_id = provider.get_chain_id().await.unwrap();
    let block = provider.get_block(BlockId::from(block_number)).await.unwrap().unwrap();

    let expected_metadata = Metadata {
        latest_block_hash: block.header.hash,
        latest_block_number: block_number,
        chain_id,
        client_version: CLIENT_VERSION.to_string(),
        instance_id: api.instance_id(),
        forked_network: None,
        snapshots: Default::default(),
    };

    assert_eq!(metadata, expected_metadata);
}

#[tokio::test(flavor = "multi_thread")]
async fn can_get_metadata_on_fork() {
    let (api, handle) =
        spawn(NodeConfig::test().with_eth_rpc_url(Some("https://bsc-dataseed.binance.org/"))).await;
    let provider = handle.http_provider();

    let metadata = api.anvil_metadata().await.unwrap();

    let block_number = provider.get_block_number().await.unwrap();
    let chain_id = provider.get_chain_id().await.unwrap();
    let block = provider.get_block(BlockId::from(block_number)).await.unwrap().unwrap();

    let expected_metadata = Metadata {
        latest_block_hash: block.header.hash,
        latest_block_number: block_number,
        chain_id,
        client_version: CLIENT_VERSION.to_string(),
        instance_id: api.instance_id(),
        forked_network: Some(ForkedNetwork {
            chain_id,
            fork_block_number: block_number,
            fork_block_hash: block.header.hash,
        }),
        snapshots: Default::default(),
    };

    assert_eq!(metadata, expected_metadata);
}

#[tokio::test(flavor = "multi_thread")]
async fn metadata_changes_on_reset() {
    let (api, _) =
        spawn(NodeConfig::test().with_eth_rpc_url(Some("https://bsc-dataseed.binance.org/"))).await;

    let metadata = api.anvil_metadata().await.unwrap();
    let instance_id = metadata.instance_id;

    api.anvil_reset(Some(Forking { json_rpc_url: None, block_number: None })).await.unwrap();

    let new_metadata = api.anvil_metadata().await.unwrap();
    let new_instance_id = new_metadata.instance_id;

    assert_ne!(instance_id, new_instance_id);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_get_transaction_receipt() {
    let (api, handle) = spawn(NodeConfig::test()).await;
    let provider = handle.http_provider();

    // set the base fee
    let new_base_fee = U256::from(1000);
    api.anvil_set_next_block_base_fee_per_gas(new_base_fee).await.unwrap();

    // send a EIP-1559 transaction
    let to = Address::random();
    let val = U256::from(1337);
    let tx = TransactionRequest::default().with_to(to).with_value(val);
    let tx = WithOtherFields::new(tx);

    let receipt = provider.send_transaction(tx.clone()).await.unwrap().get_receipt().await.unwrap();

    // the block should have the new base fee
    let block = provider.get_block(BlockId::default()).await.unwrap().unwrap();
    assert_eq!(block.header.base_fee_per_gas.unwrap(), new_base_fee.to::<u64>());

    // mine blocks
    api.evm_mine(None).await.unwrap();

    // the transaction receipt should have the original effective gas price
    let new_receipt = provider.get_transaction_receipt(receipt.transaction_hash).await.unwrap();
    assert_eq!(receipt.effective_gas_price, new_receipt.unwrap().effective_gas_price);
}

// test can set chain id
#[tokio::test(flavor = "multi_thread")]
async fn test_set_chain_id() {
    let (api, handle) = spawn(NodeConfig::test()).await;
    let provider = handle.http_provider();
    let chain_id = provider.get_chain_id().await.unwrap();
    assert_eq!(chain_id, 31337);

    let chain_id = 1234;
    api.anvil_set_chain_id(chain_id).await.unwrap();

    let chain_id = provider.get_chain_id().await.unwrap();
    assert_eq!(chain_id, 1234);
}

// <https://github.com/foundry-rs/foundry/issues/6096>
#[tokio::test(flavor = "multi_thread")]
async fn test_fork_revert_next_block_timestamp() {
    let (api, _handle) = spawn(fork_config()).await;

    // Mine a new block, and check the new block gas limit
    api.mine_one().await;
    let latest_block = api.block_by_number(BlockNumberOrTag::Latest).await.unwrap().unwrap();

    let state_snapshot = api.evm_snapshot().await.unwrap();
    api.mine_one().await;
    api.evm_revert(state_snapshot).await.unwrap();
    let block = api.block_by_number(BlockNumberOrTag::Latest).await.unwrap().unwrap();
    assert_eq!(block, latest_block);

    api.mine_one().await;
    let block = api.block_by_number(BlockNumberOrTag::Latest).await.unwrap().unwrap();
    assert!(block.header.timestamp >= latest_block.header.timestamp);
}

// test that after a snapshot revert, the env block is reset
// to its correct value (block number, etc.)
#[tokio::test(flavor = "multi_thread")]
async fn test_fork_revert_call_latest_block_timestamp() {
    let (api, handle) = spawn(fork_config()).await;
    let provider = handle.http_provider();

    // Mine a new block, and check the new block gas limit
    api.mine_one().await;
    let latest_block = api.block_by_number(BlockNumberOrTag::Latest).await.unwrap().unwrap();

    let state_snapshot = api.evm_snapshot().await.unwrap();
    api.mine_one().await;
    api.evm_revert(state_snapshot).await.unwrap();

    let multicall_contract =
        Multicall::new(address!("0xeefba1e63905ef1d7acba5a8513c70307c1ce441"), &provider);

    let timestamp = multicall_contract.getCurrentBlockTimestamp().call().await.unwrap();
    assert_eq!(timestamp, U256::from(latest_block.header.timestamp));

    let difficulty = multicall_contract.getCurrentBlockDifficulty().call().await.unwrap();
    assert_eq!(difficulty, U256::from(latest_block.header.difficulty));

    let gaslimit = multicall_contract.getCurrentBlockGasLimit().call().await.unwrap();
    assert_eq!(gaslimit, U256::from(latest_block.header.gas_limit));

    let coinbase = multicall_contract.getCurrentBlockCoinbase().call().await.unwrap();
    assert_eq!(coinbase, latest_block.header.beneficiary);
}

#[tokio::test(flavor = "multi_thread")]
async fn can_remove_pool_transactions() {
    let (api, handle) =
        spawn(NodeConfig::test().with_blocktime(Some(Duration::from_secs(5)))).await;

    let wallet = handle.dev_wallets().next().unwrap();
    let signer: EthereumWallet = wallet.clone().into();
    let from = wallet.address();

    let provider = http_provider_with_signer(&handle.http_endpoint(), signer);

    let sender = Address::random();
    let to = Address::random();
    let val = U256::from(1337);
    let tx = TransactionRequest::default().with_from(sender).with_to(to).with_value(val);
    let tx = WithOtherFields::new(tx);

    provider.send_transaction(tx.with_from(from)).await.unwrap().register().await.unwrap();

    let initial_txs = provider.txpool_inspect().await.unwrap();
    assert_eq!(initial_txs.pending.len(), 1);

    api.anvil_remove_pool_transactions(wallet.address()).await.unwrap();

    let final_txs = provider.txpool_inspect().await.unwrap();
    assert_eq!(final_txs.pending.len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_reorg() {
    let (api, handle) = spawn(NodeConfig::test()).await;
    let provider = handle.http_provider();

    let accounts = handle.dev_wallets().collect::<Vec<_>>();

    // Test calls
    // Populate chain
    for i in 0..10 {
        let tx = TransactionRequest::default()
            .to(accounts[0].address())
            .value(U256::from(i))
            .from(accounts[1].address());
        let tx = WithOtherFields::new(tx);
        api.send_transaction(tx).await.unwrap();

        let tx = TransactionRequest::default()
            .to(accounts[1].address())
            .value(U256::from(i))
            .from(accounts[2].address());
        let tx = WithOtherFields::new(tx);
        api.send_transaction(tx).await.unwrap();
    }

    // Define transactions
    let mut txs = vec![];
    for i in 0..3 {
        let from = accounts[i].address();
        let to = accounts[i + 1].address();
        for j in 0..5 {
            let tx = TransactionRequest::default().from(from).to(to).value(U256::from(j));
            txs.push((TransactionData::JSON(tx), i as u64));
        }
    }

    let prev_height = provider.get_block_number().await.unwrap();
    api.anvil_reorg(ReorgOptions { depth: 7, tx_block_pairs: txs }).await.unwrap();

    let reorged_height = provider.get_block_number().await.unwrap();
    assert_eq!(reorged_height, prev_height);

    // The first 3 reorged blocks should have 5 transactions each
    for num in 14..17 {
        let block = provider.get_block_by_number(num.into()).full().await.unwrap();
        let block = block.unwrap();
        assert_eq!(block.transactions.len(), 5);
    }

    // Verify that historic blocks are still accessible
    for num in (0..14).rev() {
        let _ = provider.get_block_by_number(num.into()).full().await.unwrap();
    }

    // Send a few more transaction to verify the chain can still progress
    for i in 0..3 {
        let tx = TransactionRequest::default()
            .to(accounts[0].address())
            .value(U256::from(i))
            .from(accounts[1].address());
        let tx = WithOtherFields::new(tx);
        api.send_transaction(tx).await.unwrap();
    }

    // Test reverting code
    let greeter = abi::Greeter::deploy(provider.clone(), "Reorg".to_string()).await.unwrap();
    api.anvil_reorg(ReorgOptions { depth: 5, tx_block_pairs: vec![] }).await.unwrap();
    let code = api.get_code(*greeter.address(), Some(BlockId::latest())).await.unwrap();
    assert_eq!(code, Bytes::default());

    // Test reverting contract storage
    let storage =
        abi::SimpleStorage::deploy(provider.clone(), "initial value".to_string()).await.unwrap();
    api.evm_mine(Some(MineOptions::Options { timestamp: None, blocks: Some(5) })).await.unwrap();
    let _ = storage
        .setValue("ReorgMe".to_string())
        .from(accounts[0].address())
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    api.anvil_reorg(ReorgOptions { depth: 3, tx_block_pairs: vec![] }).await.unwrap();
    let value = storage.getValue().call().await.unwrap();
    assert_eq!("initial value".to_string(), value);

    api.mine_one().await;
    api.mine_one().await;

    // Test raw transaction data
    let mut tx = TxEip1559 {
        chain_id: api.chain_id(),
        to: TxKind::Call(accounts[1].address()),
        value: U256::from(100),
        max_priority_fee_per_gas: 1000000000000,
        max_fee_per_gas: 10000000000000,
        gas_limit: 21000,
        ..Default::default()
    };
    let signature = accounts[5].sign_transaction_sync(&mut tx).unwrap();
    let tx = tx.into_signed(signature);
    let mut encoded = vec![];
    tx.eip2718_encode(&mut encoded);

    let pre_bal = provider.get_balance(accounts[5].address()).await.unwrap();
    api.anvil_reorg(ReorgOptions {
        depth: 1,
        tx_block_pairs: vec![(TransactionData::Raw(encoded.into()), 0)],
    })
    .await
    .unwrap();
    let post_bal = provider.get_balance(accounts[5].address()).await.unwrap();
    assert_ne!(pre_bal, post_bal);

    // Test reorg depth exceeding current height
    let res = api.anvil_reorg(ReorgOptions { depth: 100, tx_block_pairs: vec![] }).await;
    assert!(res.is_err());

    // Test reorg tx pairs exceeds chain length
    let res = api
        .anvil_reorg(ReorgOptions {
            depth: 1,
            tx_block_pairs: vec![(TransactionData::JSON(TransactionRequest::default()), 10)],
        })
        .await;
    assert!(res.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rollback() {
    let (api, handle) = spawn(NodeConfig::test()).await;
    let provider = handle.http_provider();

    // Mine 5 blocks
    for _ in 0..5 {
        api.mine_one().await;
    }

    // Get block 4 for later comparison
    let block4 = provider.get_block(4.into()).await.unwrap().unwrap();

    // Rollback with None should rollback 1 block
    api.anvil_rollback(None).await.unwrap();

    // Assert we're at block 4 and the block contents are kept the same
    let head = provider.get_block(BlockId::latest()).await.unwrap().unwrap();
    assert_eq!(head, block4);

    // Get block 1 for comparison
    let block1 = provider.get_block(1.into()).await.unwrap().unwrap();

    // Rollback to block 1
    let depth = 3; // from block 4 to block 1
    api.anvil_rollback(Some(depth)).await.unwrap();

    // Assert we're at block 1 and the block contents are kept the same
    let head = provider.get_block(BlockId::latest()).await.unwrap().unwrap();
    assert_eq!(head, block1);
}

// === wallet endpoints === //
#[tokio::test(flavor = "multi_thread")]
async fn can_get_wallet_capabilities() {
    let (api, handle) = spawn(NodeConfig::test().with_odyssey(true)).await;

    let provider = handle.http_provider();

    let init_sponsor_bal = provider.get_balance(EXECUTOR).await.unwrap();

    let expected_bal = Unit::ETHER.wei().saturating_mul(U256::from(10_000));
    assert_eq!(init_sponsor_bal, expected_bal);

    let p256_code = provider.get_code_at(P256_DELEGATION_CONTRACT).await.unwrap();

    assert_eq!(p256_code, Bytes::from_static(P256_DELEGATION_RUNTIME_CODE));

    let capabilities = api.get_capabilities().unwrap();

    let mut expect_caps = WalletCapabilities::default();
    let cap: Capabilities = Capabilities {
        delegation: DelegationCapability { addresses: vec![P256_DELEGATION_CONTRACT] },
    };
    expect_caps.insert(api.chain_id(), cap);

    assert_eq!(capabilities, expect_caps);
}

#[tokio::test(flavor = "multi_thread")]
async fn can_add_capability() {
    let (api, _handle) = spawn(NodeConfig::test().with_odyssey(true)).await;

    let init_capabilities = api.get_capabilities().unwrap();

    let mut expect_caps = WalletCapabilities::default();
    let cap: Capabilities = Capabilities {
        delegation: DelegationCapability { addresses: vec![P256_DELEGATION_CONTRACT] },
    };
    expect_caps.insert(api.chain_id(), cap);

    assert_eq!(init_capabilities, expect_caps);

    let new_cap_addr = Address::with_last_byte(1);

    api.anvil_add_capability(new_cap_addr).unwrap();

    let capabilities = api.get_capabilities().unwrap();

    let cap: Capabilities = Capabilities {
        delegation: DelegationCapability {
            addresses: vec![P256_DELEGATION_CONTRACT, new_cap_addr],
        },
    };
    expect_caps.insert(api.chain_id(), cap);

    assert_eq!(capabilities, expect_caps);
}

#[tokio::test(flavor = "multi_thread")]
async fn can_set_executor() {
    let (api, _handle) = spawn(NodeConfig::test().with_odyssey(true)).await;

    let expected_addr = address!("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
    let pk = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".to_string();

    let executor = api.anvil_set_executor(pk).unwrap();

    assert_eq!(executor, expected_addr);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_arb_get_block() {
    let (api, _handle) = spawn(NodeConfig::test().with_chain_id(Some(421611u64))).await;

    // Mine two blocks
    api.mine_one().await;
    api.mine_one().await;

    let best_number = api.block_number().unwrap().to::<u64>();

    assert_eq!(best_number, 2);

    let block = api.block_by_number(1.into()).await.unwrap().unwrap();

    assert_eq!(block.header.number, 1);
}

// Set next_block_timestamp same as previous block
// api.evm_set_next_block_timestamp(0).unwrap();
#[tokio::test(flavor = "multi_thread")]
async fn test_mine_blk_with_prev_timestamp() {
    let (api, handle) = spawn(NodeConfig::test()).await;
    let provider = handle.http_provider();

    let init_blk = provider.get_block(BlockId::latest()).await.unwrap().unwrap();

    let init_number = init_blk.header.number;
    let init_timestamp = init_blk.header.timestamp;

    // mock timestamp
    api.evm_set_next_block_timestamp(init_timestamp).unwrap();

    api.mine_one().await;

    let block = provider.get_block(BlockId::latest()).await.unwrap().unwrap();

    let next_blk_num = block.header.number;
    let next_blk_timestamp = block.header.timestamp;

    assert_eq!(next_blk_num, init_number + 1);
    assert_eq!(next_blk_timestamp, init_timestamp);

    // Sleep for 1 second
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Subsequent block should have a greater timestamp than previous block
    api.mine_one().await;

    let block = provider.get_block(BlockId::latest()).await.unwrap().unwrap();

    let third_blk_num = block.header.number;
    let third_blk_timestamp = block.header.timestamp;

    assert_eq!(third_blk_num, init_number + 2);
    assert_ne!(third_blk_timestamp, next_blk_timestamp);
    assert!(third_blk_timestamp > next_blk_timestamp);
}

// increase time by 0 seconds i.e next_block_timestamp = prev_block_timestamp
// api.evm_increase_time(0).unwrap();
#[tokio::test(flavor = "multi_thread")]
async fn test_increase_time_by_zero() {
    let (api, handle) = spawn(NodeConfig::test()).await;
    let provider = handle.http_provider();

    let init_blk = provider.get_block(BlockId::latest()).await.unwrap().unwrap();

    let init_number = init_blk.header.number;
    let init_timestamp = init_blk.header.timestamp;

    let _ = api.evm_increase_time(U256::ZERO).await;

    api.mine_one().await;

    let block = provider.get_block(BlockId::latest()).await.unwrap().unwrap();

    let next_blk_num = block.header.number;
    let next_blk_timestamp = block.header.timestamp;

    assert_eq!(next_blk_num, init_number + 1);
    assert_eq!(next_blk_timestamp, init_timestamp);
}

// evm_mine(MineOptions::Timestamp(prev_block_timestamp))
#[tokio::test(flavor = "multi_thread")]
async fn evm_mine_blk_with_same_timestamp() {
    let (api, handle) = spawn(NodeConfig::test()).await;
    let provider = handle.http_provider();

    let init_blk = provider.get_block(BlockId::latest()).await.unwrap().unwrap();

    let init_number = init_blk.header.number;
    let init_timestamp = init_blk.header.timestamp;

    api.evm_mine(Some(MineOptions::Timestamp(Some(init_timestamp)))).await.unwrap();

    let block = provider.get_block(BlockId::latest()).await.unwrap().unwrap();

    let next_blk_num = block.header.number;
    let next_blk_timestamp = block.header.timestamp;

    assert_eq!(next_blk_num, init_number + 1);
    assert_eq!(next_blk_timestamp, init_timestamp);
}

// mine 4 blocks instantly.
#[tokio::test(flavor = "multi_thread")]
async fn test_mine_blk_with_same_timestamp() {
    let (api, handle) = spawn(NodeConfig::test()).await;
    let provider = handle.http_provider();

    let init_blk = provider.get_block(BlockId::latest()).await.unwrap().unwrap();

    // Mine 4 blocks instantly
    let _ = api.anvil_mine(Some(U256::from(4)), None).await;

    let init_number = init_blk.header.number;
    let init_timestamp = init_blk.header.timestamp;
    let latest_blk_num = api.block_number().unwrap().to::<u64>();

    assert_eq!(latest_blk_num, init_number + 4);

    let mut blk_futs = vec![];
    for i in 1..=4 {
        blk_futs.push(provider.get_block(i.into()).into_future());
    }

    let timestamps = futures::future::join_all(blk_futs)
        .await
        .into_iter()
        .map(|blk| blk.unwrap().unwrap().header.timestamp)
        .collect::<Vec<_>>();

    // All timestamps should be equal. Allow for 1 second difference.
    assert!(timestamps.windows(2).all(|w| w[0] == w[1]), "{timestamps:#?}");
    assert!(
        timestamps[0] == init_timestamp || timestamps[0] == init_timestamp + 1,
        "{timestamps:#?} != {init_timestamp}"
    );
}

// <https://github.com/foundry-rs/foundry/issues/8962>
#[tokio::test(flavor = "multi_thread")]
async fn test_mine_first_block_with_interval() {
    let (api, _) = spawn(NodeConfig::test()).await;

    let init_block = api.block_by_number(0.into()).await.unwrap().unwrap();
    let init_timestamp = init_block.header.timestamp;

    // Mine 2 blocks with interval of 60.
    let _ = api.anvil_mine(Some(U256::from(2)), Some(U256::from(60))).await;

    let first_block = api.block_by_number(1.into()).await.unwrap().unwrap();
    assert_eq!(first_block.header.timestamp, init_timestamp + 60);

    let second_block = api.block_by_number(2.into()).await.unwrap().unwrap();
    assert_eq!(second_block.header.timestamp, init_timestamp + 120);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_anvil_reset_non_fork() {
    let (api, handle) = spawn(NodeConfig::test()).await;
    let provider = handle.http_provider();

    // Get initial state
    let init_block = provider.get_block(BlockId::latest()).await.unwrap().unwrap();
    let init_accounts = api.accounts().unwrap();
    let init_balance = provider.get_balance(init_accounts[0]).await.unwrap();

    // Store the instance id before reset
    let instance_id_before = api.instance_id();

    // Mine some blocks and make transactions
    for _ in 0..5 {
        api.mine_one().await;
    }

    // Send a transaction
    let to = Address::random();
    let val = U256::from(1337);
    let tx = TransactionRequest::default().with_from(init_accounts[0]).with_to(to).with_value(val);
    let tx = WithOtherFields::new(tx);

    let _ = provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();

    // Check state has changed
    let block_before_reset = provider.get_block(BlockId::latest()).await.unwrap().unwrap();
    assert!(block_before_reset.header.number > init_block.header.number);

    let balance_before_reset = provider.get_balance(init_accounts[0]).await.unwrap();
    assert!(balance_before_reset < init_balance);

    let to_balance_before_reset = provider.get_balance(to).await.unwrap();
    assert_eq!(to_balance_before_reset, val);

    // Reset to fresh in-memory state (non-fork)
    api.anvil_reset(None).await.unwrap();

    // Check instance id has changed
    let instance_id_after = api.instance_id();
    assert_ne!(instance_id_before, instance_id_after);

    // Check we're back at genesis
    let block_after_reset = provider.get_block(BlockId::latest()).await.unwrap().unwrap();
    assert_eq!(block_after_reset.header.number, 0);

    // Check accounts are restored to initial state
    let balance_after_reset = provider.get_balance(init_accounts[0]).await.unwrap();
    assert_eq!(balance_after_reset, init_balance);

    // Check the recipient's balance is zero
    let to_balance_after_reset = provider.get_balance(to).await.unwrap();
    assert_eq!(to_balance_after_reset, U256::ZERO);

    // Test we can continue mining after reset
    api.mine_one().await;
    let new_block = provider.get_block(BlockId::latest()).await.unwrap().unwrap();
    assert_eq!(new_block.header.number, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_anvil_reset_fork_to_non_fork() {
    let (api, handle) = spawn(fork_config()).await;
    let provider = handle.http_provider();

    // Verify we're in fork mode
    let metadata = api.anvil_metadata().await.unwrap();
    assert!(metadata.forked_network.is_some());

    // Mine some blocks
    for _ in 0..3 {
        api.mine_one().await;
    }

    // Reset to non-fork mode
    api.anvil_reset(None).await.unwrap();

    // Verify we're no longer in fork mode
    let metadata_after = api.anvil_metadata().await.unwrap();
    assert!(metadata_after.forked_network.is_none());

    // Check we're at block 0
    let block = provider.get_block(BlockId::latest()).await.unwrap().unwrap();
    assert_eq!(block.header.number, 0);

    // Verify we can still mine blocks
    api.mine_one().await;
    let new_block = provider.get_block(BlockId::latest()).await.unwrap().unwrap();
    assert_eq!(new_block.header.number, 1);
}
