# Forge & EVM Patterns

Applies to `crates/forge/`, `crates/evm/`, `crates/cheatcodes/`, `crates/script/`, `crates/anvil/` --
the Rust testing framework, EVM execution engine, and local node.

## Do Not

- Do not add inline ZKsync logic in these crates -- implement via strategy traits in
  `crates/strategy/zksync/`. See [ZKsync patterns](zksync.md) for details.
- Do not use `println!`/`eprintln!` -- use `sh_println!`/`sh_eprintln!` from `foundry_common::shell`.
  Clippy enforces this via `clippy.toml`.
- Do not add shared cheatcode types (expect, mock, record) to `foundry-cheatcodes` -- add them to
  `foundry-cheatcodes-common` (`crates/cheatcodes/common/`) so `foundry-zksync-core` can import them.
- Do not write forking tests without "fork" in the test name -- the CI filter relies on this.
- Do not use `dbg!()` -- workspace clippy lints warn on `dbg-macro`.
- Do not use `std::print*`/`std::eprint*` macros directly -- same clippy restriction as above.

## Architecture

```
Binaries
  crates/{forge,cast,anvil,chisel}/bin/main.rs
    │
CLI / Commands
  crates/cli/                     shared opts, handlers, utils
  crates/forge/src/cmd/           forge subcommands
  crates/cast/src/cmd/            cast subcommands
    │
Application Layer
  crates/forge/                   MultiContractRunner, test execution
  crates/script/                  script runner, broadcast recording
  crates/anvil/                   local node (EthApi, backend, pool, miner)
    │
EVM Layer
  crates/evm/evm/                 Executor, inspectors
  crates/evm/core/                Backend, DatabaseExt, fork support
  crates/evm/traces/              trace decoding, identification
  crates/evm/fuzz/                fuzzer, invariant testing
  crates/evm/coverage/            coverage collection + analysis
    │
Cheatcodes
  crates/cheatcodes/              Cheatcodes inspector (strategy dispatch point)
  crates/cheatcodes/common/       shared types (ExpectedCallTracker, MockCallData, RecordAccess)
  crates/cheatcodes/spec/         cheatcode Solidity interface specs
    │
  revm                            external EVM implementation
```

**Responsibility boundaries:**

- **Binaries** -- entry points only. No business logic.
- **CLI/Commands** -- argument parsing, config loading via `figment`, output formatting. No EVM logic.
- **Application** -- orchestrates test/script execution. Owns `MultiContractRunner`, `ScriptRunner`.
- **EVM Layer** -- execution, tracing, fuzzing, coverage. Does not know about CLI or ZKsync.
- **Cheatcodes** -- the `Cheatcodes` struct implements revm's inspector trait. It dispatches to
  strategy runners for behavior that diverges between EVM and ZKsync.

## Strategy Pattern

The strategy pattern is the core architectural decision for ZKsync integration. Three traits define
the extension points. EVM crates contain the trait definitions; ZKsync implementations live in
`crates/strategy/zksync/`.

| Trait | Definition | Purpose |
|-------|-----------|---------|
| `CheatcodeInspectorStrategyRunner` | `crates/cheatcodes/src/strategy.rs` | Cheatcode behavior dispatch |
| `ExecutorStrategyRunner` | `crates/evm/evm/src/executors/strategy.rs` | Execution strategy |
| `BackendStrategyRunner` | `crates/evm/core/src/backend/strategy.rs` | Database/backend operations |

Each strategy has a **stateless runner** (can be freely cloned) and a **stateful context** (clone
only for fresh/non-persistent operations). Canonical reference:
`docs-zksync/architecture/architecture.md`.

## Test Patterns

- Unit tests live alongside source code in `#[cfg(test)]` modules.
- Integration tests live in `tests/` directories within each crate.
- Test data: `testdata/` (standard Foundry), `testdata_zk/` (ZKsync-specific).
- Use `cargo nextest run -p <crate>` to run tests for a single crate.
- Exclude CI-filtered tests locally: `-E 'kind(test) & !test(/\b(issue|ext_integration)/)'`.
- Forking tests must contain "fork" in their name.

## Cheatcodes

The `Cheatcodes` inspector (`crates/cheatcodes/src/inspector.rs`, ~2600 lines) intercepts revm hooks
(`call`, `call_end`, `create`, etc.). When behavior must diverge for ZKsync:

1. The inspector calls `self.strategy.runner.<method>(self.strategy.context.as_mut(), ...)`.
2. EVM implementation lives in `crates/cheatcodes/` itself (the default runner).
3. ZKsync implementation lives in `crates/strategy/zksync/src/cheatcode/`.

To add a new cheatcode: look at `crates/cheatcodes/spec/` for the Solidity interface, then implement
the handler. If the cheatcode needs ZKsync-specific behavior, add a strategy method.

## Anvil

`crates/anvil/` is the local Ethereum node (upstream Foundry's Anvil). It uses an async tokio runtime.
Key types: `EthApi` (wraps backend, pool, signers, miner), `NodeHandle` (lifecycle management).
Do not confuse with `anvil-zksync` which is a separate project.

## Forge Scripts

Scripts (`crates/script/`) execute in three stages: initial execution, simulation, broadcast.
Transactions are recorded via `vm.startBroadcast()`/`vm.stopBroadcast()` cheatcodes. ZKsync scripts
encode additional EIP-712 fields (factory_deps, paymaster_params) via alloy's `OtherFields`.
Canonical reference: `docs-zksync/architecture/architecture.md` (forge script section).
