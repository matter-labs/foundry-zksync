# Updating zksolc in Foundry-ZKsync: Step-by-step guide

This guide outlines the steps and considerations for updating the `zksolc` compiler version in Foundry-ZKsync. When a new `zksolc` version is released, making it available in Foundry-ZKsync involves more than simply bumping the version number. **Backward compatibility must be carefully considered.** 

For more information about the defined compiler policy, you can check the [Foundry-ZKsync compiler section](https://foundry-book.zksync.io/zksync-specifics/compilation-overview?highlight=policy#compiler-support-policy). 

Additionally, when performing a compiler update, it's crucial to understand how Foundry settings impact the input the compiler ultimately receives. For more details about the flow, you can check the [compiler flow](compilerFlow.md)

### Process Overview
The process requires careful attention to backward compatibility, version support, and configuration changes. Below are the detailed steps to ensure a smooth compiler update:
1. **Add New Versions**:
    - In [`crates/zksync/compilers/src/compilers/zksolc/mod.rs`](https://github.com/matter-labs/foundry-zksync/blob/3f8025f53f2c4cffe6ac4b43a3e20d4ebf993c6e/crates/zksync/compilers/src/compilers/zksolc/mod.rs#L415), add the new `zksolc` version to the list of supported versions. This will make it the default (latest) version.
2. **Review Release Notes and Version Commits**:
    - Consult the `zksolc` release notes ([era-compiler-solidity/releases](https://github.com/matter-labs/era-compiler-solidity/releases)) and examine all version commits to identify potential breaking changes not explicitly mentioned in the release notes.
3. **Check Supported `solc` Versions**:
    - Verify the new `solc` versions supported by `zksolc` and confirm the existence of the corresponding `era-compiler-solc` fork in [era-solidity/releases](https://github.com/matter-labs/era-solidity/releases).
    - Add the supported `solc` version in [`crates/zksync/compilers/src/compilers/zksolc/mod.rs`](https://github.com/matter-labs/foundry-zksync/blob/3f8025f53f2c4cffe6ac4b43a3e20d4ebf993c6e/crates/zksync/compilers/src/compilers/zksolc/mod.rs#L443).
4. **Incorporate New Compiler Options**:
    - In the [`settings.rs`](https://github.com/matter-labs/foundry-zksync/blob/main/crates/zksync/compilers/src/compilers/zksolc/settings.rs) file, add any new compiler options. Ensure the settings accommodate all supported compilers and include optional ad-hoc sanitizing when building `ZkSolcInput` from the settings ([`input.rs`](https://github.com/matter-labs/foundry-zksync/blob/3f8025f53f2c4cffe6ac4b43a3e20d4ebf993c6e/crates/zksync/compilers/src/compilers/zksolc/input.rs#L126)).

### Testing and Submission

1. **Run Foundry Test Suite**:
    - Execute the complete Foundry test suite to ensure all tests pass. Pay particular attention to the compiler integration tests, which include checks for all supported versions ([`zksync_tests.rs`](https://github.com/matter-labs/foundry-zksync/blob/3f8025f53f2c4cffe6ac4b43a3e20d4ebf993c6e/crates/zksync/compilers/tests/zksync_tests.rs#L59)). Add new tests for any discovered edge cases.
2. **Submit a PR**:
    - Create a pull request with all the changes.

### Handling Specific Changes: Examples
Some scenarios arise in the past where particular attention was needed and that might be relevant to future updates.

- **New Error Types**:
    - If a new error type is added (e.g., `Ripemd160` Error as seen in [era-compiler-solidity/pull/276/files](https://github.com/matter-labs/era-compiler-solidity/pull/276/files)), incorporate the new error type into the `ErrorType` settings enum.
- **Option Renaming**:
    - When options are renamed (e.g., `fallbackToOptimizingForSize` to `sizeFallback` and `LLVMOptions` to `llvmOptions` as seen in [era-compiler-solidity/commit/62f0ce7dc6c5050e3a464a6b453d493e07384f53](https://github.com/matter-labs/era-compiler-solidity/commit/62f0ce7dc6c5050e3a464a6b453d493e07384f53)), rename the option in the config and CLI, while keeping an alias for the old one. In `ZkSettings/ZkSolcInput`, maintain both values as `Option`. Use `sizeFallback` internally and populate `fallbackToOptimizingForSize` during input sanitization for `zksolc` versions that require it. The same applies to `llvmOptions`.

### Examples
For further clarification, you can review these compiler update examples:

1. [feat: add assemblycreate for warning suppression for zksolc 1.5.10](https://github.com/matter-labs/foundry-zksync/pull/840)
2. [feat: Add support for zksolc 1.5.12](https://github.com/matter-labs/foundry-zksync/pull/1002)