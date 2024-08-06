## Foundry with zkSync Era v0.2-alpha

This repository enhances Foundry to support zkSync Era, enabling Solidity-based compilation, deployment, testing, and interaction with smart contracts on zkSync Era.

> 🔧 **Fork Notice:** This is a Foundry fork with added zkSync support.
> 
> ⚠️ **Alpha Stage:** The project is in alpha, so you might encounter issues. For more information, please review [Limitations](#limitations) section.
> 
> 🐞 **Found an Issue?** Please report it to help us improve.

### Changes Made

To use for zkSync environments, include `--zksync` when running `forge` or `vm.zkVm(true)` in tests. The modifications include:

1. **Compilation:** `solc` and `zksolc` are used for compiling. The resulting bytecodes are combined into `DualCompiledContract` and managed through `Executor` to `CheatcodeTracer`.
2. **EVM Interactions:**
   - EVM calls are standard except for `address.balance` and `block.timestamp`/`block.number`, which pull data from zkSync (ZK-storage and ZK-specific context, respectively).
3. **Transaction Handling:**
   - `CALL` and `CREATE` operations are captured and converted to zkSync transactions. This process includes fetching zkSync-equivalent bytecode, managing account nonces, and marking EOA appropriately to comply with zkSync requirements.
4. **Execution and State Management:**
   - zkSync VM processes the transaction and returns state changes, which are applied to `journaled_state`. Results are relayed back.
5. **Logging:**
   - `console.log()` outputs within zkSync VM are captured and displayed in Foundry.
   - `ZK_DEBUG_RESOLVE_HASHES` and `ZK_DEBUG_SHOW_OUTPUTS` env variable may be set to `true` to display zkSync VM call logs with resolved selector hashes (requires Internet connection), and the call outputs, respectively.
6. **Fuzzing**
   - Adds config option `no_zksync_reserved_addresses`. Since zkSync reserves addresses below 2^16 as system addresses, a fuzz test would've required a broad `vm.assume` and many `vm.excludeSender` calls to exclude these. This is not only cumbersome but could also trigger `proptest`'s global `max_global_rejects` failure. When this option is set to `true` the `proptest` generation itself ensures that no invalid addresses are generated, and thus need not be filtered adding up to the `max_test_rejects` count.

## 📊 Features & Limitations

### Features

`Foundry-zksync` offers a set of features designed to work with zkSync Era, providing a comprehensive toolkit for smart contract deployment and interaction:

- **Smart Contract Deployment**: Easily deploy smart contracts to zkSync Era mainnet, testnet, or a local test node.
- **Contract Interaction**: Call and send transactions to deployed contracts on zkSync Era testnet or local test node.
- **Solidity Testing**: Write tests in Solidity, similar to DappTools, for a familiar testing environment.
- **Fuzz Testing**: Benefit from fuzz testing, complete with shrinking of inputs and printing of counter-examples.
- **Remote RPC Forking**: Utilize remote RPC forking mode, leveraging Rust's asynchronous infrastructure like tokio.
- **Flexible Debug Logging**: Choose your debugging style:
  - DappTools-style: Utilize DsTest's emitted logs for debugging.
  - Hardhat-style: Leverage the popular console.sol contract.
- **Configurable Compiler Options**: Tailor compiler settings to your needs, including LLVM optimization modes.

Forge is quite fast at both compiling (leveraging [ethers-solc]) and testing.

### Limitations

While `foundry-zksync` is **alpha stage**, there are some limitations to be aware of:

- **Compile Time**: Some users may experience slower compile times.
- **Compiling Libraries**: Compiling non-inlinable libraries requires deployment and adding to configuration or the command line with the `--libraries` argument. For more information please refer to [official docs](https://docs.zksync.io/build/developer-reference/ethereum-differences/libraries).

    ```
    libraries = [
        "src/MyLibrary.sol:MyLibrary:0xfD88CeE74f7D78697775aBDAE53f9Da1559728E4"
    ]
    ```
- **Create2 Address Derivation**: There are differences in Create2 Address derivation compared to Ethereum. [Read the details](https://docs.zksync.io/build/developer-reference/ethereum-differences/evm-instructions#create-create2).
- **Contract Verification**: Currently contract verification via the `--verify` flag do not work as expected but will be added shortly.  
- **Specific Foundry Features**: Currently features such as `--gas-report`, `--coverage` may not work as intended. We are actively working on providing support for these feature types.
- **Solc Compatibility**: `zksolc` requires a `solc` binary to be run as a child process. The version/path to use for each can be specified by the `zksolc` and `solc` options in `foundry.toml`. Not all `solc` versions are supported by all `zksolc` versions, compiling with a `solc` version higher than the one supported may lead to unexpected errors. [Read the docs](https://docs.zksync.io/zk-stack/components/compiler/toolchain/solidity.html#limitations) about version limitations and check the [zksolc changelog](https://github.com/matter-labs/era-compiler-solidity/blob/main/CHANGELOG.md) to see the latest supported `solc` version.
- **Windows Compatibility**: Windows is not officially supported yet. The reported issues would be investigated on a best-effort basis. 

For the most effective use of our library, we recommend familiarizing yourself with these features and limitations.

## Quick Install

Follow these steps to quickly install the binaries for `foundry-zksync`:

**Note:** This installation overrides any existing forge and cast binaries in ~/.foundry. You can use forge without the --zksync flag for standard EVM chains. To revert to a previous installation, follow the instructions [here](https://book.getfoundry.sh/getting-started/installation#using-foundryup).

1. **Clone the Repository**:
   Begin by cloning the `foundry-zksync` repository from GitHub. This will download the latest version of the source code to your local machine.

   ```bash
   git clone git@github.com:matter-labs/foundry-zksync.git
   ```

2. **Change Directory**:
   Navigate into the directory where the repository has been cloned. This is where you will run the installation commands.

   ```bash
   cd foundry-zksync
   ```

3. **Run the Installer**:
   Now, you're ready to execute the installation script. This command initializes the setup and installs `foundry-zksync` binaries `forge` and `cast`.

   ```bash
   ./install-foundry-zksync
   ```

4. **Verify the Installation** (Recommended):
   After installation, it's good practice to verify that the binaries have been installed correctly. Run the following command to check the installed version:

   ```bash
   forge --version
   ```

This should return the installed version of `forge`, confirming that `foundry-zksync` is installed properly on your system.

## 📝 Development Prerequisites

Ensure you have the necessary tools and environments set up for development:

1. **Install Rust:** Use the command below to install Rust. This will also install `cargo`, Rust's package manager and build system.

   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Set Rust to Nightly Toolchain:** This project requires Rust's nightly version from September 30, 2023. Set your Rust toolchain to the appropriate nightly version using `rustup`:

   ```sh
   # Replace `<YOUR-TARGET>` with your specific platform target, e.g., `x86_64-unknown-linux-gnu`
   rustup install nightly-2023-09-30-<YOUR-TARGET>
   
   # Example for MacOS (Apple Silicon):
   rustup install nightly-2023-09-30-aarch64-apple-darwin
   ```

3. **MacOS Prerequisite - Install `make`:** MacOS users need to ensure `make` is installed. If not, install it using Homebrew:

   ```sh
   brew install make
   ```

   Then, set the path to GNU `make`:

   ```sh
   # For x86_64 MacOs users:
   # export PATH="/usr/local/opt/make/libexec/gnubin:$PATH"
   export PATH="/opt/homebrew/opt/make/libexec/gnubin:$PATH"
   ```

   Add this export line to your shell profile (`~/.bash_profile`, `~/.zshrc`, etc.) to make the change permanent.

## 💾 Installation

Each tool within our suite can be installed individually, or you can install the entire suite at once.

### Installing `forge` 🛠️

To install `forge`, execute the command below. This action will overwrite any previous `forge` installations, but the functionality remains consistent. Post-installation, `forge` will be accessible as an executable from `~/.cargo/bin`.

Run the following command:

```bash
cargo install --path ./crates/forge --profile local --force --locked
```

### Installing `cast` 📡

Similarly, to install `cast`, use the following command. Like `forge`, this will replace any existing `cast` installations without altering functionality. Once installed, `cast` becomes available as an executable in `~/.cargo/bin`.

Run the following command:

```bash
cargo install --path ./crates/cast --profile local --force --locked
```

### Installing the Entire Suite 📦

To install all the tools in the suite:

```bash
cargo build --release
```

## Quickstart 

In an empty directory, run the following command:
``` 
forge init
```

Let's check out what forge generated for us:

```
$ tree . -d -L 1
.
├── lib
├── script
├── src
└── test
```

#### Compiling contracts

We can build the project with `forge build --zksync`:
```
$ forge build --zksync
Compiling smart contracts...
Compiled Successfully
```

#### Listing missing libraries

To scan missing non-inlinable libraries, you can build the project using the `--zk-detect-missing-libraries-flag`. This will give a list of the libraries that need to be deployed and their addresses
provided via the `libraries` option for the contracts to compile.
Metadata about the libraries will be saved in `.zksolc-libraries-cache/missing_library_dependencies.json`.


#### Running Tests

You can run the tests using `forge test --zksync`. 

The command and its expected output are shown below:

```bash
$ forge test --zksync

Ran 2 tests for test/Counter.t.sol:CounterTest
[PASS] testFuzz_SetNumber(uint256) (runs: 256, μ: 8737, ~: 8737)
[PASS] test_Increment() (gas: 8702)
Suite result: ok. 2 passed; 0 failed; 0 skipped; finished in 3.57s (3.56s CPU time)

Ran 1 test suite in 3.57s (3.57s CPU time): 2 tests passed, 0 failed, 0 skipped (2 total tests)
```

## Configuration

### Using `foundry.toml`

Foundry is designed to be very configurable. You can configure Foundry using a file called [`foundry.toml`](./crates/config) in the root of your project, or any other parent directory. See [config package](./crates/config/README.md#all-options) for all available options.

Configuration can be arbitrarily namespaced by profiles. The default profile is named `default` (see ["Default Profile"](./crates/config/README.md#default-profile)).

You can select another profile using the `FOUNDRY_PROFILE` environment variable. You can also override parts of your configuration using `FOUNDRY_` or `DAPP_` prefixed environment variables, like `FOUNDRY_SRC`.

`forge init` creates a basic, extendable `foundry.toml` file.

To see your current configuration, run `forge config`. To see only basic options (as set with `forge init`), run `forge config --basic`. This can be used to create a new `foundry.toml` file with `forge config --basic > foundry.toml`.

By default `forge config` shows the currently selected foundry profile and its values. It also accepts the same arguments as `forge build`. An example `foundry.toml` for zkSync with zksolc configurations may look like:

```
[profile.default]
src = 'src'
out = 'out'
libs = ['lib']

[profile.default.zksync]
compile = true
fallback_oz = true
mode = '3'
```

### Additional Configuration

You can find additional setup and configurations guides in the [Foundry Book][foundry-book]:

-   [Setting up VSCode][vscode-setup]
-   [Shell autocompletions][shell-setup]

## Contributing

See our [contributing guidelines](./CONTRIBUTING.md).

## Acknowledgements

-   Foundry is a clean-room rewrite of the testing framework [DappTools](https://github.com/dapphub/dapptools). None of this would have been possible without the DappHub team's work over the years.
-   [Matthias Seitz](https://twitter.com/mattsse_): Created [ethers-solc] which is the backbone of our compilation pipeline, as well as countless contributions to ethers, in particular the `abigen` macros.
-   [Rohit Narurkar](https://twitter.com/rohitnarurkar): Created the Rust Solidity version manager [svm-rs](https://github.com/roynalnaruto/svm-rs) which we use to auto-detect and manage multiple Solidity versions.
-   [Brock Elmore](https://twitter.com/brockjelmore): For extending the VM's cheatcodes and implementing [structured call tracing](https://github.com/foundry-rs/foundry/pull/192), a critical feature for debugging smart contract calls.
-   All the other [contributors](https://github.com/foundry-rs/foundry/graphs/contributors) to the [ethers-rs](https://github.com/gakonst/ethers-rs) & [foundry](https://github.com/foundry-rs/foundry) repositories and chatrooms.

### Acknowledgments - foundry-zksync
- [Moonsong Labs](https://moonsonglabs.com/): Implemented [zkSync support](./crates/zksync/), and resolved a number of different challenges to enable zkSync support. 

[foundry-book]: https://book.getfoundry.sh
[foundry-gha]: https://github.com/foundry-rs/foundry-toolchain
[ethers-solc]: https://github.com/gakonst/ethers-rs/tree/master/ethers-solc/
[vscode-setup]: https://book.getfoundry.sh/config/vscode.html
[shell-setup]: https://book.getfoundry.sh/config/shell-autocompletion.html
