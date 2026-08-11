---
id: "008-simulation-substrate"
title: "Simulation substrate: tick model, event bus, and the stack decision"
status: approved
created: "2026-08-10"
depends_on:
  - "002-dual-view-architecture"
establishes:
  - "docs/design/stack-decision.md"
summary: >
  The first implementation-facing spec. It pins the technical
  requirements any stack must satisfy (a deterministic fixed-tick
  interior simulation that runs headless, a typed Macro-to-Micro event
  bus, save states as pure serialized state, and a two-renderer
  presentation over one simulation) and records the stack decision
  taken at the 2026-08-11 human checkpoint: the game is built on Bevy
  (Rust). The simulation core is authored as a pure, renderer-free
  crate; the two views are Bevy render layers over that one crate.
  Implementation specs (009 onward) claim the concrete crate territory
  via establishes edges as they are authored.
---

# 008: Simulation substrate

## 1. Purpose

Convert the architectural guarantees of spec 002 into engineering
requirements, and record the engine/stack decision as an explicit human
checkpoint rather than an accident of the first commit.

## 2. Hard requirements on any candidate stack

1. **Deterministic fixed-tick core.** The interior simulation advances
   on a fixed tick, independent of frame rate, with no wall-clock or
   render-order dependence. Same state plus same event queue yields
   identical outcomes (spec 002 §4).
2. **Headless simulation.** The Micro simulation must run without its
   renderer attached (while the player is in the Macro view, and in
   automated tests). Simulation and presentation are separate modules
   with a one-way state-read boundary.
3. **Typed event bus.** Macro-to-Micro communication is a queue of
   typed, serializable events (hull node, magnitude, timestamp) per
   the spec 002 coupling contract. Events are data; no callbacks
   across the view boundary.
4. **Saves are state.** A save file is the serialized shared state
   plus pending event queue, nothing else. Loading a save and
   replaying the same inputs reproduces the same run segment.
5. **Two renderers, one truth.** Isometric 3D exterior and top-down
   cross-section interior render from the same shared state; neither
   renderer owns gameplay data.

## 3. The decision: Bevy (Rust)

Taken at the 2026-08-11 human checkpoint (maintainer selection; the
weighing record lives in `docs/design/stack-decision.md`). The game is
built on **Bevy (Rust)**.

Rationale, in the order the criteria were weighed:

- The ECS architecture makes the hard requirements of section 2
  structural rather than disciplinary: a fixed-tick schedule over a
  renderer-free simulation crate is the natural Bevy shape, not an
  imposed convention.
- Late-game factory scale is the long-term performance risk, and Rust
  plus ECS is the strongest answer available to it.
- The maintainer toolchain is already Rust-native (spec-spine itself,
  the surrounding workspace), so contributor ergonomics favor Rust
  despite Bevy's younger editor tooling.

The accepted cost: both renderers (isometric 3D exterior, top-down
cross-section interior) are built by hand rather than configured in an
editor, so the first playable arrives later than it would on Godot.

Consequences for the workspace shape (binding on specs 009 onward):

1. The interior simulation is a pure crate with no Bevy renderer
   dependency, exercised headless in tests (section 2, requirements
   1 and 2).
2. The Macro-to-Micro event bus is a typed, serde-serializable queue
   crate shared by both views (requirement 3).
3. The two views are Bevy render/UI layers reading the shared state;
   neither owns gameplay data (requirement 5).

Implementation specs (repo layout and Cargo workspace, simulation
core, event bus, the two renderers, CI) are authored next, each
claiming its crate territory via `establishes` edges.

## 4. Out of scope

- Gameplay content of any kind (owned by specs 003 through 007).
- Art pipeline, asset formats, audio middleware: future specs after
  the stack decision.
