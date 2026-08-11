# Rust CI

<!-- Normative authority: specs/013-ci/spec.md -->

Two gates on every pull request once code exists:

- **govern** (`spec-spine.yml`, spec 000): compile, staleness, lint,
  couple. Keeps the corpus coherent; runs on every PR.
- **rust** (`rust.yml`, spec 013): format check, clippy with warnings
  denied, the headless workspace test suite, workspace build. Keeps
  the code honest; exists once the Cargo workspace does.

The points that matter:

- The strictness spec 009 kept out of the crates lives in the clippy
  invocation (`-D warnings`); lints are silenced in code with a
  justified `#[allow]`, never by loosening CI.
- Runners have no GPU, which is the point: the sim's replay
  determinism and save/load equivalence tests (spec 010 §8) run on
  every PR, so a determinism regression is a red build.
- Toolchain comes from `rust-toolchain.toml`, no version matrix.
  Dependency caching is mandatory and fails open.
- Landing rule: `rust.yml` ships in the same PR that creates the
  Cargo workspace root, so Rust code is gated from its first commit,
  and `rust` joins `govern` as a required status check.

Territory: the workflow file is claimed when it lands, with a
`# Spec: 013-ci` header; until then spec 013 owns this brief.
