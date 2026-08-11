---
id: "013-ci"
title: "Rust CI: the workspace gate beside the governance gate"
status: approved
created: "2026-08-11"
depends_on:
  - "009-cargo-workspace"
  - "010-simulation-core"
  - "012-renderers"
establishes:
  - "docs/design/ci.md"
summary: >
  The contract for the Rust CI workflow at .github/workflows/rust.yml,
  which runs beside the existing governance gate (spec 000's
  spec-spine.yml) on every pull request. It pins the four checks
  (cargo fmt --check, clippy over the whole workspace with warnings
  denied, the headless workspace test suite, and a workspace build),
  the toolchain source (rust-toolchain.toml, no version matrix),
  mandatory dependency caching, and the landing rule: the workflow
  file lands in the same PR that creates the Cargo workspace root, so
  Rust code is gated from its first commit. The strictness that spec
  009 kept out of the crates (warnings deny) lives in the clippy
  invocation here, and the determinism replay test of spec 010 runs on
  every PR, making a determinism regression a red build. The workflow
  file edge is added when the file lands; until then this spec owns
  its design brief.
---

# 013: Rust CI

## 1. Purpose

The governance gate (spec 000's `spec-spine.yml`) keeps the corpus
coherent; nothing yet gates the code the corpus authorizes. This spec
defines the Rust workflow that does: what it checks, where its
strictness comes from, and when it lands. Specs 009 §4 (warnings are
CI's job), 010 §8 (headless test contract), and 012 §6 (structural
tests) each name CI as their enforcement point; this spec is that
point.

## 2. The workflow

`.github/workflows/rust.yml`, triggered on `pull_request` like the
governance gate, one job, four checks in order:

1. **Format.** `cargo fmt --check`. No style debates in review;
   the formatter is the style.
2. **Clippy.** `cargo clippy --workspace --all-targets -- -D
   warnings`. This invocation is where the spec 009 §4 strictness
   lives: crates carry no `#![deny(warnings)]`, CI denies instead.
   Silencing a lint happens in code with an `#[allow]` and a
   justification comment, never by loosening this invocation.
3. **Test.** `cargo test --workspace`, headless. GitHub runners have
   no GPU or display, which is exactly the point: the sim's test
   contract (spec 010 §8, including replay determinism and save/load
   equivalence) and the renderers' structural tests (spec 012 §6)
   must pass here. A determinism regression is a red build on the PR
   that introduces it.
4. **Build.** `cargo build --workspace` so binary targets that tests
   do not link (the `icebeek` binary) still compile on every PR.

## 3. Toolchain and caching

- **Toolchain from the repo, not the runner.** The workflow installs
  the toolchain that `rust-toolchain.toml` pins (spec 009 §4). No
  version matrix; the game ships one toolchain, CI checks that one.
- **Dependency caching is mandatory.** Bevy dependency graphs are
  heavy; the workflow caches the cargo registry and build artifacts
  keyed on the lockfile and the pinned toolchain, so a routine PR
  runs in minutes, not tens of minutes. Cache misses must fail open
  (a cold run is slow, never wrong).

## 4. Landing rule

The workflow file lands in the same PR that creates the Cargo
workspace root: a `cargo` workflow on a repo with no `Cargo.toml`
fails vacuously, and an ungated first code PR is exactly what spec
discipline forbids. That PR also:

- adds this spec's `.github/workflows/rust.yml` establishes edge
  (the I-004 pattern of spec 009 §5), with a `# Spec: 013-ci` header
  comment in the file itself;
- adds the deferred workspace-file edges of spec 009;
- registers the `rust` job as a required status check beside
  `govern` in the repository merge rules (an operator action noted
  in the PR).

## 5. Both gates, disjoint jobs

`spec-spine.yml` (govern) and `rust.yml` stay separate workflows:
the governance gate runs on every PR including docs-and-specs-only
changes, while the Rust gate exists only once the workspace does.
Neither subsumes the other; a PR that touches code and corpus must
pass both. This spec owns only `rust.yml`; the governance workflow
remains spec 000 territory.

## 6. Out of scope

- Release, packaging, and distribution pipelines (itch/Steam
  artifacts, signing): future specs.
- Platform matrix builds (macOS, Windows) and nightly/scheduled
  jobs: future amendment when a second platform becomes a target.
- The governance workflow content: spec 000.
- Repository merge-rule administration beyond the one registration
  named in section 4.
