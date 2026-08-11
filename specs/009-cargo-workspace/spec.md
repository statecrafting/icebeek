---
id: "009-cargo-workspace"
title: "Cargo workspace layout: crates, boundaries, and toolchain"
status: approved
created: "2026-08-11"
depends_on:
  - "008-simulation-substrate"
establishes:
  - "docs/design/workspace-layout.md"
  - "Cargo.toml"
  - "rust-toolchain.toml"
summary: >
  The first code-facing spec. It fixes the Cargo workspace shape that
  the Bevy decision (spec 008) implies: a virtual workspace root, five
  crates under crates/ (icebeek-events, icebeek-sim,
  icebeek-render-exterior, icebeek-render-interior, icebeek-app), and
  the dependency rules between them that make the spec 008 hard
  requirements structural: the event vocabulary knows no engine, the
  simulation core carries no renderer, the two render crates never
  touch each other, and the binary crate is the only place everything
  meets. It also pins the toolchain policy (pinned stable via
  rust-toolchain.toml, edition 2024, workspace-level dependency
  versions) and assigns the crate territory that specs 010 through 013
  will claim. No code lands under this spec; it is the layout the
  later specs build inside.
---

# 009: Cargo workspace layout

## 1. Purpose

Spec 008 decided the stack (Bevy, Rust) and bound three consequences on
the workspace shape: a pure simulation crate with no renderer
dependency, a typed serde-serializable event crate shared by both
views, and two render layers that read shared state without owning
gameplay data. This spec turns those consequences into a concrete
Cargo workspace: crate names, directory layout, the dependency edges
that are allowed to exist, and the toolchain policy. Everything a later
spec builds must fit inside this shape; changing the shape means
amending this spec first.

## 2. Workspace shape

The repository root becomes a virtual Cargo workspace (a root
`Cargo.toml` with a `[workspace]` table and no `[package]`). All
crates live under `crates/`; no Rust code exists outside it.

```
Cargo.toml                       workspace root (virtual manifest)
rust-toolchain.toml              pinned stable toolchain
crates/
  icebeek-events/                typed event vocabulary and queue     (spec 011)
  icebeek-sim/                   simulation core, shared state model  (spec 010)
  icebeek-render-exterior/       the Macro view: isometric 3D         (spec 012)
  icebeek-render-interior/       the Micro view: cross-section        (spec 012)
  icebeek-app/                   the one binary, named `icebeek`      (spec 012)
```

The view crates are named for what they render (exterior, interior),
not for the in-house view names, because a crate called
`icebeek-macro` would read as a Rust proc-macro crate.

## 3. Crate boundary rules

The dependency graph is a strict DAG, and these are the only edges:

```
icebeek-events <- icebeek-sim <- icebeek-render-exterior <- icebeek-app
                              <- icebeek-render-interior <- icebeek-app
```

1. **`icebeek-events` knows no engine.** It depends on serde (and the
   standard library) and nothing heavier. It defines the typed event
   vocabulary of the spec 002 coupling contract: hull node, magnitude,
   timestamp. No Bevy dependency of any kind, ever.
2. **`icebeek-sim` carries no renderer.** It depends on
   `icebeek-events` and must not depend on Bevy's render, window,
   asset, or audio machinery. `cargo test -p icebeek-sim` runs on a
   machine with no display. Whether the core uses `bevy_ecs` alone or
   stays engine-free entirely is decided in spec 010, not here; this
   spec only forbids the renderer surface.
3. **Render crates never touch each other.** `icebeek-render-exterior`
   and `icebeek-render-interior` each depend on Bevy, `icebeek-sim`,
   and `icebeek-events`. Neither depends on the other, and neither
   writes gameplay state; they read it through the simulation crate's
   public API (spec 008 requirement 5).
4. **`icebeek-app` is the only binary.** It wires the simulation
   schedule and both render layers, owns the view-switch shell (the
   camera/audio crossfade of spec 002 §2), and produces the binary
   named `icebeek`. It is the only crate that depends on both render
   crates.
5. **Spec linkage travels in the manifest.** Every crate's
   `Cargo.toml` carries `[package.metadata.spec-spine] spec = "..."`
   naming its owning spec, so the coupling gate resolves ownership
   from the manifest without per-file headers.

## 4. Toolchain and workspace policy

- **Pinned stable.** `rust-toolchain.toml` pins a specific stable
  release (channel `"1.xx"`, not `"stable"`), bumped deliberately by
  commit, never by whatever a contributor has installed.
- **Edition 2024** for every crate.
- **One version per dependency.** Shared dependencies (Bevy, serde)
  are declared once in `[workspace.dependencies]` at the root; member
  crates reference them with `workspace = true`. A member crate never
  pins its own version of a shared dependency.
- **Warnings are CI's job.** Crates do not carry `#![deny(warnings)]`;
  the strictness lives in the CI invocation (spec 013) so local
  iteration stays fast.

## 5. Territory plan for specs 010 through 013

This spec owns the layout rule itself and, once they exist, the
workspace-level files (`Cargo.toml`, `rust-toolchain.toml`). An
approved spec may not claim files that do not exist yet (the indexer
rejects it), so those two `establishes` edges are added to this spec
by the first implementation PR that creates the files. The crate
directories are claimed by the specs that fill them:

| spec | claims |
|------|--------|
| 010-simulation-core | `crates/icebeek-sim/` |
| 011-event-bus | `crates/icebeek-events/` |
| 012-renderers | `crates/icebeek-render-exterior/`, `crates/icebeek-render-interior/`, `crates/icebeek-app/` |
| 013-ci | `.github/workflows/` additions for the Rust build |

If spec 012 grows too large, the app shell may split into its own
spec; that split amends this table.

No code lands under this spec. The root manifests are created by the
first implementation PR (spec 010 at the earliest), which lands after
this spec and its successors pass their human checkpoints.

## 6. Out of scope

- Crate internals: state model and tick schedule (010), event schema
  (011), render architecture (012).
- CI pipeline content and caching strategy (013).
- `.cargo/config.toml` build ergonomics (fast linker, shared target
  dir): decided alongside CI in 013 if wanted.
- Asset pipeline, art formats, audio middleware: future specs.
