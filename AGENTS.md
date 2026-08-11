# Foundry ZKsync -- Agent Guide

Foundry ZKsync is a ZKsync Era fork of Foundry for smart contract compilation, deployment,
testing, and interaction. This is a **Rust** monorepo (Cargo workspace) using `make` for automation.

## Repository Map

```
crates/
  forge/              Test framework + CLI commands          (bin)
  cast/               Contract interaction CLI               (bin)
  anvil/              Local Ethereum node                    (bin)
  chisel/             Solidity REPL                          (bin)
  cli/                Shared CLI infrastructure
  script/             Forge script runner
  evm/
    core/             EVM backend, database, fork support
    evm/              Executor + inspectors
    traces/           Execution tracing + decoding
    fuzz/             Fuzzing + invariant testing
    coverage/         Code coverage
    abi/              Solidity ABI utilities
    networks/         Custom network features
  cheatcodes/         Cheatcode inspector + strategy dispatch
    common/           Shared types (expect, mock, record)
    spec/             Cheatcode specifications
  zksync/
    core/             ZKsync VM, state, cheatcode tracers
    compilers/        zksolc integration, dual compilation
    inspectors/       ZKsync trace collection
  strategy/zksync/    Strategy runners (cheatcode, executor, backend)
  config/             Configuration (foundry.toml + ZkSyncConfig)
  common/             Shared utilities, shell macros
  verify/             Contract verification
  linking/            Library linking
  primitives/         Core types
testdata/             Standard Foundry test data
testdata_zk/          ZKsync-specific test data
docs-zksync/          ZKsync architecture + process docs
.agents/              Agent guides
```

## Commands

- `cargo +nightly clippy -p <crate> -- -D warnings` -- lint a single crate
- `cargo +nightly fmt` -- format Rust
- `cargo nextest run -p <crate>` -- test a single crate
- `make lint` -- format + clippy + typos (whole workspace)
- `make test` -- all unit + doc tests
- `make pr` -- full pre-push check (cargo deny + lint + test)
- `make build` -- build the project

## Domain Guides

- Touching `crates/forge/`, `crates/evm/`, `crates/cheatcodes/`, `crates/script/`, or `crates/anvil/`? Read [Forge & EVM patterns](.agents/forge-evm.md) first.
- Touching any file with `zksync` in its path? Read [ZKsync patterns](.agents/zksync.md) first.
- Performing an upstream Foundry merge? Read [Upstream merge guide](.agents/upstream-merge.md) first.

## Agent Workflow

1. Plan changes and identify files you will touch. Ask for clarification if needed.
2. Read the relevant domain guide (see pointers above).
3. Check existing patterns in similar files before writing new code.
4. Implement with minimal diff -- change only what's necessary, don't refactor adjacent code.
5. Per modified crate: `cargo +nightly clippy -p <crate> -- -D warnings` + `cargo nextest run -p <crate>`.
6. Before committing: `make pr`. Verify no secrets are being committed.
7. Update docs if behavior changed.

## Conventions

- **Commits** -- conventional commits: `feat:`, `fix:`, `chore:`, `refactor:`.
- **Output** -- use `sh_println!`/`sh_eprintln!`, never `println!`/`eprintln!` (clippy enforces this).
- **ZKsync notes** -- prefix with `NOTE(zk):` when explaining divergence from upstream Foundry.
- **AI disclosure** -- disclose AI assistance in PRs per `CONTRIBUTING.md`.
- **Dependencies** -- never add a new dep without checking if an existing crate covers the need.
- **Forking tests** -- test names must contain "fork" if they use forking.
