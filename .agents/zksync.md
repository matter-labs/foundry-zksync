# ZKsync Integration Patterns

Applies to any file with `zksync` in its path, plus `crates/strategy/zksync/`.

## Do Not

- Do not add ZKsync behavior by modifying EVM crate internals -- implement the corresponding
  strategy trait (`CheatcodeInspectorStrategyRunner`, `ExecutorStrategyRunner`, or
  `BackendStrategyRunner`). See [Forge & EVM patterns](forge-evm.md) for trait locations.
- Do not remove ZKsync telemetry code -- it is intentionally maintained. See PR #1008.
- Do not bypass the strategy pattern for forge/evm/cheatcodes ZKsync logic -- always implement
  via the strategy runners in `crates/strategy/zksync/`.
- Do not apply the strategy pattern to `cast` commands -- cast is stateless by design and uses
  dedicated `zksync.rs` modules instead (explicit architectural decision).
- Do not add ZKsync code as loose functions in a crate's `lib.rs` -- follow the established
  convention of a sibling `zksync.rs` file or `zksync/` subdirectory.
- Do not move shared cheatcode types into `foundry-cheatcodes` -- they belong in
  `foundry-cheatcodes-common` (`crates/cheatcodes/common/`) so `foundry-zksync-core` can use them.

## Where ZKsync Code Lives

ZKsync-specific code is in **two locations**: dedicated crates and modules inside other crates.

### Dedicated ZKsync Crates

```
crates/zksync/
  core/               VM integration, state, cheatcode tracers, type conversions
  compilers/          zksolc compiler, dual compilation (EVM + ZKsync), linking
  inspectors/         trace collection (wraps Foundry's TraceInspector)

crates/strategy/zksync/
  cheatcode/          CheatcodeInspectorStrategyRunner (runner + context)
  executor/           ExecutorStrategyRunner
  backend/            BackendStrategyRunner
```

### ZKsync Modules Inside Other Crates

When modifying these, also read the parent crate's patterns:

- `crates/cast/src/zksync.rs` -- `ZkCast` wrapper, `ZkTransactionOpts`
- `crates/cast/src/cmd/{call,send,mktx,estimate}/zksync.rs` -- cast subcommand ZKsync paths
- `crates/forge/src/cmd/{create,inspect}/zksync.rs` -- forge subcommand ZKsync paths
- `crates/script/src/build/zksync.rs` -- ZKsync script compilation
- `crates/verify/src/verify/zksync.rs` + `src/etherscan/zksync.rs` + `src/zksync/` -- verification
- `crates/linking/src/zksync.rs` -- ZKsync library linking
- `crates/config/src/zksync.rs` -- `ZkSyncConfig` struct (compile, startup, zksolc settings)
- `crates/test-utils/src/zksync.rs` -- test utilities

## Transaction Execution Architecture

When `forge test` or `forge script` runs in ZKsync mode:

```
revm call/create hooks
  │
  ▼
Cheatcodes inspector intercepts transaction
  │
  ▼
ZksyncCheatcodeInspectorStrategyRunner
  ├── zksync_try_call()    dispatches call to ZKsync VM
  └── zksync_try_create()  dispatches create to ZKsync VM
        │
        ▼
      ZKsync VM (crates/zksync/core/src/vm/)
        ├── CheatcodeTracer (implements DynTracer)
        ├── Storage translations (EVM ↔ ZKsync: balance, nonce, code)
        └── Result conversion back to revm format
              │
              ▼
            revm opcode execution skipped (result already computed)
```

**Storage translations** convert EVM storage layout to ZKsync equivalents. Key helpers in
`crates/zksync/core/src/`: `get_balance_key()`, `get_nonce_key()`, `compute_create_address()`,
`compute_create2_address()`. This translation runs once per VM switch in both directions.

**Nonces**: ZKsync uses separate transaction and deployment nonces. See the
[book](https://foundry-book.zksync.io/zksync-specifics/developer-guide/nonces) for details.

## Dual Compilation

`crates/zksync/compilers/` handles compiling contracts for both EVM and ZKsync targets.
`DualCompiledContracts` (`crates/zksync/compilers/src/dual_compiled_contracts.rs`) pairs EVM and
ZKsync artifacts for the same source.

The zksolc compiler settings flow: `foundry.toml` -> `ZkSyncConfig` -> `ZkSolcSettings` ->
`ZkSolcVersionedInput` (sanitized for compiler version compatibility). When compiler options are
renamed across zksolc versions, keep aliases for backward compatibility. Use `Option` types in
`ZkSettings`/`ZkSolcInput` and populate old names during sanitization for older zksolc versions.
Canonical reference: `docs-zksync/compiler_input_flow.md`.

## Cast Commands (Exception to Strategy Pattern)

Cast subcommands are stateless one-off operations. They do NOT use the strategy pattern. Instead,
ZKsync logic lives in dedicated modules alongside the standard implementation:

- Standard: `crates/cast/src/cmd/call/mod.rs`
- ZKsync:   `crates/cast/src/cmd/call/zksync.rs`

The main entry point checks the mode and dispatches accordingly. When adding a new cast command
with ZKsync support, create a sibling `zksync.rs` file and invoke it from the main command.
Canonical reference: `docs-zksync/architecture/architecture.md` (cast section).

## Canonical References

- Strategy runners: `crates/strategy/zksync/src/cheatcode/runner/mod.rs`
- VM execution: `crates/zksync/core/src/vm/inspect.rs`
- Cheatcode tracer: `crates/zksync/core/src/vm/tracers/cheatcode.rs`
- Dual compilation: `crates/zksync/compilers/src/dual_compiled_contracts.rs`
- ZKsync config: `crates/config/src/zksync.rs`
- Architecture: `docs-zksync/architecture/architecture.md`
- Compiler flow: `docs-zksync/compiler_input_flow.md`
- Compiler updates: `docs-zksync/compiler_update.md`
