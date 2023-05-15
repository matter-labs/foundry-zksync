# ZkSync Smart Contract Testing, Deployment and Interaction Framework with Foundry
### About
Currently the industry standard Solidity smart contract test and deploy frameworks are Hardhat and Truffle. They both use JavaScript to test and deploy solidity smart contracts. Another smart contract testing platform by the name of Foundry. The  advantage that Foundry provides is that all tests are also written solidity creating a smoother developer experience. With Foundry, the engineer does not need to switch languages to write tests and deploy contracts.. 

Currently only Hardhat has developed functionality for zkSync. The purpose of this repository is to create functionality with Foundry to fully test, compile and deploy smart contracts on zkSync using only Solidity, as well as interact with those contracts once deployed.

---
## Contents

- [**Quick Start / Installation**](https://github.com/matter-labs/foundry-zksync#quick-start--installation)
- [**v0.0 Feature Set**](https://github.com/matter-labs/foundry-zksync#feature-set)
- [**Environment Variables**](https://github.com/matter-labs/foundry-zksync#environment-variables)
- [**Blockchain Interaction**](https://github.com/matter-labs/foundry-zksync/blob/main/README.md#blockchain-interaction)
- [**Bridging Assets**](https://github.com/matter-labs/foundry-zksync#bridging-assets-with-cast-zk-send)
- [**Compilation**](https://github.com/matter-labs/foundry-zksync#compilation)
- [**Deployment**](https://github.com/matter-labs/foundry-zksync#deployment)
- [**Contract Interaction**](https://github.com/matter-labs/foundry-zksync#contract-interaction)
- [**Deploy and Interact with `SimpleFactory.sol`**](https://github.com/matter-labs/foundry-zksync#usage-example-simplefactorysol)
- [**Account Abstraction Multisig example**](https://github.com/matter-labs/foundry-zksync#account-abstraction-multisig)


---
---

### Quick Start / Installation

Clone these two repos to the same directory:

- [**foundry-zksync**](https://github.com/matter-labs/foundry-zksync) - this is our application, we will be building this in the following steps
- [**sample-fzksync-project**](https://github.com/sammyshakes/sample-fzksync-project) - this is the sample project that contains the smart contract to be compiled


```bash
# make working directory and cd anywhere on filesystem
$ mkdir fzksync && cd fzksync
# clone foundry-zksync
$ git clone https://github.com/matter-labs/foundry-zksync.git 
# clone fzksync-project
$ git clone https://github.com/sammyshakes/sample-fzksync-project.git
# cd into foundry-zksync and build
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

NOTE: All commands are entered from the project root folder

---

## Environment Variables

By providing the following environment variables in the `.env` file at the `PROJECT-ROOT` folder, the `--rpc-url` and `--chain` flags can be ommitted in command lines.
```bash
# ETH_RPC_URL can be used to replace --rpc-url in command line 
ETH_RPC_URL=http://localhost:3050

# ZKSYNC_RPC_URL can be used to replace --l2-url in command line 
ZKSYNC_RPC_URL=https://zksync2-testnet.zksync.dev

# CHAIN can be used to replace --chain in command line  
# Local: 270, Testnet: 280
CHAIN=270
```

---

### Spin up local docker node
[Follow these instructions to set up local docker node](https://era.zksync.io/docs/api/hardhat/testing.html)

---

## Blockchain Interaction

### Use the `zkcast` command to get blockchain data:
```bash
# chain id local node
../foundry-zksync/target/debug/zkcast chain-id --rpc-url http://localhost:3050
# output:
270

#TESTNET
# chain id testnet
../foundry-zksync/target/debug/zkcast chain-id --rpc-url https://zksync2-testnet.zksync.dev:443
# output:
280

# client
../foundry-zksync/target/debug/zkcast client --rpc-url https://zksync2-testnet.zksync.dev:443
# output:
zkSync/v2.0

# get account L2 eth balance
../foundry-zksync/target/debug/zkcast balance 0x42C7eF198f8aC9888E2B1b73e5B71f1D4535194A --rpc-url https://zksync2-testnet.zksync.dev:443
# output:
447551277794355871

# gas price
../foundry-zksync/target/debug/zkcast gas-price --rpc-url https://zksync2-testnet.zksync.dev:443
# output:
250000000

# timestamp of latest block
../foundry-zksync/target/debug/zkcast age --block latest --rpc-url https://zksync2-testnet.zksync.dev:443
# output:
Mon May  1 16:11:07 2023

# get latest block:
../foundry-zksync/target/debug/zkcast block latest --rpc-url https://zksync2-testnet.zksync.dev:443
# output:
baseFeePerGas        250000000
difficulty           0
extraData            0x
gasLimit             4294967295
gasUsed              40277767
hash                 0x6c5b7c9b82b48bd77c0f506d74ed32aec6ab5c52e6c9c604ee8825a0b4a68289
logsBloom            0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
miner                0x0000000000000000000000000000000000000000
mixHash              0x0000000000000000000000000000000000000000000000000000000000000000
nonce                0x0000000000000000
number               5024177
parentHash           0x9fbb3c9e5ef3b7807152367eeab5759cce14c290118de0e9011777a640cd7068
receiptsRoot         0x0000000000000000000000000000000000000000000000000000000000000000
sealFields           []
sha3Uncles           0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347
size                 0
stateRoot            0x0000000000000000000000000000000000000000000000000000000000000000
timestamp            1682957640
totalDifficulty      0
l1BatchNumber        null
l1BatchTimestamp     null
```

---

## Bridging Assets L1 ↔ L2 

### Bridge assets L1 ↔ L2 with `zkcast zk-send` and `zkcast zk-deposit`


### ***L1 → L2 deposits:*** 

```bash
zkcast zk-deposit <TO> <AMOUNT> <TOKEN> --rpc-url <RPC-URL> --l2-url <L2URL> --chain <CHAIN-ID> --private-key <PRIVATE-KEY>
```
NOTE: Leave `<TOKEN>` blank to bridge ETH

```bash
Usage: zkcast zk-deposit  <TO> <AMOUNT> --rpc-url <ETH_RPC_URL> --l2-url <L2URL> [OPTIONS] [BRIDGE] [TIP]

Arguments:
  <TO>
          The destination of the transaction.

  <AMOUNT>
          Amount of token to deposit.

  [BRIDGE]
          The address of a custom bridge to call.

  [TIP]
          Optional fee that the user can choose to pay in addition to the regular transaction fee.

Options:
  -z, --l2-url <L2URL>
          The zkSync RPC Layer 2 endpoint. Can be provided via the env var ZKSYNC_RPC_URL or --l2-url from the command line.
          
          NOTE: For Deposits, ETH_RPC_URL, or --rpc-url should be set to the Layer 1 RPC URL
          
          [env: ZKSYNC_RPC_URL=https://zksync2-testnet.zksync.dev]

      --token <TOKEN>
          Token to bridge. Leave blank for ETH.

  -h, --help
          Print help (see a summary with '-h')
```

#### Example Usage
```bash
../foundry-zksync/target/debug/zkcast zkdeposit 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 1000000 --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```
#### Output
```js
Bridging assets....
Transaction Hash: 0x55793df0a636aedd098309e3487c6d9ec0910422d5b9f0bdbdf764bc82dc1b9f
```
---

### ***L2 → L1 withdrawals:***

```bash
zkcast zk-send --withdraw <TO> --amount <AMOUNT> <TOKEN> --rpc-url <RPC-URL> --private-key <PRIVATE-KEY>


Arguments:
  [TO]                  The withdraw recipient.


Bridging options:
  -w, --withdraw        For L2 -> L1 withdrawals.

      --token <TOKEN>   Token to bridge. Leave blank for ETH.

  -a, --amount <AMOUNT> Amount of token to bridge. Required value when bridging
```

```
#### Example Usage
```bash
../foundry-zksync/target/debug/zkcast zk-send --withdraw 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 --amount 1000000 --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```
#### Output
```js
Bridging assets....
Transaction Hash: 0x94ef9e2eed345dcfef6f0b4f953f431b8edc6760e29b598a7cf446dab18d6317
```

---

## Compilation

***v0.0*** ***Command***:
## `zkforge zk-build`
### aliases: `zkforge zkbuild`, `zkforge zk-compile`, `zkforge zkb`

Compile smart contracts to zkEvm bytecode and store compile output files into a logical directory structure `<PROJECT-ROOT>/zkout/` for easy retrieval for other components of the application.




```bash
Compiler subcommands for zkSync

Usage: 
zkforge zk-build [OPTIONS]


Options:
      --use-zksolc   Specify zksolc compiler version (default if left blank)
      --is-system    Enable the system contract compilation mode.
      --force-evmla  Sets the EVM legacy assembly pipeline forcibly
      -h, --help         Print help

  
```

### Compiling with `--is-system` flag
It is necessary for some contracts, including those that deploy other contracts (such as factory contracts), to be compiled using the `--is-sytem` flag. These contracts should be placed in the `src/is-system/` folder, if the folder does not exist, manually create it:

![image](https://user-images.githubusercontent.com/76663878/236301037-2a536ab0-3d09-44f3-a74d-5f5891af335b.png)

#### Example Usage
To compile with only default compiler options (v1.3.9):
```bash
../foundry-zksync/target/debug/zkforge zk-build 
```

#### Compiler Settings
`zksolc` compiler version can optionally be configured using `--use-zksolc` flag:
```bash
../foundry-zksync/target/debug/zkforge zkb --use-zksolc v1.3.8
```

#### Output
`zksolc` compiler artifacts can be found in the output folder:
```bash
<PROJECT-ROOT>/zkout/<CONTRACT_FILENAME>
```
![image](https://user-images.githubusercontent.com/76663878/234152279-e144e489-41ab-4cbd-8321-8ccd9b0aa6ef.png)

Example terminal output:

![image](https://user-images.githubusercontent.com/76663878/236305625-8c7519e2-0c5e-492f-a4bc-3b019a95e34f.png)

---
## Deployment

***v0.0*** ***Command***:
## `zkforge zk-create`
### aliases: `zkforge zkcreate`, `zkforge zk-deploy`, `zkforge zkc`

Manage deployments in the native foundry/zkforge fashion, using the `zkforge zk-create` command.

```bash
Deploy smart contracts to zksync.

Usage: zkforge zk-create <CONTRACT> [OPTIONS] --rpc-url <RPC-URL> --chain <CHAIN-ID> --private-key <PRIVATE-KEY>

Options:
  -h, --help
          Print help (see a summary with '-h')

ZkCreate options:
      --constructor-args <ARGS>...
          The constructor arguments.

      --constructor-args-path <FILE>
          The path to a file containing the constructor arguments.

  <CONTRACT>
          The contract identifier in the form `<path>:<contractname>`.

ZkSync Features:
      --factory-deps <FACTORY-DEPS>...
          The factory dependencies in the form `<path>:<contractname>`.
```


#### Example Usage
To Deploy `src/Greeter.sol` to zksync local node:
```bash
../foundry-zksync/target/debug/zkforge zkc src/Greeter.sol:Greeter --constructor-args "ZkSync + Pineapple" --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --rpc-url http://localhost:3050 --chain 270
```

#### Output
```js
Deploying contract...
+-------------------------------------------------+
Contract successfully deployed to address: 0xa1b809005e589f81de6ef9f48d67e35606c05fc3
Transaction Hash: 0x34782985ba7c70b6bc4a8eb2b95787baec29356171fdbb18608037a2fcd7eda8
Gas used: 168141
Effective gas price: 250000000
Block Number: 249
+-------------------------------------------------+
```


---
## Contract Interaction
***v0.0*** ***Commands***:
## `zkcast zk-send`
### aliases: `zkcast zks`, `zkcast zksend`
Interact with deployed contracts in the native foundry/zkforge fashion using the CLI `zkcast zk-send` command:
```bash
Sign and publish a zksync transaction.

Usage: zkcast zk-send [OPTIONS] [TO] [SIG] [ARGS]...

Arguments:
  [TO]        The destination of the transaction.

  [SIG]       The signature of the function to call.

  [ARGS]...   The arguments of the function to call.

Options:
  -h, --help   Print help (see a summary with '-h')

Bridging options:
  -d, --deposit         For L1 -> L2 deposits.

  -w, --withdraw        For L2 -> L1 withdrawals.

      --token <TOKEN>   Token to bridge. Leave blank for ETH.

  -a, --amount <AMOUNT> Amount of token to bridge. Required value when bridging
```

- Retrieving and interacting with chain data, for example, block numbers and gas estimates
- Interact with deployed contracts on (zkSync Testnet or Local Docker Node)

### ***Non-state changing calls:***

```bash
zkcast call <CONTRACT_ADDRESS> <FUNCTION_SIG> --rpc-url <RPC-URL>
```
#### Example Usage
```bash
../foundry-zksync/target/debug/zkcast call 0x97b985951fd3e0c1d996421cc783d46c12d00082 "greet()(string)" --rpc-url http://localhost:3050
```
#### Output
```js
ZkSync + Pineapple
```

## Send transactions:

```bash
zkcast zk-send <CONTRACT_ADDRESS> <FUNCTION_SIG> <FUNCTION_ARGS> --rpc-url <RPC-URL> --private-key <PRIVATE-KEY> --chain <CHAIN-ID>
```
### Example Usage
```bash
../foundry-zksync/target/debug/zkcast zk-send 0x97b985951fd3e0c1d996421cc783d46c12d00082 "setGreeting(string)" "Killer combo!"  --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```
#### Output
```js
Sending transaction....
Transaction Hash: 0x7651fba8ddeb624cca93f89da493675ccbc5c6d36ee25ed620b07424ce338552
```

#### Verify output
```bash
../foundry-zksync/target/debug/zkcast call 0x97b985951fd3e0c1d996421cc783d46c12d00082 "greet()(string)" --rpc-url http://localhost:3050
```
#### Output
```js
Killer combo!
```

---

## Usage Example: `SimpleFactory.sol`
### Deploying and Interacting with `SimpleFactory.sol`

#### Compile contracts:
`SimpleFactory.sol` must be compiled with the `is-system` flag, so they need to be placed in the `src/is-sytem/` folder

```bash
../foundry-zksync/target/debug/zkforge zk-build
```

### Deploy `SimpleFactory.sol`

```bash
../foundry-zksync/target/debug/zkforge zkc src/SimpleFactory.sol:SimpleFactory --constructor-args 01000041691510d85ddfc6047cba6643748dc028636d276f09a546ab330697ef 010000238a587670be26087b7812eab86eca61e7c4014522bdceda86adb2e82f --factory-deps src/Child.sol:Child src/StepChild.sol:StepChild --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --rpc-url http://localhost:3050 --chain 270
```

#### Output:
```js
Deploying contract...
+-------------------------------------------------+
Contract successfully deployed to address: 0xa1b809005e589f81de6ef9f48d67e35606c05fc3
Transaction Hash: 0x34782985ba7c70b6bc4a8eb2b95787baec29356171fdbb18608037a2fcd7eda8
Gas used: 168141
Effective gas price: 250000000
Block Number: 249
+-------------------------------------------------+
```

### Deploy `StepChlid.sol` via `SimpleFactory.sol`
```bash
../foundry-zksync/target/debug/zkcast zk-send 0x23cee3fb585b1e5092b7cfb222e8e873b05e9519 "newStepChild()" --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```

#### Output:
```bash
Sending transaction....
Transaction Hash: 0xa82a0636b71af058d4916d81868eebc41173ca07b78d30fe57f4b74e9294ef25
```

### Interact with `SimpleFactory.sol`
```bash
../foundry-zksync/target/debug/zkcast call 0x23cee3fb585b1e5092b7cfb222e8e873b05e9519 "stepChildren(uint256)(address)" 0 --rpc-url http://localhost:3050
```

#### Output:
`StepChild.sol` deployed address:
```js
0xbc88C5Cdfe2659ebDD5dbb7e1a695A4cb189Df96
```

### Interact with `StepChild.sol`
Use `zkcast call` to check initial state:
```bash
../foundry-zksync/target/debug/zkcast call 0xbc88C5Cdfe2659ebDD5dbb7e1a695A4cb189Df96 "isEnabled()(bool)" --rpc-url http://localhost:3050
```

#### Output:
```js
false
```

Use `zkcast zk-send` to modify state:
```bash
../foundry-zksync/target/debug/zkcast zk-send 0xbc88C5Cdfe2659ebDD5dbb7e1a695A4cb189Df96 "enable()" --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```

#### Output:
```bash
Sending transaction....
Transaction Hash: 0xe005e15e9f58b7dcdcc7b16a9d5c706ddef7a4c9cab82216ea944d5344ba01ae
```


Use `zkcast call` to check modified state:
```bash
../foundry-zksync/target/debug/zkcast call 0xbc88C5Cdfe2659ebDD5dbb7e1a695A4cb189Df96 "isEnabled()(bool)" --rpc-url http://localhost:3050
```

#### Output:
```js
true
```

---

# Account abstraction multisig


### This section compiles, deploys and interacts with the contracts from the zkSync Era [**Account Abstraction Multisig example**](https://era.zksync.io/docs/dev/tutorials/custom-aa-tutorial.html)

Contracts:
- [**AAFactory.sol**](https://era.zksync.io/docs/dev/tutorials/custom-aa-tutorial.html)
- [**TwoUserMultiSig.sol**](https://github.com/sammyshakes/sample-fzksync-project/blob/main/src/TwoUserMultiSig.sol)

## Compile `AAFactory.sol`:
#### `AAFactory.sol` needs to be compiled with the `--is-system` flag because it will be interacting with system contracts to deploy the multisig wallets.
It needs to be placed in the `src/is-sytem/` folder

```bash
# command line using zkforge zk-build
../foundry-zksync/target/debug/zkforge zk-build
```
#### Output:
```bash
AAFactory -> Bytecode Hash: "010000791703a54dbe2502b00ee470989c267d0f6c0d12a9009a947715683744" 
Compiled Successfully
```

## Deploy `AAFactory.sol`:

To deploy the factory we need the `Bytecode Hash` of the `TwoUserMultiSig.sol` contract to provide to the constructor of `AAFactory.sol`:

```js
constructor(bytes32 _aaBytecodeHash) {
        aaBytecodeHash = _aaBytecodeHash;
    }
```
`Note: `aaBytecodeHash` = BytecodeHash of "TwoUserMultiSig.sol"`

#### To deploy a contract that deploys other contracts it is necessary to provide the bytecodes of the children contracts in the `factory-deps` field of the transaction. This can be accomplished by using the `--factory-deps` flag and providing the full contract path in the format: `<path>:<contractname>`

```bash
# command line using zkforge zk-create
../foundry-zksync/target/debug/zkforge zkc src/is-system/AAFactory.sol:AAFactory --constructor-args 010007572230f4df5b4e855ff48d4cdfffc9405522117d7e020ee42650223460 --factory-deps src/TwoUserMultiSig.sol:TwoUserMultisig --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --rpc-url http://localhost:3050 --chain 270
```

#### Output:
```bash
Deploying contract...
+-------------------------------------------------+
Contract successfully deployed to address: 0xd5608cec132ed4875d19f8d815ec2ac58498b4e5
Transaction Hash: 0x0e6f55ff1619af8b3277853a8f2941d0481635880358316f03ae264e2de059ed
Gas used: 154379
Effective gas price: 250000000
Block Number: 291
+-------------------------------------------------+
```

Now that we have the `AAFactory.sol` contract address we can call `deployAccount` function to deploy a new `TwoUserMultiSig.sol` instance.

Here is the interface of `deployAccount`:
```js
function deployAccount(bytes32 salt, address owner1, address owner2) external returns (address accountAddress)
```

we need to provide the two owner addresses for the newly deployed multisig:
```js
owner1 = 0xa61464658AfeAf65CccaaFD3a512b69A83B77618
owner2 = 0x0D43eB5B8a47bA8900d84AA36656c92024e9772e
```

We are also just using a `0x00` value for the ***salt*** parameter. (you will need a unique value for salt for each instance that uses same owner wallets)
```bash
# command line using zkcast zk-send
../foundry-zksync/target/debug/zkcast zk-send 0xd5608cec132ed4875d19f8d815ec2ac58498b4e5 "deployAccount(bytes32,address,address)(address)" 0x00 0xa61464658AfeAf65CccaaFD3a512b69A83B77618 0x0D43eB5B8a47bA8900d84AA36656c92024e9772e --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```

#### Output:
```bash
Sending transaction....
Transaction Hash: 0x43a4dded84a12891dfae4124b42b9f091750e953193bd779a7e5e4d422909e73
0x03e50ec034f1d363de0add752c33d4831a2731bf, <---- Deployed contract address
```
Viola! The new `TwoUserMultiSig.sol` contract has been deployed to:
```js
0x03e50ec034f1d363de0add752c33d4831a2731bf
```

We can check the tx receipt using `zkcast tx <TX-HASH>`
```bash
../foundry-zksync/target/debug/zkcast tx 0x22364a3e191ad10013c5f20036e9696e743a4f686bc58a0106ef0b9e7592347c --rpc-url http://localhost:3050
```

### Output:
```bash
blockHash            0x2f3e2be46a7cb9f9e9df503903990e6670e88224e52232c988b5a730c82d98c0
blockNumber          297
from                 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049
gas                  217367
gasPrice             250000000
hash                 0x43a4dded84a12891dfae4124b42b9f091750e953193bd779a7e5e4d422909e73
input                0x76fb8b650000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a61464658afeaf65cccaafd3a512b69a83b776180000000000000000000000000d43eb5b8a47ba8900d84aa36656c92024e9772e
nonce                147
r                    0x16385d99ccaaa5e84bb97d76a0afb310350c2ca4165ed41d458efa80cd76d3bd
s                    0x3ec55287f223e760b7dd82a676feece939832e4c5a3d73f3aa979bd2cd48801c
to                   0xd5608cEC132ED4875D19f8d815EC2ac58498B4E5
transactionIndex     0
v                    1
value                0
l1BatchNumber        149
l1BatchTxIndex       0
```

We can verify by using `zkcast call` to call the public variables 'owner1' and 'owner2' on the newly deployed `TwoUserMultiSig.sol` contract:

Verify `owner1`:
```bash
# command line using zkcast call
../foundry-zksync/target/debug/zkcast call 0x03e50ec034f1d363de0add752c33d4831a2731bf "owner1()(address)" --rpc-url http://localhost:3050
```
#### Output:
```js
0xa61464658AfeAf65CccaaFD3a512b69A83B77618
```

Verify `owner2`:
```bash
# command line using zkcast call
../foundry-zksync/target/debug/zkcast call 0x03e50ec034f1d363de0add752c33d4831a2731bf "owner2()(address)" --rpc-url http://localhost:3050
```
#### Output:
```js
0x0D43eB5B8a47bA8900d84AA36656c92024e9772e
```
