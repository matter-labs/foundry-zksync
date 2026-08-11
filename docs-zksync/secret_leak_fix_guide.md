# 🔑 Secret Leak Fix Guide During Upstream Merges

During upstream merges, it's common to encounter commits that unintentionally leak secrets, such as API keys, private tokens, or other sensitive strings. While many of these may be public and not an actual security threat, our CI pipeline integrates with TruffleHog to detect and block any such leaks.

If you're reading this, it's likely because a secret (probably an API key) from an upstream commit triggered this CI check and is blocking your PR.

This guide describes how to fix these issues by rewriting the git history.

---

## ⚠️ Context

- **CI Job**: Our CI setup blocks PRs that contain commits introducing secrets.
- **Trigger**: Usually happens during an upstream merge when a public secret exists in one or more commits.

---

## 🧰 Resolution Strategy

We have two main strategies:

### ✅ 1. Dropping the Problematic Commit
If the commit that introduces the secret is trivial or already fixed in a later commit, it can be dropped.

#### Steps

```bash
git rebase -i origin/main
```

- In the interactive UI, change `pick` to `drop` for the offending commit:

```bash
drop abc123 commit with leaked secret
pick def456 fixed the secret leak
```

Then:

```bash
git push --force
```

---

### 🛠️ 2. Editing the Commit and preserving useful changes
If the commit has both valuable code and a secret leak, edit it instead.

#### Steps

```bash
git rebase -i origin/main
```

- In the interactive menu:

```bash
edit abc123 commit with leaked secret + valid changes
```

- Git will pause. Now fix the leaked secret in the file:

```bash
vim path/to/leaked_file.ext
git add path/to/leaked_file.ext
git rebase --continue
```

Then:

```bash
git push --force
```

---

## 🧾 Summary Table

| Situation                             | Action         |
|--------------------------------------|----------------|
| Only leaked secret in commit            | `drop`         |
| Useful changes + leaked secret          | `edit` & fix   |
| Final step after fixing              | `git push --force` |

---

## 📌 Notes

- Prefer editing over dropping when unsure.
- Always force-push after rebase to update the remote.
- Ping the members of the team that might be affected by the changes.

---

For full context on our merge process, see the [Foundry Upstream Merge Update Guide](./foundry-upstream-merge-guide.md).

