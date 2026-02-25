# Agent Guide Design Decisions

This document explains why the agent guide system is structured the way it is.

## Progressive Disclosure

`AGENTS.md` (~80 lines) is loaded into every agent conversation. Domain guides (`.agents/*.md`)
load only when the agent touches the relevant code area. This keeps base token cost low while
ensuring deep context is available on demand.

If `AGENTS.md` contained everything, it would cost thousands of tokens per interaction for context
that's irrelevant to most tasks.

## Single Source of Truth

All agent guidance lives in `AGENTS.md` + `.agents/`. Tool-specific files are thin redirects:

- `CLAUDE.md` -> redirects to `AGENTS.md`
- Future `.cursorrules`, `.github/copilot-instructions.md`, etc. -> same redirect pattern

No content is duplicated across tool-specific files. When guidance changes, update one place.

## Guardrails Over Guidelines

Rules use "Do NOT" and "NEVER" instead of "prefer" and "consider." Agents follow prohibitions
more reliably than suggestions. Every "Do Not" rule traces to an actual mistake an agent would
make or a pattern that looks correct but is wrong in this codebase:

- Adding inline ZKsync logic instead of using strategy traits (looks correct, breaks upstream merges)
- Using `println!` instead of `sh_println!` (compiles fine, fails clippy)
- Adding shared types to `foundry-cheatcodes` instead of `foundry-cheatcodes-common` (works, but
  creates circular dependency with `foundry-zksync-core`)
- Applying strategy pattern to cast commands (consistent, but wrong -- cast is intentionally stateless)

## Canonical References

Domain guides point to real code files instead of embedding examples:

- "Canonical reference: `crates/cheatcodes/src/strategy.rs`"
- NOT: "Here's how the strategy pattern works: ```rust ... ```"

Embedded code examples go stale. The codebase is always up to date. Agents can read the referenced
file to see the current implementation.

## Maintenance

- **New crate added** -> update repository map in `AGENTS.md`
- **New convention** -> add to relevant domain guide, not `AGENTS.md` (unless cross-cutting)
- **Agent mistake observed** -> add "Do Not" rule to the relevant domain guide
- **New AI tool support** -> create entry-point file redirecting to `AGENTS.md`
- **Upstream merge changes architecture** -> update `forge-evm.md` and `zksync.md` as needed
- **New repetitive workflow** -> consider adding `.agents/skills/<name>/SKILL.md`
