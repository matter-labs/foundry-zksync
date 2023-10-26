# zkforge: Command Guide for Compilation and Deployment

### Compilation with `zkforge zk-build`

**Aliases:** `zkforge zkbuild`, `zkforge zk-compile`, `zkforge zkb`.

**Function:** Compiles smart contracts into zkEVM bytecode, outputting files into a structured directory `<PROJECT-ROOT>/zkout/`.

**Usage:**

```sh
zkforge zk-build [OPTIONS]
```

**Options:**

- `--use-zksolc`: Specify zksolc compiler version (default if left blank).
- `--is-system`: Enables system contract compilation mode.
- `--force-evmla`: Forces the EVM legacy assembly pipeline.
- `-h, --help`: Prints help.

**Note:** The `--is-system` flag is essential for contracts like factory contracts. These should be in `src/is-system/`. Create the folder if it doesn't exist.

![System Contracts Folder](https://user-images.githubusercontent.com/76663878/236301037-2a536ab0-3d09-44f3-a74d-5f5891af335b.png)

**Example Usage:**

Compile with default compiler options (v1.3.11).

```sh
zkforge zk-build
```

**Compiler Settings:**

Set `zksolc` compiler version using `--use` flag.

```bash
zkforge zkb --use 0.8.19
```

**Example Output:**

`zksolc` compiler artifacts location:

```bash
<PROJECT-ROOT>/zkout/<CONTRACT_FILENAME>
```
![Compiler Artifacts](https://user-images.githubusercontent.com/76663878/234152279-e144e489-41ab-4cbd-8321-8ccd9b0aa6ef.png)

Example terminal output:

![Terminal Output](https://user-images.githubusercontent.com/76663878/236305625-8c7519e2-0c5e-492f-a4bc-3b019a95e34f.png)

**Important:** Until `forge remappings` are implemented, use relative import paths:

![Import Paths](https://github.com/matter-labs/foundry-zksync/assets/76663878/490b34f4-e286-42a7-8570-d4b228ec10c7)

`SimpleFactory.sol` and `AAFactory.sol` should be in `src/is-system/`.

---

### Deployment with `zkforge zk-create`

**Aliases:** `zkforge zkcreate`, `zkforge zk-deploy`, `zkforge zkc`

**Function:** Deploys smart contracts to zksync.

**Usage:**

```sh
zkforge zk-create <CONTRACT> [OPTIONS] --rpc-url <RPC-URL> --chain <CHAIN-ID> --private-key <PRIVATE-KEY>
```

**Options:**

- `-h, --help`: Prints help.
- `--constructor-args <ARGS>...`: Constructor arguments.
- `--constructor-args-path <FILE>`: File path containing constructor arguments.
- `<CONTRACT>`: Contract identifier in `<path>:<contractname>` form.
- `--factory-deps <FACTORY-DEPS>...`: Factory dependencies in `<path>:<contractname>` form.

**Example:**

Deploy `src/Greeter.sol` to zkSync testnet:

```bash
zkforge zkc src/Greeter.sol:Greeter --constructor-args "ZkSync + Pineapple" --private-key <"PRIVATE_KEY"> --rpc-url https://zksync2-testnet.zksync.dev:443 --chain 280
```

**Output:**

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

## Deploy and Interact with `SimpleFactory.sol`

### Compile `SimpleFactory.sol`

**Note:** Compile with the `is-system` flag; place in `src/is-system/`.

```bash
zkforge zk-build
```

### Deploy `SimpleFactory.sol`

```sh
zkforge zkc src/SimpleFactory.sol:SimpleFactory --constructor-args 01000041691510d85ddfc6047cba6643748dc028636d276f09a546ab330697ef 010000238a587670be26087b7812eab86eca61e7c4014522bdceda86adb2e82f --factory-deps src/Child.sol:Child src/StepChild.sol:StepChild --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --rpc-url http://localhost:3050 --chain 270
```

**Output:**

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
zkcast zk-send 0x23cee3fb585b1e5092b7cfb222e8e873b05e9519 "newStepChild()" --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```

**Output:**

```sh
Sending transaction....
Transaction Hash: 0xa82a0636b71af058d4916d81868eebc41173ca07b78d30fe57f4b74e9294ef25
```