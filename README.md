# Foundry with zkSync Era v0.1

This repository provides [Foundry](https://github.com/foundry-rs/foundry) functionality in Solidity for compiling, deploying, testing, and interacting with smart contracts on **zkSync Era**. 

**What is foundry?**

Foundry is a blazing fast, portable and modular toolkit for Ethereum application development written in Rust.

Foundry consists of:

- **Forge:** Ethereum testing framework (like Truffle, Hardhat and DappTools).
- **Cast:** Swiss army knife for interacting with EVM smart contracts, sending transactions and getting chain data.
- **Anvil:** Local Ethereum node, akin to Ganache, Hardhat Network.
- **Chisel:** Fast, utilitarian, and verbose solidity REPL.

Need help getting started with Foundry? Read the 📖 [Foundry Book](https://book.getfoundry.sh/) (WIP)!

Foundry-zkSync adds:

- **zkForge:** zkSync testing framework (like Truffle, Hardhat and DappTools).
- **zkCast:** Swiss army knife for interacting with zkEVM smart contracts, sending transactions and getting chain data.

Need help getting started with **Foundry-zkSync**? Read the 📖 [Usage Guides](./docs/dev/zksync/) (WIP)!

## ⚠️ Caution

Please note that `foundry-zksync` is still in its **alpha** stage. Some features might not be fully supported yet and may not work as intended. However, it is open-sourced, and contributions are welcome!

## 📊 Features & Limitations

| ✅ Features                                                                                     | 🚫 Limitations                                                         |
|------------------------------------------------------------------------------------------------|------------------------------------------------------------------------|
| Compile smart contracts with the [zksolc compiler](https://github.com/matter-labs/zksolc-bin). | Can't find `test/` directory |
| Deploy smart contracts to zkSync Era mainnet, testnet, or local test node.                     | `script` command lacks `zksolc` support.                               |
| Bridge assets L1 <-> L2.                                                                       | Cheat codes are not supported.                                         |
| Call deployed contracts on zkSync Era testnet or local test node.                              | Lacks advanced testing methods (e.g., variant testing).                |
| Send transactions to deployed contracts on zkSync Era testnet or local test node.              |                                                                        |
| Simple 'PASS / FAIL' testing.                                                                  |                                                                        |

## 📝 Prerequisites

- [Rust Compiler](https://www.rust-lang.org/tools/install)

## 💾 Installation

Each tool within our suite can be installed individually, or you can install the entire suite at once.

### Installing `zkforge` 🛠️

Run the following command:

```bash
cargo install --path ./crates/zkforge --profile local --force --locked
```

This installs `zkforge` to `~/.cargo/bin`, making it available as an executable.

### Installing `zkcast` 📡

Run the following command:

```bash
cargo install --path ./crates/zkcast --profile local --force --locked
```

This installs `zkcast` to `~/.cargo/bin`, allowing it to be used as an executable.

### Installing the Entire Suite 📦

To install all the tools in the suite:

```bash
cargo build --release
```

## Quickstart 

Run:
``` 
zkforge init --template https://github.com/dutterbutter/hello-foundry-zksync
```

Let's check out what zkforge generated for us:

```
$ cd hello-foundry-zksync
$ tree . -d -L 1
.
├── abis
├── broadcast
├── interfaces
├── lib
├── script
├── src
├── test
```

#### Compiling contracts

We can build the project with zkforge zkbuild:
```
$ zkforge zkbuild
Compiling smart contracts...
Child -> Bytecode Hash: 010000410c1f3728a3887d9bc854d978ce441ccef394319cb26c58e0ba90df46
Counter -> Bytecode Hash: 0100003bc44686be52940f3f2bd8a0feef17700663cba9edb978886c08123811
Greeter -> Bytecode Hash: 0100008f03cbc9c98bb0a883736bf9c1d8801b74928ed78148ddbd5445defddf
StepChild -> Bytecode Hash: 010000239f712c49b5804a34b1f995e034d853e2c6d2edcb60646f1bf9f057f2
Compiler run completed with warnings
TwoUserMultisig -> Bytecode Hash: 01000757a0867b6d7aba75853f126e7780bd893ae384a4718a2a03a6b53a5ee1
AAFactory -> Bytecode Hash: 0100007b76ee1ed575d19043b0b995632ac07ae996aefbbc8238f490f492c793
SimpleFactory -> Bytecode Hash: 0100021b7653e052f7f8218197d79e28de792ff243a30711fb63251644d47524
Compiled Successfully
```

#### Running Tests

You can run the tests using `zkforge test`. 

>❗Known issue of not being able to find tests in the `/tests/` directory. 

The command and its expected output are shown below:

```bash
$ zkforge test

Running 2 tests for Counter.sol:ContractBTest
[PASS] test_CannotSubtract43() (gas: 9223372034707517612)
[PASS] test_NumberIs42() (gas: 9223372034707517612)
Test result: ok. 2 passed; 0 failed; 0 skipped; finished in 43.08ms

Running 1 test for Counter.sol:OwnerUpOnlyTest
[PASS] test_IncrementAsOwner() (gas: 9223372034707517612)
Test result: ok. 1 passed; 0 failed; 0 skipped; finished in 43.46ms

Running 2 tests for Counter.sol:CounterTest
[PASS] test_Increment() (gas: 9223372034707517612)
[PASS] test_Increment_twice() (gas: 9223372034707517612)
Test result: ok. 2 passed; 0 failed; 0 skipped; finished in 47.81ms

Ran 3 test suites: 5 tests passed, 0 failed, 0 skipped (5 total tests)
```

## Configuration

### Using `foundry.toml`

Foundry is designed to be very configurable. You can configure Foundry using a file called [`foundry.toml`](./crates/config) in the root of your project, or any other parent directory. See [config package](./crates/config/README.md#all-options) for all available options.

Configuration can be arbitrarily namespaced by profiles. The default profile is named `default` (see ["Default Profile"](./crates/config/README.md#default-profile)).

You can select another profile using the `FOUNDRY_PROFILE` environment variable. You can also override parts of your configuration using `FOUNDRY_` or `DAPP_` prefixed environment variables, like `FOUNDRY_SRC`.

`zkforge init` creates a basic, extendable `foundry.toml` file.

To see your current configuration, run `zkforge config`. To see only basic options (as set with `zkforge init`), run `zkforge config --basic`. This can be used to create a new `foundry.toml` file with `zkforge config --basic > foundry.toml`.

By default `zkforge config` shows the currently selected foundry profile and its values. It also accepts the same arguments as `zkforge build`.

### DappTools Compatibility

You can re-use your `.dapprc` environment variables by running `source .dapprc` before using a Foundry tool.

### Additional Configuration

You can find additional setup and configurations guides in the [Foundry Book][foundry-book]:

-   [Setting up VSCode][vscode-setup]
-   [Shell autocompletions][shell-setup]

## Contributing

See our [contributing guidelines](./CONTRIBUTING.md).

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

## Acknowledgements

-   Foundry is a clean-room rewrite of the testing framework [DappTools](https://github.com/dapphub/dapptools). None of this would have been possible without the DappHub team's work over the years.
-   [Matthias Seitz](https://twitter.com/mattsse_): Created [ethers-solc](https://github.com/gakonst/ethers-rs/tree/master/ethers-solc/) which is the backbone of our compilation pipeline, as well as countless contributions to ethers, in particular the `abigen` macros.
-   [Rohit Narurkar](https://twitter.com/rohitnarurkar): Created the Rust Solidity version manager [svm-rs](https://github.com/roynalnaruto/svm-rs) which we use to auto-detect and manage multiple Solidity versions.
-   [Brock Elmore](https://twitter.com/brockjelmore): For extending the VM's cheatcodes and implementing [structured call tracing](https://github.com/foundry-rs/foundry/pull/192), a critical feature for debugging smart contract calls.
-   All the other [contributors](https://github.com/foundry-rs/foundry/graphs/contributors) to the [ethers-rs](https://github.com/gakonst/ethers-rs) & [foundry](https://github.com/foundry-rs/foundry) repositories and chatrooms.

[foundry-book]: https://book.getfoundry.sh
[foundry-gha]: https://github.com/foundry-rs/foundry-toolchain
[ethers-solc]: https://github.com/gakonst/ethers-rs/tree/master/ethers-solc/
[vscode-setup]: https://book.getfoundry.sh/config/vscode.html
[shell-setup]: https://book.getfoundry.sh/config/shell-autocompletion.html