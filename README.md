# Foundry with zkSync Era v0.1-alpha

This repository provides [Foundry](https://github.com/foundry-rs/foundry) functionality in Solidity for compiling, deploying, testing, and interacting with smart contracts on **zkSync Era**. 

**What is foundry?**

Foundry is a blazing fast, portable and modular toolkit for Ethereum application development written in Rust.

Foundry consists of:

- **Forge:** Ethereum testing framework (like Truffle, Hardhat and DappTools).
- **Cast:** Swiss army knife for interacting with EVM smart contracts, sending transactions and getting chain data.
- **Anvil:** Local Ethereum node, akin to Ganache, Hardhat Network.
- **Chisel:** Fast, utilitarian, and verbose solidity REPL.

Need help getting started with Foundry? Read the 📖 [Foundry Book](https://book.getfoundry.sh/) (WIP)!

### Foundry-zkSync adds:

- **zkForge:** zkSync testing framework (like Hardhat and DappTools).
- **zkCast:** Swiss army knife for interacting with zkEVM smart contracts, sending transactions and getting chain data.

Need help getting started with **Foundry-zkSync**? Read the 📖 [Usage Guides](./docs/dev/zksync/) (WIP)!

## ⚠️ Caution

Please note that `foundry-zksync` is still in its **alpha** stage. Some features might not be fully supported yet and may not work as intended. However, it is open-sourced, and contributions are welcome!

## 📊 Features & Limitations

### Features

`Foundry-zksync` offers a set of features designed to work with zkSync Era, providing a comprehensive toolkit for smart contract deployment and interaction:

- **Smart Contract Deployment**: Easily deploy smart contracts to zkSync Era mainnet, testnet, or a local test node.
- **Asset Bridging**: Bridge assets between L1 and L2, facilitating seamless transactions across layers.
- **Contract Interaction**: Call and send transactions to deployed contracts on zkSync Era testnet or local test node.
- **Solidity Testing**: Write tests in Solidity, similar to DappTools, for a familiar testing environment.
- **Fuzz Testing**: Benefit from fuzz testing, complete with shrinking of inputs and printing of counter-examples.
- **Remote RPC Forking**: Utilize remote RPC forking mode, leveraging Rust's asynchronous infrastructure like tokio.
- **Flexible Debug Logging**: Choose your debugging style:
  - DappTools-style: Utilize DsTest's emitted logs for debugging.
  - Hardhat-style: Leverage the popular console.sol contract.
- **Configurable Compiler Options**: Tailor compiler settings to your needs, including LLVM optimization modes.

# Limitations

While `foundry-zksync` is **alpha stage**, there are some limitations to be aware of:

- **Cheat Codes Support**: Not all cheat codes are fully supported. [View the list of supported cheat codes](./SUPPORTED_CHEATCODES.md).
- **Compile Time**: Some users may experience slower compile times.
- **Compiling Libraries**: Compiling non-inlinable libraries requires deployment and adding to configuration. For more information please refer to [official docs](https://era.zksync.io/docs/tools/hardhat/compiling-libraries.html).

    ```
    libraries = [
        "src/MyLibrary.sol:MyLibrary:0xfD88CeE74f7D78697775aBDAE53f9Da1559728E4"
    ]
    ```
- **Create2 Address Derivation**: There are differences in Create2 Address derivation compared to Ethereum. [Read the details](https://era.zksync.io/docs/reference/architecture/differences-with-ethereum.html#create-create2).
- **Specific Foundry Features**: Currently features such as `--gas-report` may not work as intended. We are actively working on providing support for these feature types.

For the most effective use of our library, we recommend familiarizing yourself with these features and limitations. 

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
zkforge init hello_foundry
```

Let's check out what zkforge generated for us:

```
$ cd hello_foundry
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
Compiled Successfully
```

#### Running Tests

You can run the tests using `zkforge test`. 

The command and its expected output are shown below:

```bash
$ zkforge test

Running 2 tests for Counter.t.sol:CounterTest
[PASS] testFuzz_SetNumber(uint256) (runs: 256, μ: 9223372034707527035, ~: 9223372034707527076)
[PASS] test_Increment() (gas: 9223372034707527339)
Test result: ok. 2 passed; 0 failed; 0 skipped; finished in 5.15s

Ran 1 test suites: 2 tests passed, 0 failed, 0 skipped (2 total tests)
```

## Configuration

### Using `foundry.toml`

Foundry is designed to be very configurable. You can configure Foundry using a file called [`foundry.toml`](./crates/config) in the root of your project, or any other parent directory. See [config package](./crates/config/README.md#all-options) for all available options.

Configuration can be arbitrarily namespaced by profiles. The default profile is named `default` (see ["Default Profile"](./crates/config/README.md#default-profile)).

You can select another profile using the `FOUNDRY_PROFILE` environment variable. You can also override parts of your configuration using `FOUNDRY_` or `DAPP_` prefixed environment variables, like `FOUNDRY_SRC`.

`zkforge init` creates a basic, extendable `foundry.toml` file.

To see your current configuration, run `zkforge config`. To see only basic options (as set with `zkforge init`), run `zkforge config --basic`. This can be used to create a new `foundry.toml` file with `zkforge config --basic > foundry.toml`.

By default `zkforge config` shows the currently selected foundry profile and its values. It also accepts the same arguments as `zkforge build`. An example `foundry.toml` for zkSync with zksolc configurations may look like:

```
[profile.default]
src = 'src'
out = 'out'
libs = ['lib']

[profile.zksync]
src = 'src'
libs = ['lib']
fallback_oz = true
is_system = true
mode = "2"
```

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

## Acknowledgements

-   Foundry is a clean-room rewrite of the testing framework [DappTools](https://github.com/dapphub/dapptools). None of this would have been possible without the DappHub team's work over the years.
-   [Matthias Seitz](https://twitter.com/mattsse_): Created [ethers-solc](https://github.com/gakonst/ethers-rs/tree/master/ethers-solc/) which is the backbone of our compilation pipeline, as well as countless contributions to ethers, in particular the `abigen` macros.
-   [Rohit Narurkar](https://twitter.com/rohitnarurkar): Created the Rust Solidity version manager [svm-rs](https://github.com/roynalnaruto/svm-rs) which we use to auto-detect and manage multiple Solidity versions.
-   [Brock Elmore](https://twitter.com/brockjelmore): For extending the VM's cheatcodes and implementing [structured call tracing](https://github.com/foundry-rs/foundry/pull/192), a critical feature for debugging smart contract calls.
-   All the other [contributors](https://github.com/foundry-rs/foundry/graphs/contributors) to the [ethers-rs](https://github.com/gakonst/ethers-rs) & [foundry](https://github.com/foundry-rs/foundry) repositories and chatrooms.

### Acknowledgments - foundry-zksync
- [Moonsong Labs](https://moonsonglabs.com/): Implemented [era-cheatcodes](./crates/era-cheatcodes/), and resolved a number of different challenges to enable zkSync support. 

[foundry-book]: https://book.getfoundry.sh
[foundry-gha]: https://github.com/foundry-rs/foundry-toolchain
[ethers-solc]: https://github.com/gakonst/ethers-rs/tree/master/ethers-solc/
[vscode-setup]: https://book.getfoundry.sh/config/vscode.html
[shell-setup]: https://book.getfoundry.sh/config/shell-autocompletion.html