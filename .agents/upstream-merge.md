# Upstream Merge Guide

Applies when merging changes from upstream [Foundry](https://github.com/foundry-rs/foundry) into
this fork. Full reference: `docs-zksync/upstream_merge.md`.

## Do Not

- Do not rebase onto upstream -- use squash merge for main integration (step 6).
- Do not squash individual commits in step 7 -- they must be merge commits.
- Do not adopt upstream changes to `.github/workflows/release.yml` -- preserve our custom version.
- Do not remove ZKsync telemetry code during conflict resolution (PR #1008).
- Do not keep `docker-publish.yml` if it appears from upstream -- delete it.
- Do not start a merge without verifying that commits from the previous merge (step 7) are
  already in `main`.

## Branch Naming

| Branch | Purpose |
|--------|---------|
| `upstream-<HASH>` | Main tracking branch (draft PR) |
| `upstream-<HASH>-merge` | Conflict resolution |
| `upstream-<HASH>-build` | Compilation fixes |
| `upstream-<HASH>-commits` | Individual commit incorporation |

## Steps

### 1. Select Target Commit

Choose a target commit from upstream `master` (usually the latest).

### 2. Create Tracking Branch

Create `upstream-<SHORT_HASH>` from `main`. Open a draft PR to signal work is in progress.

### 3. Resolve Merge Conflicts

Create `upstream-<SHORT_HASH>-merge`. Pull the target upstream commit. Resolve conflicts.
Open PR into the tracking branch.

**Critical during conflict resolution:**

- **Common cheatcodes**: `ExpectedCallTracker`, `RecordAccess`, `MockCallDataContext` and
  `MockCallReturnData` live in `foundry-cheatcodes-common`, not `foundry-cheatcodes`. Keep them
  there.
- **Strategy pattern**: Where upstream has inline logic that we replaced with strategy trait calls,
  implement the upstream changes in the trait implementations (both EVM and ZKsync runners), not
  inline.
- **alloy-zksync**: May need a version bump to match upstream's alloy version.
- **Telemetry**: Preserve all ZKsync telemetry code.
- **Release workflow**: Keep our `.github/workflows/release.yml`, discard upstream changes.
- **Docker**: Remove `docker-publish.yml` if it appears.

### 4. Fix Compilation Errors

Create `upstream-<SHORT_HASH>-build`. Make `cargo build` work. Open PR.

### 5. Fix CI and Tests

Fix tests, clippy errors, and `cargo deny` issues. May be one or multiple PRs.

### 6. Squash Merge into Main

Squash merge the final PR into `main`. This includes all upstream changes as a single commit.

**Gate: Confirm with user before merging.**

### 7. Incorporate Individual Commits

Create `upstream-<SHORT_HASH>-commits` from `main`. Pull upstream commits again, resolving
conflicts by picking `HEAD` (`git checkout --ours .`). The result should have the commits but
no actual changes.

**NEVER squash these.** Enable merge commits in GitHub settings (Settings > Pull Requests >
Allow Merge Commits), merge the PR, then disable merge commits again.

### 8. Post-Merge Tasks

- Update [foundry-zksync-book](https://github.com/matter-labs/foundry-zksync-book) with relevant
  changes.
- Review incorporated PRs for new cheatcodes or commands that need ZKsync support/tests.

## Anti-Patterns

- Resolving strategy-pattern conflicts by inlining ZKsync logic instead of updating trait impls.
- Accepting upstream's release workflow changes.
- Forgetting to check that previous merge's individual commits (step 7) are in `main` before
  starting a new merge.
