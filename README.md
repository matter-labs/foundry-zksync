## Foundry with ZKsync Era v0.2-alpha

This repository enhances Foundry to support ZKsync, enabling Solidity-based compilation, deployment, testing, and interaction with smart contracts on ZKsync.

> 🔧 **Fork Notice:** This is a Foundry fork with added zkSync support.
> 
> ⚠️ **Alpha Stage:** The project is in alpha, so you might encounter issues.
> 
> 🐞 **Found an Issue?** Please report it to help us improve by opening an issue or submitting a pull request.
>
> 📚 **Foundry ZKsync Book:** For detailed information, including installation instructions, usage examples, and advanced guides, please refer to the [Foundry ZKsync Book](https://foundry-book.zksync.io/).

## Quick Install

Follow these steps to quickly install the binaries for `foundry-zksync`:

**Note:** This installation overrides any existing `forge` and `cast` binaries in `~/.foundry`. You can use `forge` without the `--zksync` flag for standard EVM chains. To revert to a previous installation, follow the instructions [here](https://book.getfoundry.sh/getting-started/installation#using-foundryup).

### 1. **Run the Installer**

Run the following `curl` command, which downloads and runs the installation script:

```bash
curl -L https://raw.githubusercontent.com/matter-labs/foundry-zksync/main/install-foundry-zksync | bash
```

This command will download the latest `foundry-zksync` binaries (`forge` and `cast`) and set them up on your system.

### 2. **Verify the Installation** (Recommended)

After installation, it's recommended to verify that the binaries have been installed correctly. Run the following command to check the installed version:

```bash
forge --version
```

This should return the installed version of `forge`, confirming that `foundry-zksync` is installed properly on your system.

## Quickstart 

In an empty directory, run the following command:

```bash 
forge init
```

Let's check out what forge generated for us:

```bash
$ tree . -d -L 1
.
├── lib
├── script
├── src
└── test
```

#### Compiling contracts

We can build the project with `forge build --zksync`:

```bash
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
forge test --zksync

Ran 2 tests for test/Counter.t.sol:CounterTest
[PASS] testFuzz_SetNumber(uint256) (runs: 256, μ: 8737, ~: 8737)
[PASS] test_Increment() (gas: 8702)
Suite result: ok. 2 passed; 0 failed; 0 skipped; finished in 3.57s (3.56s CPU time)

Ran 1 test suite in 3.57s (3.57s CPU time): 2 tests passed, 0 failed, 0 skipped (2 total tests)
```

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

## Configuration

### Using `foundry.toml`

Foundry is designed to be very configurable. You can configure Foundry using a file called [`foundry.toml`](./crates/config) in the root of your project, or any other parent directory. See [config package](./crates/config/README.md#all-options) for all available options.

Configuration can be arbitrarily namespaced by profiles. The default profile is named `default` (see ["Default Profile"](./crates/config/README.md#default-profile)).

You can select another profile using the `FOUNDRY_PROFILE` environment variable. You can also override parts of your configuration using `FOUNDRY_` or `DAPP_` prefixed environment variables, like `FOUNDRY_SRC`.

`forge init` creates a basic, extendable `foundry.toml` file.

To see your current configuration, run `forge config`. To see only basic options (as set with `forge init`), run `forge config --basic`. This can be used to create a new `foundry.toml` file with `forge config --basic > foundry.toml`.

By default `forge config` shows the currently selected foundry profile and its values. It also accepts the same arguments as `forge build`. An example `foundry.toml` for zkSync with zksolc configurations may look like:

```bash
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
