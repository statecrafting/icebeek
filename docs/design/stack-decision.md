# Decision record: engine and stack

<!-- Normative authority: specs/008-simulation-substrate/spec.md -->

**Status: DECIDED, 2026-08-11. The game is built on Bevy (Rust).**

Any candidate had to satisfy the hard requirements of spec 008: a
deterministic fixed-tick interior simulation that runs headless, a
typed serializable event bus between the views, saves as pure state,
and two renderers over one simulation.

The weighing:

| Candidate | For | Against |
|-----------|-----|---------|
| **Bevy (Rust), selected** | ECS fits deterministic tick and headless sim naturally; strongest correctness story; maintainer toolchain is Rust-native | Youngest tooling; both renderers built by hand; slowest first playable |
| Godot (+ C#/Rust ext.) | Mature editor; 2D and 3D scene tooling covers both views out of the box | Determinism and headless discipline must be imposed deliberately |
| Web (TypeScript + PixiJS/Three.js) | Fastest prototyping and distribution | Performance ceiling for late-game factories |

Decision criteria, in order: prototype speed for validating the core
loop, late-game simulation performance, contributor ergonomics. Bevy
loses the first criterion and wins the second and third; the second is
the one that cannot be retrofitted.

- [x] Maintainer selects the stack: **Bevy (Rust)** (2026-08-11)
- [x] Spec `008-simulation-substrate` amended with the decision and approved
- [ ] Implementation specs authored (Cargo workspace layout, simulation
      core crate, event bus crate, the two renderers, CI), each
      claiming territory via `establishes` edges
