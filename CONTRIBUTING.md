# Contributing to SubStream Protocol Contracts

Thank you for contributing! This document covers the development workflow, CI requirements, and how to handle dependency security issues.

---

## Development Setup

```bash
git clone https://github.com/SubStream-Protocol/SubStream-Protocol-Contracts.git
cd SubStream-Protocol-Contracts
cargo build
cargo test
```

---

## CI Requirements

Every pull request must pass:

- `cargo fmt --all -- --check` — formatting
- `cargo clippy --all-targets --all-features -- -D warnings` — linting
- `cargo test` — unit tests
- `cargo audit --deny warnings` — dependency vulnerability scan (see below)

---

## Dependency Vulnerability Scanning

We use [`cargo-audit`](https://github.com/rustsec/rustsec/tree/main/cargo-audit) to scan `Cargo.lock` against the [RustSec Advisory Database](https://rustsec.org/) on every PR. If a known CVE is found in any dependency, the pipeline fails and the PR cannot be merged.

### Running the audit locally

```bash
# Install cargo-audit (first time only)
cargo install cargo-audit --locked

# Run the audit
cargo audit
```

### Resolving a flagged vulnerability

When `cargo audit` fails, follow these steps:

**Step 1 — Identify the advisory**

The output will show the affected crate, the CVE/RUSTSEC ID, and the patched version:

```
error[RUSTSEC-2024-XXXX]: <vulnerability description>
    Crate:     some-crate
    Version:   1.2.3
    Patched:   >= 1.2.4
```

**Step 2 — Update the dependency**

If the crate is a direct dependency in `Cargo.toml`:

```bash
cargo update -p some-crate --precise 1.2.4
```

If it is a transitive dependency (pulled in by another crate), add a `[patch]` section to the workspace `Cargo.toml`:

```toml
[patch.crates-io]
some-crate = { version = "1.2.4" }
```

**Step 3 — Verify the fix**

```bash
cargo audit          # should now pass
cargo test           # ensure nothing is broken
```

**Step 4 — Commit the updated `Cargo.lock`**

```bash
git add Cargo.lock Cargo.toml
git commit -m "fix: patch RUSTSEC-2024-XXXX in some-crate"
```

### Resolving Dependabot-generated PRs

Dependabot automatically opens PRs to bump dependencies when secure versions are released. To merge them:

1. Review the diff — it should only change `Cargo.lock` (and sometimes `Cargo.toml` version constraints).
2. Run `cargo test` locally on the branch to confirm nothing breaks.
3. If tests pass and CI is green, approve and merge.

If a Dependabot bump causes a **version conflict** (two crates require incompatible versions of the same dependency):

```bash
# Check what requires the conflicting version
cargo tree -i conflicting-crate

# Try to find a compatible version range
cargo update -p conflicting-crate

# If no compatible version exists, pin with a workspace patch
# In Cargo.toml:
# [patch.crates-io]
# conflicting-crate = { version = "X.Y.Z" }
```

If the conflict cannot be resolved without breaking changes, open an issue describing the constraint and tag it `dependencies`.

---

## Submitting a Pull Request

1. Fork the repository and create a branch: `git checkout -b fix/your-description`
2. Make your changes and ensure all CI checks pass locally.
3. Open a PR against `main` with a clear description of what changed and why.
4. Reference any related issues with `Closes #<issue-number>`.
