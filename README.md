## ZkSync Smart Contract Testing, Deployment and Interaction Tooling Framework with Foundry
### About
Currently the industry standard Solidity smart contract test and deploy frameworks are Hardhat and Truffle. They both use JavaScript to test and deploy solidity smart contracts. Another smart contract testing platform by the name of Foundry. The  advantage that Foundry provides is that all tests are also written solidity creating a smoother developer experience. With Foundry, the engineer does not need to switch languages to write tests and deploy contracts.. 

Currently only Hardhat has developed functionality for zkSync. The purpose of this repository is to create functionality with Foundry to fully test, compile and deploy smart contracts on zkSync using only Solidity, as well as interact with those contracts once deployed.

## Quick Start

Clone these three repos to the same directory:

- [**foundry-zksync**](https://github.com/matter-labs/foundry-zksync) - this is our application, we will be building this in the following steps
- [**zksync-era**](https://github.com/sammyshakes/zksync-era) - this is the SDK repo, we need to pull this locally to repair broken dependencies
- [**sample-fzksync-project**](https://github.com/sammyshakes/sample-fzksync-project) - this is the sample project that contains the smart contract to be compiled


### Installation

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
$ cargo build -p foundry-cli
```
---

# Version 0.0

We need to establish the functionality we want for release v0.0 of this implementation. Below we will specify the exact features to accomplish our v0.0 release.

### Feature Set

- ***Compile smart contracts with zksolc compiler***
- ***Deploy smart contracts to zkSync Testnet or Local Test Node***
- ***Bridge assets L1 <-> L2***
- ***Make contract calls to deployed contracts on zkSync Testnet or Local Test Node***
- ***Send transactions to deployed contracts on zkSync Testnet or Local Test Node***
- ***Spin up Local Test Node***

NOTE: All commands are entered from the project root folder

## Compilation

***v0.0*** ***Command***:
## `forge build`

Compile smart contracts to zkEvm bytecode using the `--zksync` flag and store compile output files into a logical directory structure `out/zksync/` for easy retrieval to other components of the application.


For the `forge build` help screen:
```
../foundry-zksync/target/debug/forge build --zksync --help
```
```
Compiler subcommands for zkSync

Usage: forge build --zksync [OPTIONS] <CONTRACT FILENAME>

Arguments:
  <CONTRACT FILENAME>  Contract filename from project src/ ex: 'Contract.sol'

Options:
      --system-mode  System mode flag
      --force-evmla  Sets the EVM legacy assembly pipeline forcibly
  -h, --help         Print help
```
### Example Usage
To compile `src/Greeter.sol` with only default compiler options:
```
../foundry-zksync/target/debug/forge build --zksync "Greeter.sol" 
```

### Compiler Settings
`zksolc` compiler can be configured using `.env` file in project root
```
ZKSOLC_COMPILER_VERSION = "1.3.8"       # zksolc compiler version
OS = "linux"                            # OPERATING SYSTEM: linux, windows, macosx
ARCH = "amd64"                          # ARCHITIECTURE: amd64, arm64

```

###Output
`zksolc` compiler artifacts as well as standard-json input file can be found in the folder:
```
<PROJECT-ROOT>/out/zksolc/<CONTRACT_FILENAME>
```
![image](https://user-images.githubusercontent.com/76663878/231275745-4d33cb52-9a2a-4bc1-a48d-e9b5e48030c1.png)

## Deployment

***v0.0*** ***Command***:
## `forge create`

Manage deployments in the native foundry/forge fashion, using the `forge create` command with the `--zksync` flag.

- Deploy to zkSync specified networks (zkSync Testnet or Local Docker Node)  via `--rpc-url` and `--chain-id` flags
- Support Command line input arguments that currently exist for `forge create` plus newly built zkSync specific command line arguments for a more seamless user experience. Examples: `--rpc-url`, `--chain-id`, `--private-key`

For the `forge create` help screen:
```
../foundry-zksync/target/debug/forge create --zksync --help
```

```
Deploy to ZkSync with Chain Id. Ex. --zksync 280

Usage: forge create <CONTRACT> --zksync <CHAIN-ID>

Arguments:
  <CHAIN-ID>  Chain id testnet: 280, local: 270

Options:
  -h, --help  Print help
```
Command Line:
```
`forge create <CONTRACT_PATH> --constructor-args [CONSTRUCTOR_ARGS] --rpc-url <http://localhost:3050> --private-key <PRIVATE_KEY> --zksync <CHAIN_ID>`
```

### Example Usage
To Deploy `src/Greeter.sol` to zksync local node:
```
../foundry-zksync/target/debug/forge create src/Greeter.sol:Greeter --constructor-args "ZkSync + Pineapple" --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --zksync 270
```

### Output
```
0x5fe58d975604e6af62328d9e505181b94fc0718c, <---- Deployed contract address
```



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


