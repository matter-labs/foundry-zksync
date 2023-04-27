# ZkSync Smart Contract Testing, Deployment and Interaction Framework with Foundry
### About
Currently the industry standard Solidity smart contract test and deploy frameworks are Hardhat and Truffle. They both use JavaScript to test and deploy solidity smart contracts. Another smart contract testing platform by the name of Foundry. The  advantage that Foundry provides is that all tests are also written solidity creating a smoother developer experience. With Foundry, the engineer does not need to switch languages to write tests and deploy contracts.. 

Currently only Hardhat has developed functionality for zkSync. The purpose of this repository is to create functionality with Foundry to fully test, compile and deploy smart contracts on zkSync using only Solidity, as well as interact with those contracts once deployed.

---
## Contents

- [**Quick Start / Installation**](https://github.com/matter-labs/foundry-zksync#quick-start--installation)
- [**v0.0 Feature Set**](https://github.com/matter-labs/foundry-zksync#feature-set)
- [**Environment Variables**](https://github.com/matter-labs/foundry-zksync#environment-variables)
- [**Compilation**](https://github.com/matter-labs/foundry-zksync#compilation)
- [**Deployment**](https://github.com/matter-labs/foundry-zksync#deployment)
- [**Contract Interaction**](https://github.com/matter-labs/foundry-zksync#contract-interaction)
- [**Bridging Assets**](https://github.com/matter-labs/foundry-zksync#bridging-assets-with-cast-zk-send)
- [**Deploy and Interact with `SimpleFactory.sol`**](https://github.com/matter-labs/foundry-zksync#usage-example-simplefactorysol)

---
---

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
## Environment Variables

By providing the following environment variables in the `.env` file at the `PROJECT-ROOT` folder, the `--rpc-url` and `--chain` flags can be ommitted in command lines.
```bash
# ETH_RPC_URL can be used to replace --rpc-url in command line 
ETH_RPC_URL=http://localhost:3050

# CHAIN can be used to replace --chain in command line  
# Local: 270, Testnet: 280
CHAIN=270
```

---
### Spin up local docker node
[Follow these instructions to set up local docker node](https://era.zksync.io/docs/api/hardhat/testing.html)

---
## Compilation

***v0.0*** ***Command***:
## `forge zk-build`
### aliases: `forge zkbuild`, `forge zk-compile`, `forge zkb`

Compile smart contracts to zkEvm bytecode and store compile output files into a logical directory structure `<PROJECT-ROOT>/zkout/` for easy retrieval for other components of the application.




```bash
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
#### Example Usage
To compile `src/Greeter.sol` with only default compiler options (v1.3.9):
```bash
../foundry-zksync/target/debug/forge zk-build "Greeter.sol" 
```

#### Compiler Settings
`zksolc` compiler version can optionally be configured using `--use-zksolc` flag:
```bash
../foundry-zksync/target/debug/forge zkb "Greeter.sol" --use-zksolc v1.3.8
```

#### Output
`zksolc` compiler artifacts can be found in the output folder:
```bash
<PROJECT-ROOT>/zkout/<CONTRACT_FILENAME>
```
![image](https://user-images.githubusercontent.com/76663878/234152279-e144e489-41ab-4cbd-8321-8ccd9b0aa6ef.png)

---
## Deployment

***v0.0*** ***Command***:
## `forge zk-create`
### aliases: `forge zkcreate`, `forge zk-deploy`, `forge zkc`

Manage deployments in the native foundry/forge fashion, using the `forge zk-create` command.


```bash
Deploy to ZkSync with Chain Id.

Usage: forge zk-create <CONTRACT> [OPTIONS] --rpc-url <RPC-URL> --chain <CHAIN-ID> --private-key <PRIVATE-KEY>

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

#### Example Usage
To Deploy `src/Greeter.sol` to zksync local node:
```bash
../foundry-zksync/target/debug/forge zkc src/Greeter.sol:Greeter --constructor-args "ZkSync + Pineapple" --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --rpc-url http://localhost:3050 --chain 270
```

#### Output
```js
Contract successfully deployed to address: 0x97b985951fd3e0c1d996421cc783d46c12d00082
Transaction Hash: 0xf8cc268c48f80ba30ab4b05ebc600b5ae044404efc3916d3e7b7c02fe0179710
```


---
## Contract Interaction
***v0.0*** ***Commands***:
## `cast zk-send`
### aliases: `cast zks`, `cast zksend`
Interact with deployed contracts in the native foundry/forge fashion using the CLI `cast call` and `cast zk-send` commands>

- Retrieving and interacting with chain data, for example, block numbers and gas estimates
- Interact with deployed contracts on (zkSync Testnet or Local Docker Node)

### ***Non-state changing calls:***

```bash
cast call <CONTRACT_ADDRESS> <FUNCTION_SIG> --rpc-url <RPC-URL>
```
#### Example Usage
```bash
../foundry-zksync/target/debug/cast call 0x97b985951fd3e0c1d996421cc783d46c12d00082 "greet()(string)" --rpc-url http://localhost:3050
```
#### Output
```js
ZkSync + Pineapple
```

## Send transactions:

```bash
cast zk-send <CONTRACT_ADDRESS> <FUNCTION_SIG> <FUNCTION_ARGS> --rpc-url <RPC-URL> --private-key <PRIVATE-KEY> --chain <CHAIN-ID>
```
### Example Usage
```bash
../foundry-zksync/target/debug/cast zk-send 0x97b985951fd3e0c1d996421cc783d46c12d00082 "setGreeting(string)" "Killer combo!"  --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```
#### Output
```js
Sending transaction....
Transaction Hash: 0x7651fba8ddeb624cca93f89da493675ccbc5c6d36ee25ed620b07424ce338552
```

#### Verify output
```bash
../foundry-zksync/target/debug/cast call 0x97b985951fd3e0c1d996421cc783d46c12d00082 "greet()(string)" --rpc-url http://localhost:3050
```
#### Output
```js
Killer combo!
```
---

## Bridging Assets with `cast zk-send`

### Bridge assets L1 ↔ L2 with `--deposit` and `---withdraw`

### ***L1 → L2 deposits:***

```bash
cast zk-send --deposit <TO> --amount <AMOUNT> <TOKEN> --rpc-url <RPC-URL> --private-key <PRIVATE-KEY>
```
NOTE: Leave <TOKEN> blank to bridge ETH

#### Example Usage
```bash
../foundry-zksync/target/debug/cast zk-send --deposit 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 --amount 1000000 --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```
#### Output
```js
Bridging assets....
Transaction Hash: 0x55793df0a636aedd098309e3487c6d9ec0910422d5b9f0bdbdf764bc82dc1b9f
```

### ***L2 → L1 withdrawals:***

```bash
cast zk-send --withdraw <TO> --amount <AMOUNT> <TOKEN> --rpc-url <RPC-URL> --private-key <PRIVATE-KEY>
```
#### Example Usage
```bash
../foundry-zksync/target/debug/cast zk-send --withdraw 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 --amount 1000000 --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```
#### Output
```js
Bridging assets....
Transaction Hash: 0x94ef9e2eed345dcfef6f0b4f953f431b8edc6760e29b598a7cf446dab18d6317
```
---

## Usage Example: `SimpleFactory.sol`
### Deploying and Interacting with `SimpleFactory.sol`

#### Compile contracts:
`SimpleFactory.sol`
```bash
../foundry-zksync/target/debug/forge zk-build "SimpleFactory.sol" 
```
`Child.sol`
```bash
../foundry-zksync/target/debug/forge zk-build "Child.sol"
```
`StepChild.sol`
```bash
../foundry-zksync/target/debug/forge zk-build "StepChild.sol"
```

### Deploy `SimpleFactory.sol`

```bash
../foundry-zksync/target/debug/forge zkc src/SimpleFactory.sol:SimpleFactory --constructor-args 01000041691510d85ddfc6047cba6643748dc028636d276f09a546ab330697ef 010000238a587670be26087b7812eab86eca61e7c4014522bdceda86adb2e82f --factory-deps src/Child.sol:Child src/StepChild.sol:StepChild --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --rpc-url http://localhost:3050 --chain 270
```

#### Output:
```bash
Contract successfully deployed to address: 0x23cee3fb585b1e5092b7cfb222e8e873b05e9519
Transaction Hash: 0x498066df55979cbe182d4cea4487eb8e5acff2433094fe2f7317590957095028
```

### Deploy `StepChlid.sol` via `SimpleFactory.sol`
```bash
../foundry-zksync/target/debug/cast zk-send 0x23cee3fb585b1e5092b7cfb222e8e873b05e9519 "newStepChild()" --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```

#### Output:
```bash
Sending transaction....
Transaction Hash: 0xa82a0636b71af058d4916d81868eebc41173ca07b78d30fe57f4b74e9294ef25
```

### Interact with `SimpleFactory.sol`
```bash
../foundry-zksync/target/debug/cast call 0x23cee3fb585b1e5092b7cfb222e8e873b05e9519 "stepChildren(uint256)(address)" 0 --rpc-url http://localhost:3050
```

#### Output:
`StepChild.sol` deployed address:
```js
0xbc88C5Cdfe2659ebDD5dbb7e1a695A4cb189Df96
```

### Interact with `StepChild.sol`
Use `cast call` to check initial state:
```bash
../foundry-zksync/target/debug/cast call 0xbc88C5Cdfe2659ebDD5dbb7e1a695A4cb189Df96 "isEnabled()(bool)" --rpc-url http://localhost:3050
```

#### Output:
```js
false
```

Use `cast zk-send` to modify state:
```bash
../foundry-zksync/target/debug/cast zk-send 0xbc88C5Cdfe2659ebDD5dbb7e1a695A4cb189Df96 "enable()" --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```

#### Output:
```bash
Sending transaction....
Transaction Hash: 0xe005e15e9f58b7dcdcc7b16a9d5c706ddef7a4c9cab82216ea944d5344ba01ae
```


Use `cast call` to check modified state:
```bash
../foundry-zksync/target/debug/cast call 0xbc88C5Cdfe2659ebDD5dbb7e1a695A4cb189Df96 "isEnabled()(bool)" --rpc-url http://localhost:3050
```

#### Output:
```js
true
```

