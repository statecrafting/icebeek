# Workspace layout

<!-- Normative authority: specs/009-cargo-workspace/spec.md -->

One virtual Cargo workspace, five crates, a strict dependency DAG:

```
crates/
  icebeek-events            typed event vocabulary; serde only, no engine
  icebeek-sim               simulation core; no renderer dependency, tests run headless
  icebeek-render-exterior   the Macro view (isometric 3D exterior)
  icebeek-render-interior   the Micro view (top-down cross-section interior)
  icebeek-app               the one binary (`icebeek`); wires sim + both views
```

```
icebeek-events <- icebeek-sim <- icebeek-render-exterior <- icebeek-app
                              <- icebeek-render-interior <- icebeek-app
```

The rules that matter:

- Events know no engine; the sim carries no renderer; the two render
  crates never depend on each other; only the app binary sees
  everything.
- Toolchain is a pinned stable release in `rust-toolchain.toml`;
  shared dependency versions live once in `[workspace.dependencies]`.
- Each crate's manifest names its owning spec via
  `[package.metadata.spec-spine] spec = "..."`.

Crate territory is claimed by the specs that fill it: 010 the sim
core, 011 the event bus, 012 the renderers and app shell, 013 CI.
