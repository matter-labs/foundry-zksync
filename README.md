test ci

# Foundry with zkSync Era v0.1

This repository provides [Foundry](https://github.com/foundry-rs/foundry) functionality in Solidity for compiling, deploying, and interacting with smart contracts on zkSync Era.

### Supported features

- Compile smart contracts with the [zksolc compiler](https://github.com/matter-labs/zksolc-bin).
- Deploy smart contracts to zkSync Era mainnet, testnet, or local test node.
- Bridge assets L1 <-> L2.
- Call deployed contracts on zkSync Era testnet or local test node.
- Send transactions to deployed contracts on zkSync Era testnet or local test node.

---

## Contents

- [Set up](https://github.com/matter-labs/foundry-zksync#set-up)
- [Interact with blockchain](https://github.com/matter-labs/foundry-zksync#interact-with-blockchain)
- [Compile](https://github.com/matter-labs/foundry-zksync#compile)
- [Deploy](https://github.com/matter-labs/foundry-zksync#deploy)
- [Bridge assets](https://github.com/matter-labs/foundry-zksync#bridge-assets-l1--l2)
- [Interact with contract](https://github.com/matter-labs/foundry-zksync#interact-with-contract)
- [Deploy and interact with `SimpleFactory.sol`](https://github.com/matter-labs/foundry-zksync#deploy-and-interact-with-simplefactorysol)
- [Account abstraction multisig example](https://github.com/matter-labs/foundry-zksync#account-abstraction-multisig)
- [Troubleshooting](https://github.com/matter-labs/foundry-zksync#troubleshooting)

---

## Set up

### Prerequisites

- [Rust compiler](https://www.rust-lang.org/tools/install).

### Installation

> Installation steps include cloning the zkSync Era application, [**foundry-zksync**](https://github.com/matter-labs/foundry-zksync), and the example project, [**sample-fzksync-project**](https://github.com/sammyshakes/sample-fzksync-project).

1. Create a top-level directory and `cd` into it.

```
mkdir fzksync && cd fzksync
```

2. Clone the repos into the same directory.

```sh
git clone https://github.com/matter-labs/foundry-zksync.git 
git clone https://github.com/sammyshakes/sample-fzksync-project.git
```

3. `cd` into the `foundry-zksync` and build the application.

```sh
cd foundry-zksync
cargo build -p foundry-cli
```

4. `cd` into the project folder and update the git submodules.

```sh
cd ../sample-fzksync-project
git submodule update --init --recursive
```

### Environment variables

Create a new file `.env` at the `PROJECT-ROOT` and copy/paste the following environment variables. 

```txt
# ETH_RPC_URL replaces --rpc-url on the command line 
ETH_RPC_URL=http://localhost:3050

## L1_RPC_URL and L2_RPC_URL are only used for `zk-deposit`
L1_RPC_URL=https://localhost:8545
# L2_RPC_URL can be used to replace --l2-url in command line for `zk-deposit`
L2_RPC_URL=https://localhost:3050

# CHAIN replaces --chain in command line  
# Local: 270, Testnet: 280, Mainnet: 324
CHAIN=270
```

### Spin up local Docker node

Follow [these instructions]((https://era.zksync.io/docs/api/hardhat/testing.html)) to set up a local Docker node.

---

## Interact with blockchain

> Enter all commands from the `sample-fzksync-project` project root.

### Use the `zkcast` command

#### Get chain id of local node

```sh
../foundry-zksync/target/debug/zkcast chain-id --rpc-url http://localhost:3050
```

**Output**

270

#### Get chain id of testnet 

```sh
../foundry-zksync/target/debug/zkcast chain-id --rpc-url https://zksync2-testnet.zksync.dev:443
```

**Output**

```sh
280
```

#### Get client 

```sh
../foundry-zksync/target/debug/zkcast client --rpc-url https://zksync2-testnet.zksync.dev:443
```

**Output**

```sh
zkSync/v2.0
```

#### Get account's L2 ETH balance

```sh 
../foundry-zksync/target/debug/zkcast balance 0x42C7eF198f8aC9888E2B1b73e5B71f1D4535194A --rpc-url https://zksync2-testnet.zksync.dev:443
```

**Output**

```sh
447551277794355871
```

#### Get gas price

```sh
../foundry-zksync/target/debug/zkcast gas-price --rpc-url https://zksync2-testnet.zksync.dev:443
```

**Example output**

```sh
250000000
```

#### Get timestamp of latest block

```sh
../foundry-zksync/target/debug/zkcast age --block latest --rpc-url https://zksync2-testnet.zksync.dev:443
```

**Example output**

```sh
Mon May  1 16:11:07 2023
```

#### Get latest block

```sh
../foundry-zksync/target/debug/zkcast block latest --rpc-url https://zksync2-testnet.zksync.dev:443
```

**Example output**

```sh
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

## Compile with `zkforge zk-build`

> Aliases: `zkforge zkbuild`, `zkforge zk-compile`, `zkforge zkb`.

Compile smart contracts to zkEVM bytecode and store the compiled output files in a logical directory structure `<PROJECT-ROOT>/zkout/` for easy retrieval by other components of the application.

```sh

Compiler subcommands for zkSync

Usage: 
zkforge zk-build [OPTIONS]

Options:
      --use-zksolc   Specify zksolc compiler version (default if left blank)
      --is-system    Enable the system contract compilation mode.
      --force-evmla  Sets the EVM legacy assembly pipeline forcibly
      -h, --help         Print help
```

> `--is-system` flag: It is necessary to compile some contracts, including those that deploy other contracts (such as factory contracts), using the `--is-system` flag. These contracts should be placed in the `src/is-system/` folder. If the folder does not exist, manually create it.

        ![image](https://user-images.githubusercontent.com/76663878/236301037-2a536ab0-3d09-44f3-a74d-5f5891af335b.png)

### Example usage

To compile with default compiler options (v1.3.11).

```sh
../foundry-zksync/target/debug/zkforge zk-build 
```

### Compiler settings

Configure the `zksolc` compiler version using the optional `--use` flag.

```bash
../foundry-zksync/target/debug/zkforge zkb --use 0.8.19
```

**Example output**

`zksolc` compiler artifacts can be found in the output folder:

```bash
<PROJECT-ROOT>/zkout/<CONTRACT_FILENAME>
```
![image](https://user-images.githubusercontent.com/76663878/234152279-e144e489-41ab-4cbd-8321-8ccd9b0aa6ef.png)

Example terminal output:

![image](https://user-images.githubusercontent.com/76663878/236305625-8c7519e2-0c5e-492f-a4bc-3b019a95e34f.png)

NOTE: Currently, until `forge remappings` are implemented, import paths must be relative to the contract importing it:

![image](https://github.com/matter-labs/foundry-zksync/assets/76663878/490b34f4-e286-42a7-8570-d4b228ec10c7)

`SimpleFactory.sol` and `AAFactory.sol` are in the `src/is-system/` folder.

---

## Deploy with `zkforge zk-create`

> Aliases: `zkforge zkcreate`, `zkforge zk-deploy`, `zkforge zkc`

```sh
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

#### Example

To deploy `src/Greeter.sol` to zkSync testnet:

```bash
../foundry-zksync/target/debug/zkforge zkc src/Greeter.sol:Greeter --constructor-args "ZkSync + Pineapple" --private-key <"PRIVATE_KEY"> --rpc-url https://zksync2-testnet.zksync.dev:443 --chain 280
```

#### Output

```txt
Deploying contract...
+-------------------------------------------------+
Contract successfully deployed to address: 0x07d485ff2df314b240ec392ed86b137a661ddd35
Transaction Hash: 0xdb6864fe1d19572a3ff509c5c7ed43f033d2dab8261a843808ed46e6e6ee51be
Gas used: 89879008
Effective gas price: 250000000
Block Number: 6651906
+-------------------------------------------------+
```

---

## Bridge assets L1 ↔ L2 with `zkcast zk-send` and `zkcast zk-deposit`

### L1 → L2 deposits

```sh
zkcast zk-deposit <TO> <AMOUNT> <TOKEN> --l1-rpc-url <L1-RPC-URL> --l2-url <L2URL> --chain <CHAIN-ID> --private-key <PRIVATE-KEY>
```
NOTE: Leave `<TOKEN>` blank to bridge ETH

```bash
Usage: zkcast zk-deposit  <TO> <AMOUNT> --l1-rpc-url <ETH_RPC_URL> --l2-url <L2URL> [OPTIONS] [BRIDGE] [TIP]

Arguments:
  <TO>
          The L2 address that receives the tokens.

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

#### Example - error on this one

```sh
../foundry-zksync/target/debug/zkcast zkdeposit 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 1000000 --rpc-url http://localhost:8545 --l2-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```

#### Output

```txt
Bridging assets....
Transaction Hash: 0x55793df0a636aedd098309e3487c6d9ec0910422d5b9f0bdbdf764bc82dc1b9f
```
---

### L2 → L1 withdrawals

```sh
zkcast zk-send --withdraw <TO> --amount <AMOUNT> <TOKEN> --rpc-url <RPC-URL> --private-key <PRIVATE-KEY>


Arguments:
  [TO]                  The withdraw recipient.


Bridging options:
  -w, --withdraw        For L2 -> L1 withdrawals.

      --token <TOKEN>   Token to bridge. Leave blank for ETH.

  -a, --amount <AMOUNT> Amount of token to bridge. Required value when bridging
```

#### Example

```sh
../foundry-zksync/target/debug/zkcast zk-send --withdraw 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 --amount 1000000 --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```

#### Output

```text
Bridging assets....
+-------------------------------------------------+
Transaction Hash: 0x3562f47db61de149fb7266c3a65935c4e8324cceb5a1db8718390a8a5a210191
Gas used: 10276475
Effective gas price: 250000000
Block Number: 6652714
+-------------------------------------------------+
```

---

## Interact with contract with `zkcast zk-send`

> Aliases: `zkcast zks`, `zkcast zksend`

Interact with deployed contracts in the native foundry/zkforge fashion using the CLI `zkcast zk-send` command.

```sh
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

- Retrieve and interact with chain data. For example, block numbers and gas estimates.
- Interact with deployed contracts on (zkSync Era testnet or local Docker node).

### Non-state changing calls

```sh
zkcast call <CONTRACT_ADDRESS> <FUNCTION_SIG> --rpc-url <RPC-URL>
```

#### Example

```bash
../foundry-zksync/target/debug/zkcast call 0x97b985951fd3e0c1d996421cc783d46c12d00082 "greet()(string)" --rpc-url http://localhost:3050
```

#### Output

```txt
ZkSync + Pineapple
```

### Send transactions

```sh
zkcast zk-send <CONTRACT_ADDRESS> <FUNCTION_SIG> <FUNCTION_ARGS> --rpc-url <RPC-URL> --private-key <PRIVATE-KEY> --chain <CHAIN-ID>
```

#### Example

```sh
../foundry-zksync/target/debug/zkcast zk-send 0x97b985951fd3e0c1d996421cc783d46c12d00082 "setGreeting(string)" "Killer combo!"  --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```

#### Output

```txt
Sending transaction....
Transaction Hash: 0x7651fba8ddeb624cca93f89da493675ccbc5c6d36ee25ed620b07424ce338552
```

#### Verify output

```sh
../foundry-zksync/target/debug/zkcast call 0x97b985951fd3e0c1d996421cc783d46c12d00082 "greet()(string)" --rpc-url http://localhost:3050
```

#### Output

```txt
Killer combo!
```

---

## Deploy and interact with `SimpleFactory.sol`

### Compile contract

`SimpleFactory.sol` must be compiled with the `is-system` flag, so they need to be placed in the `src/is-system/` folder

```bash
../foundry-zksync/target/debug/zkforge zk-build
```

### Deploy `SimpleFactory.sol`

```sh
../foundry-zksync/target/debug/zkforge zkc src/SimpleFactory.sol:SimpleFactory --constructor-args 01000041691510d85ddfc6047cba6643748dc028636d276f09a546ab330697ef 010000238a587670be26087b7812eab86eca61e7c4014522bdceda86adb2e82f --factory-deps src/Child.sol:Child src/StepChild.sol:StepChild --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --rpc-url http://localhost:3050 --chain 270
```

#### Output

```txt
Deploying contract...
+-------------------------------------------------+
Contract successfully deployed to address: 0xa1b809005e589f81de6ef9f48d67e35606c05fc3
Transaction Hash: 0x34782985ba7c70b6bc4a8eb2b95787baec29356171fdbb18608037a2fcd7eda8
Gas used: 168141
Effective gas price: 250000000
Block Number: 249
+-------------------------------------------------+
```

### Deploy `StepChild.sol` via `SimpleFactory.sol`

```sh
../foundry-zksync/target/debug/zkcast zk-send 0x23cee3fb585b1e5092b7cfb222e8e873b05e9519 "newStepChild()" --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```

#### Output

```sh
Sending transaction....
Transaction Hash: 0xa82a0636b71af058d4916d81868eebc41173ca07b78d30fe57f4b74e9294ef25
```

### Interact with `SimpleFactory.sol`

```sh
../foundry-zksync/target/debug/zkcast call 0x23cee3fb585b1e5092b7cfb222e8e873b05e9519 "stepChildren(uint256)(address)" 0 --rpc-url http://localhost:3050
```

#### Output

`StepChild.sol` deployed address:

```txt
0xbc88C5Cdfe2659ebDD5dbb7e1a695A4cb189Df96
```

### Interact with `StepChild.sol`

Use `zkcast call` to check initial state:

```sh
../foundry-zksync/target/debug/zkcast call 0xbc88C5Cdfe2659ebDD5dbb7e1a695A4cb189Df96 "isEnabled()(bool)" --rpc-url http://localhost:3050
```

#### Output:

```txt
false
```

Use `zkcast zk-send` to modify state:

```sh
../foundry-zksync/target/debug/zkcast zk-send 0xbc88C5Cdfe2659ebDD5dbb7e1a695A4cb189Df96 "enable()" --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```

#### Output

```sh
Sending transaction....
Transaction Hash: 0xe005e15e9f58b7dcdcc7b16a9d5c706ddef7a4c9cab82216ea944d5344ba01ae
```

Use `zkcast call` to check modified state.

```sh
../foundry-zksync/target/debug/zkcast call 0xbc88C5Cdfe2659ebDD5dbb7e1a695A4cb189Df96 "isEnabled()(bool)" --rpc-url http://localhost:3050
```

#### Output

```txt
true
```

---

## Account abstraction multisig

This section compiles, deploys, and interacts with the contracts from the zkSync Era [**Account Abstraction Multisig example**](https://era.zksync.io/docs/dev/tutorials/custom-aa-tutorial.html)

Contracts:

- [**AAFactory.sol**](https://era.zksync.io/docs/dev/tutorials/custom-aa-tutorial.html)
- [**TwoUserMultiSig.sol**](https://github.com/sammyshakes/sample-fzksync-project/blob/main/src/TwoUserMultiSig.sol)

### Compile `AAFactory.sol`

`AAFactory.sol` needs to be compiled with the `--is-system` flag because it will be interacting with system contracts to deploy the multisig wallets.

Place the contract in the `src/is-system/` folder

```sh
# command line using zkforge zk-build
../foundry-zksync/target/debug/zkforge zk-build
```

#### Output

```sh
AAFactory -> Bytecode Hash: "010000791703a54dbe2502b00ee470989c267d0f6c0d12a9009a947715683744" 
Compiled Successfully
```

### Deploy `AAFactory.sol`:

To deploy the factory we need the `Bytecode Hash` of the `TwoUserMultiSig.sol` contract to provide to the constructor of `AAFactory.sol`.

```js
constructor(bytes32 _aaBytecodeHash) {
        aaBytecodeHash = _aaBytecodeHash;
    }
```

Note: `aaBytecodeHash` = Bytecode hash of `TwoUserMultiSig.sol`

To deploy a contract that deploys other contracts, it is necessary to provide the bytecodes of the child contracts in the `factory-deps` field of the transaction. This can be accomplished by using the `--factory-deps` flag and providing the full contract path in the format: `<path>:<contractname>`

```sh
# command line using zkforge zk-create
../foundry-zksync/target/debug/zkforge zkc src/is-system/AAFactory.sol:AAFactory --constructor-args 010007572230f4df5b4e855ff48d4cdfffc9405522117d7e020ee42650223460 --factory-deps src/TwoUserMultiSig.sol:TwoUserMultisig --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --rpc-url http://localhost:3050 --chain 270
```

#### Output

```sh
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

Here is the interface of `deployAccount`.

```js
function deployAccount(bytes32 salt, address owner1, address owner2) external returns (address accountAddress)
```

We need to provide the two owner addresses for the newly deployed multisig:

```js
owner1 = 0xa61464658AfeAf65CccaaFD3a512b69A83B77618
owner2 = 0x0D43eB5B8a47bA8900d84AA36656c92024e9772e
```

We are also just using a `0x00` value for the ***salt*** parameter. (You will need a unique value for salt for each instance that uses same owner wallets).

```sh
# command line using zkcast zk-send
../foundry-zksync/target/debug/zkcast zk-send 0xd5608cec132ed4875d19f8d815ec2ac58498b4e5 "deployAccount(bytes32,address,address)(address)" 0x00 0xa61464658AfeAf65CccaaFD3a512b69A83B77618 0x0D43eB5B8a47bA8900d84AA36656c92024e9772e --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```

#### Output

```sh
Sending transaction....
Transaction Hash: 0x43a4dded84a12891dfae4124b42b9f091750e953193bd779a7e5e4d422909e73
0x03e50ec034f1d363de0add752c33d4831a2731bf, <---- Deployed contract address
```

The new `TwoUserMultiSig.sol` contract has been deployed to:

```txt
0x03e50ec034f1d363de0add752c33d4831a2731bf
```

Check the tx receipt using `zkcast tx <TX-HASH>`

```sh
../foundry-zksync/target/debug/zkcast tx 0x22364a3e191ad10013c5f20036e9696e743a4f686bc58a0106ef0b9e7592347c --rpc-url http://localhost:3050
```

#### Output

```sh
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

Verify with `zkcast call` to call the public variables 'owner1' and 'owner2' on the newly deployed `TwoUserMultiSig.sol` contract.

Verify `owner1`:

```sh
# command line using zkcast call
../foundry-zksync/target/debug/zkcast call 0x03e50ec034f1d363de0add752c33d4831a2731bf "owner1()(address)" --rpc-url http://localhost:3050
```

#### Output

```txt
0xa61464658AfeAf65CccaaFD3a512b69A83B77618
```

Verify `owner2`:

```sh
# command line using zkcast call
../foundry-zksync/target/debug/zkcast call 0x03e50ec034f1d363de0add752c33d4831a2731bf "owner2()(address)" --rpc-url http://localhost:3050
```

#### Output

```txt
0x0D43eB5B8a47bA8900d84AA36656c92024e9772e
```

## Troubleshooting

### Verify arguments

Make sure that:

* You are using zksync specific methods (`zkcreate` not `create`, `zksend` not `send`).
* You set the correct `--rpc-url`.
* You have the proper contract address - the bytecodes in zkSync Era are different to in EVM - so the resulting contracts will be deployed at different addresses.

### 'Method not found' when calling 'send'

If you get errors like `(code: -32601, message: Method not found, data: None)` - you are probably using a `send` method instead of `zksend`.

### 'Could not get solc: Unknown version provided', 'checksum not found'

These errors might show up on the Mac with ARM chip (M1, M2) due to the fact that most recent solc compilers are not auto-downloaded there.

There are 2 workarounds:

 - Use an older compiler by adding `--use 0.8.19` flag to the `zk-build` command.
 - Download the compiler manually and then use the `--offline` mode. (Download the compiler into ~/.svm/VERSION/solc-VERSION -- for example ~/.svm/0.8.20/solc-0.8.20).

You can get the latest compiler version for MacOs AARCH here: https://github.com/ethers-rs/solc-builds/tree/master/macosx/aarch64

You might have to remove the `zkout` directory (that holds the compilation artifacts) and in some rare scenarios also cleanup the installed solc versions (by removing `~/.svm/` directory)

### `solc` versions >0.8.19 are not supported, found 0.8.20

This means that our zksync compiler doesn't support that version of solidity yet.

In such case, please remove the artifacts (by removing `zkout` directory) and re-run with the older version of solidity (`--use 0.8.19`) for example.

You might also have to remove the `~/.svm/0.8.20/solc-0.8.20` file.
