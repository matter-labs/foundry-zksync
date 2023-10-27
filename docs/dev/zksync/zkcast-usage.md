# zkcast Usage Guide

Welcome to the zkcast usage guide! This document will provide you with detailed instructions on how to use various commands within zkcast to interact with blockchain, bridge assets between L1 and L2, and interact with contracts. The guide is structured for clarity and ease of use.

## Setting Up

### Spin up Local Docker Node

- Follow the [Instructions to setup local Docker node](https://era.zksync.io/docs/tools/testing/dockerized-testing.html).

## Basic Blockchain Interactions

### Get Chain ID

- **Local Node:**
  ```sh
  zkcast chain-id --rpc-url http://localhost:3050
  ```
  **Output:** `270`

- **Testnet:**
  ```sh
  zkcast chain-id --rpc-url https://zksync2-testnet.zksync.dev:443
  ```
  **Output:** `280`

### Get Client Information

- **Command:**
  ```sh
  zkcast client --rpc-url https://zksync2-testnet.zksync.dev:443
  ```
  **Output:** `zkSync/v2.0`

### Get Account's L2 ETH Balance

- **Command:**
  ```sh 
  zkcast balance 0x42C7eF198f8aC9888E2B1b73e5B71f1D4535194A --rpc-url https://zksync2-testnet.zksync.dev:443
  ```
  **Output:** `447551277794355871`

### Get Gas Price

- **Command:**
  ```sh
  zkcast gas-price --rpc-url https://zksync2-testnet.zksync.dev:443
  ```
  **Example Output:** `250000000`

### Get Latest Block

- **Command:**
  ```sh
  zkcast block latest --rpc-url https://zksync2-testnet.zksync.dev:443
  ```
  **Example Output:** 
  ```sh
  baseFeePerGas        250000000
  ...
  l1BatchTimestamp     null
  ```

## Bridging Assets Between L1 and L2

### L1 → L2 Deposits

- **Command:**
  ```sh
  zkcast zk-deposit <TO> <AMOUNT> <TOKEN> --l1-rpc-url <L1-RPC-URL> --l2-url <L2URL> --chain <CHAIN-ID> --private-key <PRIVATE-KEY>
  ```
  **Note:** Leave `<TOKEN>` blank to bridge ETH.

  **Example (Error Case):**
  ```sh
  zkcast zkdeposit 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 1000000 --rpc-url http://localhost:8545 --l2-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
  ```
  **Output:** `Bridging assets.... Transaction Hash: 0x55793df0a636aedd098309e3487c6d9ec0910422d5b9f0bdbdf764bc82dc1b9f`

### L2 → L1 Withdrawals

- **Command:**
  ```sh
  zkcast zk-send --withdraw <TO> --amount <AMOUNT> <TOKEN> --rpc-url <RPC-URL> --private-key <PRIVATE-KEY>
  ```
  **Example:**
  ```sh
  zkcast zk-send --withdraw 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 --amount 1000000 --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
  ```
  **Output:** 
  ```
  Bridging assets....
  Transaction Hash: 0x3562f47db61de149fb7266c3a65935c4e8324cceb5a1db8718390a8a5a210191
  Gas used: 10276475
  Effective gas price: 250000000
  Block Number: 6652714
  ```

## Interacting with Contracts

### General Usage

- **Aliases:** `zkcast zks`, `zkcast zksend`
- **Purpose:** Interact with deployed contracts in the native foundry/zkforge fashion using the CLI `zkcast zk-send` command.
- **Scope:** Retrieve and interact with chain data, such as block numbers and gas estimates. Interact with deployed contracts on zkSync Era testnet or local Docker node.

### Non-state Changing Calls

- **Command:**
  ```sh
  zkcast call <CONTRACT_ADDRESS> <FUNCTION_SIG> --rpc-url <RPC-URL>
  ```
  **Example:**
  ```bash
  zkcast call 0x97b985951fd3e0c1d996421cc783d46c12d00082 "greet()(string)" --rpc-url http://localhost:3050
  ```
  **Output:** `ZkSync + Pineapple`

### Send Transactions

- **Command:**
  ```sh
  zkcast zk-send <CONTRACT_ADDRESS> <FUNCTION_SIG> <FUNCTION_ARGS> --rpc-url <RPC-URL> --private-key <PRIVATE-KEY> --chain <CHAIN-ID>
  ```
  **Example:**
  ```sh
  zkcast zk-send 0x97b985951fd3e0c1d996421cc783d46c12d00082 "setGreeting(string)" "Killer combo!"  --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
  ```
  **Output:** 
  ```
  Sending transaction....
  Transaction Hash: 0x7651fba8ddeb624cca93f89da493675ccbc5c6d36ee25ed620b07424ce338552
  ```

### Verify Output

- **Command:**
  ```sh
  zkcast call 0x97b985951fd3e0c1d996421cc783d46c12d00082 "greet()(string)" --rpc-url http://localhost:3050
  ```
  **Output:** `Killer combo!`

This guide provides a comprehensive overview of the commands available in zkcast for various blockchain interactions, bridging assets