---
id: "012-renderers"
title: "The renderers: two views, one shell, read-only over one truth"
status: approved
created: "2026-08-11"
depends_on:
  - "002-dual-view-architecture"
  - "008-simulation-substrate"
  - "009-cargo-workspace"
  - "010-simulation-core"
  - "011-event-bus"
establishes:
  - "docs/design/renderers.md"
  - { kind: directory, path: "crates/icebeek-render-exterior/" }
  - { kind: directory, path: "crates/icebeek-render-interior/" }
  - { kind: directory, path: "crates/icebeek-app/" }
summary: >
  The contract for the presentation layer: crates/icebeek-render-exterior
  (the Macro, isometric 3D), crates/icebeek-render-interior (the Micro,
  top-down cross-section), and crates/icebeek-app (the one binary and
  the host). The shared rules make spec 008 requirement 5 structural:
  render crates read simulation state through queries and push typed
  commands, never mutating state directly; presentation entities are
  never serialized; every visual interpolates between the last two sim
  ticks. The app owns the window, the fixed-timestep driver that ticks
  the sim, input capture into commands, save/load orchestration, and
  the single fast camera/audio crossfade between views. The sim ticks
  regardless of which view is focused: the unfocused renderer idles,
  the simulation never does. Crate directory edges are added when the
  crates land; until then this spec owns its design brief.
---

# 012: The renderers

## 1. Purpose

Specs 002 and 008 require two presentations over one simulation with
neither owning gameplay data; specs 009 and 010 give the presentation
layer its crates and its read-only boundary. This spec is the contract
for what those three crates do: the shared presentation rules, each
view's rendering responsibilities, and the app shell that hosts
everything.

## 2. Shared presentation rules

Binding on both render crates:

1. **Read-only over the one truth.** Render systems read simulation
   state through `bevy_ecs` queries and the sim crate's read API.
   The only write path out of a renderer is pushing typed player
   commands into the sim's command queue (spec 010 §6). A render
   system that mutates simulation state directly is a spec violation
   regardless of whether it compiles.
2. **Presentation state is disposable.** Cameras, meshes, sprites,
   UI trees, animation timers, and audio handles live in the render
   crates and are never serialized. Deleting every presentation
   entity and rebuilding from simulation state must reproduce the
   same picture; saves contain no presentation data (spec 010 §7).
3. **Interpolate, never extrapolate.** Visuals interpolate between
   the last two completed sim ticks (spec 010 §3). Renderers never
   predict future state and never read frame time into gameplay.
4. **Full Bevy is allowed here.** The render crates and the app are
   the only crates that may depend on the `bevy` umbrella (spec 009
   §3.3, spec 010 §2 forbids it in the sim).

## 3. The exterior renderer (the Macro)

`icebeek-render-exterior` renders what spec 002 §2 assigns the Macro:
terrain and ice topography, the ship silhouette and gross motion,
weather presentation, expedition deployment, prow impacts, and intake
events. Isometric 3D. Its audio palette is the exterior one: wind,
grinding ice, hull groans. It presents the Fog of Winter from the
world domain's reveal state (spec 006 §2) and may observe the event
queue to drive effects (an impact flash at the struck node), but the
queue remains sim-owned (spec 011).

## 4. The interior renderer (the Micro)

`icebeek-render-interior` renders the cross-section: decks, rooms,
the logistics spine (belts, pipes, data lines), drones in motion,
machine states, and a heat overlay over the thermal field (spec 004
§4 and §5). Top-down. Its audio palette is the interior one:
machinery hum, belt rhythm. It renders no horizon, terrain, or hull
exterior (spec 002 §2).

## 5. The app shell

`icebeek-app` is the host and the only binary (spec 009 §3.4),
producing the executable named `icebeek`:

1. **Owns the Bevy `App`**: window, schedules, plugin wiring for
   both render crates.
2. **Drives the sim.** The fixed-timestep accumulator of spec 010 §3
   lives here: the app calls `tick()` at TICK_HZ regardless of frame
   rate and regardless of which view is focused. The unfocused
   view's render systems idle; the simulation never does (spec 002
   §3.4: neither view blocks the other).
3. **Captures input** and translates it into typed commands; no
   input handler reaches into simulation state directly.
4. **Owns the view switch**: a single deliberate camera/audio
   crossfade, fast enough to be used constantly (spec 002 §2). The
   switch is pure presentation; it enqueues nothing and blocks
   nothing in the sim.
5. **Orchestrates save/load** using the sim crate's save contract
   (spec 010 §7), including pause-for-io if needed; a paused sim is
   an app-level state, never a sim-internal one.

## 6. Test contract

Render crates are hard to exercise without a GPU; the enforceable
surface is structural:

1. **Workspace discipline.** `cargo tree` shows no `bevy` umbrella
   dependency under `icebeek-sim` and no dependency in either
   direction between the two render crates (spec 009 §3).
2. **Headless survival.** The workspace test suite (spec 013) runs
   on machines with no display; render-crate tests are compile-time
   and logic-only (interpolation math, command construction), never
   windowed.
3. **Rebuild property.** The presentation-state-is-disposable rule
   (section 2.2) is exercised by a test that despawns all
   presentation entities and asserts a rebuild from simulation state
   succeeds.

## 7. Territory

This spec owns `crates/icebeek-render-exterior/`,
`crates/icebeek-render-interior/`, and `crates/icebeek-app/` once
they exist; the directory edges are added by the PRs that create the
crates (the I-004 pattern of spec 009 §5), and each crate manifest
carries `[package.metadata.spec-spine] spec = "012-renderers"` from
its first commit. Until then this spec owns
`docs/design/renderers.md`. If the app shell outgrows this spec, it
splits by amending the spec 009 §5 table and this spec together.

## 8. Out of scope

- Art direction, style targets, asset formats, audio middleware:
  future specs (spec 008 §4).
- Camera feel, control bindings, UI layout: playable-prototype
  iteration; this spec fixes boundaries, not aesthetics.
- The sim-side read API shape: spec 010.
- CI wiring that runs the structural tests: spec 013.
