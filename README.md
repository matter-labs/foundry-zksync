# ZkSync Smart Contract Testing, Deployment and Interaction Tooling Framework with Foundry
### About
Currently the industry standard Solidity smart contract test and deploy frameworks are Hardhat and Truffle. They both use JavaScript to test and deploy solidity smart contracts. Another smart contract testing platform by the name of Foundry. The  advantage that Foundry provides is that all tests are also written solidity creating a smoother developer experience. With Foundry, the engineer does not need to switch languages to write tests and deploy contracts.. 

Currently only Hardhat has developed functionality for zkSync. The purpose of this repository is to create functionality with Foundry to fully test, compile and deploy smart contracts on zkSync using only Solidity, as well as interact with those contracts once deployed.

### Quick Start / Installation

Clone these three repos to the same directory:

- [**foundry-zksync**](https://github.com/matter-labs/foundry-zksync) - this is our application, we will be building this in the following steps
- [**zksync-era**](https://github.com/sammyshakes/zksync-era) - this is the SDK repo, we need to pull this locally to repair broken dependencies
- [**sample-fzksync-project**](https://github.com/sammyshakes/sample-fzksync-project) - this is the sample project that contains the smart contract to be compiled


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

## Version 0.0

We need to establish the functionality we want for release v0.0 of this implementation. Below we will specify the exact features to accomplish our v0.0 release.

### Feature Set

- ***Compile smart contracts with zksolc compiler***
- ***Deploy smart contracts to zkSync Testnet or Local Test Node***
- ***Bridge assets L1 <-> L2***
- ***Make contract calls to deployed contracts on zkSync Testnet or Local Test Node***
- ***Send transactions to deployed contracts on zkSync Testnet or Local Test Node***
- ***Spin up Local Test Node***

NOTE: All commands are entered from the project root folder

---
## Compilation

***v0.0*** ***Command***:
## `forge zk-build`
### aliases: `forge zk-compile`, `forge zkb`

Compile smart contracts to zkEvm bytecode and store compile output files into a logical directory structure `<PROJECT-ROOT>/zkout/` for easy retrieval for other components of the application.




```
Compiler subcommands for zkSync

Usage: 
forge zk-build <CONTRACT_FILENAME> [OPTIONS]

Arguments:
  <CONTRACT FILENAME>  Contract filename from     project src/ ex: 'Contract.sol'

  Options:
      --use-zksolc   Specify zksolc compiler version (default if left blank)
      --system-mode  System mode flag
      --force-evmla  Sets the EVM legacy assembly pipeline forcibly
  -h, --help         Print help

  


```
### Example Usage
To compile `src/Greeter.sol` with only default compiler options (v1.3.9):
```
../foundry-zksync/target/debug/forge zk-build "Greeter.sol" 
```

### Compiler Settings
`zksolc` compiler version can optionally be configured using `--use-zksolc` flag:
```
../foundry-zksync/target/debug/forge zkb "Greeter.sol" --use-zksolc v1.3.8
```

### Output
`zksolc` compiler artifacts can be found in the folder:
```
<PROJECT-ROOT>/zkout/<CONTRACT_FILENAME>
```
![image](https://user-images.githubusercontent.com/76663878/231275745-4d33cb52-9a2a-4bc1-a48d-e9b5e48030c1.png)

---
## Deployment

***v0.0*** ***Command***:
## `forge zk-create`
### aliases: `forge zk-deploy`, `forge zkc`

Manage deployments in the native foundry/forge fashion, using the `forge zk-create` command.


```
Deploy to ZkSync with Chain Id. Ex. --zksync 280

Usage: forge zk-create <CONTRACT> [OPTIONS] <RPC-URL> <CHAIN-ID>

Arguments:
  <CONTRACT>
          The contract identifier in the form `<path>:<contractname>`.
  <RPC-URL> '--rpc-url'
  <CHAIN-ID>  `--chain 280' testnet, local: 270

Options:
  -h, --help  Print help

  --constructor-args <ARGS>...
          The constructor arguments.

  --factory-deps <FACTORY-DEPS>...
          The factory dependencies in the form `<path>:<contractname>`
```
Command Line:
```
`forge create <CONTRACT> --constructor-args [CONSTRUCTOR_ARGS] --rpc-url <http://localhost:3050> --private-key <PRIVATE_KEY> --chain <CHAIN_ID>`
```

### Example Usage
To Deploy `src/Counter.sol` to zksync local node:
```
../foundry-zksync/target/debug/forge zkc src/Counter.sol:Counter --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --rpc-url http://localhost:3050 --chain 270
```

### Output
```
0x5fe58d975604e6af62328d9e505181b94fc0718c, <---- Deployed contract address
```


---
## Interaction

Interact with deployed contracts in the native foundry/forge fashion using the CLI `cast call` and `cast send` commands>

- Retrieving and interacting with chain data, for example, block numbers and gas estimates
- Interact with deployed contracts on (zkSync Testnet or Local Docker Node)

***v0.0*** ***Commands***:

***Non-state changing calls:***

`cast call <CONTRACT_ADDRESS> <FUNCTION_SIG> --rpc-url zk-rpc`

***Send transactions:***

`cast send <CONTRACT_ADDRESS> <FUNCTION_SIG> <FUNCTION_ARGS> --rpc-url zk-sync --private-key <PRIVATE-KEY> --zksync`


---
## Bridging Assets

Bridge assets L1 ↔ L2 with `--zksync-deposit` and `--zksync-withdraw`

***L1 → L2 deposits:***

`cast send --rpc-url <RPC-URL> --private-key <PRIVATE-KEY> --zksync-deposit <TO> <AMOUNT> <TOKEN>`

***L2 → L1 withdrawals:***

`cast send --rpc-url <RPC-URL> --private-key <PRIVATE-KEY> --zksync-withdraw <TO> <AMOUNT> <TOKEN>`

---
## UPDATE 4/20
New build command with refactored code `forge zk-build`

```bash
# command line using forge zk-build:
../foundry-zksync/target/debug/forge zk-build --contract_name Greeter.sol --use_zksolc v1.3.8
```

zk-build commands saves compiled artifacts to `<PROJECT_ROOT>/zkout/` folder, adjacent to Foundry's native `/out/` folder

