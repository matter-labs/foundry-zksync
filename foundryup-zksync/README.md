# `foundryup-zksync`

Update or revert to a specific Foundry-zksync branch with ease.

`foundryup` supports installing and managing multiple versions.

## Installing

<!-- TODO: update to reference curl link once available -->

```sh
./install.sh
```

## Usage

To install the **nightly** version:

```sh
foundryup-zksync
```

To **install** a specific **version** (in this case the `nightly` version):

```sh
foundryup-zksync --version nightly
```

To install a specific **branch** (in this case the `release/0.0.2-alpha.3` branch's latest commit):

```sh
foundryup-zksync --branch release/0.0.2-alpha.3
```

To install a **fork's main branch** (in this case `transmissions11/foundry`'s main branch):

```sh
foundryup-zksync --repo transmissions11/foundry
```

To install a **specific branch in a fork** (in this case the `patch-10` branch's latest commit in `transmissions11/foundry`):

```sh
foundryup-zksync --repo transmissions11/foundry --branch patch-10
```

To install from a **specific Pull Request**:

```sh
foundryup-zksync --pr 1071
```

To install from a **specific commit**:

```sh
foundryup-zksync -C 94bfdb2
```

To install a local directory or repository (e.g. one located at `~/git/foundry`, assuming you're in the home directory)

#### Note: --branch, --repo, and --version flags are ignored during local installations.

```sh
foundryup-zksync --path ./git/foundry
```

---

**Tip**: All flags have a single character shorthand equivalent! You can use `-i` instead of `--install`, etc.

---

## Uninstalling

Foundry contains everything in a `.foundry` directory, usually located in `/home/<user>/.foundry/` on Linux, `/Users/<user>/.foundry/` on MacOS and `C:\Users\<user>\.foundry` on Windows where `<user>` is your username.

To uninstall Foundry remove the `.foundry` directory.

#### Warning ⚠️: .foundry directory can contain keystores. Make sure to backup any keystores you want to keep.

Remove Foundry from PATH:

- Optionally Foundry can be removed from editing shell configuration file (`.bashrc`, `.zshrc`, etc.). To do so remove the line that adds Foundry to PATH:

```sh
export PATH="$PATH:/home/user/.foundry/bin"
```
