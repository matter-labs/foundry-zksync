## Installation / Quick Start

Clone these three repos to the same directory:

- [**foundry-zksync**](https://github.com/matter-labs/foundry-zksync) - this is our application, we will be building this in the following steps
- [**zksync-era**](https://github.com/sammyshakes/zksync-era) - this is the SDK repo, we need to pull this locally to repair broken dependencies
- [**sample-fzksync-project**](https://github.com/sammyshakes/sample-fzksync-project) - this is the sample project that contains the smart contract to be compiled


### Quick Start

```bash
# make working directory and cd anywhere on filesystem
$ mkdir fzksync && cd fzksync
# clone foundry-zksync
$ git clone https://github.com/matter-labs/foundry-zksync.git 
# clone zksync-era
$ git clone https://github.com/sammyshakes/zksync-era.git
# clone fzksync-project
$ git clone https://github.com/sammyshakes/sample-fzksync-project.git
# cd into foundry-zksync, swap branch, and build
$ cd foundry-zksync
$ git checkout -b explore
$ git pull origin explore
$ cargo build -p foundry-cli
```
---
# Compile

# Version 0.0

We need to establish the functionality we want for release v0.0 of this implementation. Below we will specify the exact features to accomplish our v0.0 release.

## Feature Sets

- ***Compile smart contracts with zksolc compiler***
- ***Deploy smart contracts to zkSync Testnet or Local Test Node***
- ***Bridge assets L1 <-> L2***
- ***Make contract calls to deployed contracts on zkSync Testnet or Local Test Node***
- ***Send transactions to deployed contracts on zkSync Testnet or Local Test Node***
- ***Spin up Local Test Node***
- *********************************Run either unit tests or integration tests*********************************

## Compilation

Compile smart contracts to zkEvm bytecode and store compile output files into a logical directory structure `out/zksync/` for easy retrieval to other components of the application.

- Configuration/CLI
    - Support Command line input arguments that currently exist for `forge build` plus newly built zkSync specific command line arguments to support different compiler versions and operating systems via the standard configuration files and CLI flag overrides
- Managing Contracts for both networks
    - Currently we compile both EVM and zkEVM smart contracts when using the `--zksync` flag thus creating artifacts for both networks, keeping them separate yet organized in the output directory.

***v0.0*** ***Command***:

`forge build --zksync <CONTRACT_PATH>`

## Deployment

Manage deployments in the native foundry/forge fashion, using the `forge create` command.

- Prepare deployment transaction according to zkSync specifications using CL/Configuration files.
- Deploy to zkSync specified networks (zkSync Testnet or Local Docker Node)  via `--rpc-url` and `--chain-id` flags
- Configuration/CLI
    - Support Command line input arguments that currently exist for `forge create` plus newly built zkSync specific command line arguments for a more seamless user experience. Examples: `--rpc-url`, `--chain-id`, `--private-key`, `--zksync` and more via the standard configuration files and CLI flag overrides

***v0.0*** ***Command***:

`forge create <CONTRACT_PATH> --constructor-args [CONSTRUCTOR_ARGS] --rpc-url <http://localhost:3050> --private-key <PRIVATE_KEY> --zksync <CHAIN_ID>`

## Interaction

Interact with deployed contracts in the native foundry/forge fashion using the CLI `cast call` and `cast send` commands>

- Retrieving and interacting with chain data, for example, block numbers and gas estimates
- Interact with deployed contracts on (zkSync Testnet or Local Docker Node)
- Bridging assets L1 ↔ L2 with `--zksync-deposit` and `--zksync-withdraw`
- Use proper configuration techniques describe above

***v0.0*** ***Commands***:

***Non-state changing calls:***

`cast call <CONTRACT_ADDRESS> <FUNCTION_SIG> --rpc-url zk-rpc`

***Send transactions:***

`cast send <CONTRACT_ADDRESS> <FUNCTION_SIG> <FUNCTION_ARGS> --rpc-url zk-sync --private-key <PRIVATE-KEY> --zksync`

***L1 → L2 deposits:***

`cast send --rpc-url <RPC-URL> --private-key <PRIVATE-KEY> --zksync-deposit <TO> <AMOUNT> <TOKEN>`

***L2 → L1 withdrawals:***

`cast send --rpc-url <RPC-URL> --private-key <PRIVATE-KEY> --zksync-withdraw <TO> <AMOUNT> <TOKEN>`


