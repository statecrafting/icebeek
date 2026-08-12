---
id: "010-simulation-core"
title: "Simulation core: one crate of truth, fixed tick, determinism discipline"
status: approved
created: "2026-08-11"
depends_on:
  - "003-core-gameplay-loop"
  - "004-ship-systems"
  - "005-automation-control-plane"
  - "006-world-and-expeditions"
  - "007-tech-progression"
  - "008-simulation-substrate"
  - "009-cargo-workspace"
establishes:
  - "docs/design/simulation-core.md"
  - { kind: directory, path: "crates/icebeek-sim/" }
summary: >
  The contract for crates/icebeek-sim, the single crate where every
  gameplay mechanic of specs 003 through 007 lives. It resolves the
  question spec 009 deferred: the core uses bevy_ecs alone (the ECS
  library, never the bevy umbrella), with determinism enforced by
  normative discipline: a single-threaded, totally ordered tick
  schedule, no wall clock, no order-sensitive iteration over unordered
  collections, and one seeded RNG that only exterior event-generation
  systems may draw from. It pins the fixed-tick model (TICK_HZ,
  initially 20, host-driven accumulator, renderers interpolate), the
  top-level state domains, typed player commands as the only outside
  write path, and the test contract: headless execution, byte-identical
  replay, and save/load equivalence. The crate directory edge is added
  when the crate lands; until then this spec owns its design brief.
---

# 010: Simulation core

## 1. Purpose

Specs 003 through 007 define what the game does; spec 008 requires
that all of it happen in a deterministic, headless, renderer-free
core; spec 009 gives that core a home at `crates/icebeek-sim`. This
spec is the contract for what lives inside: the ECS decision, the tick
model, the state domains, the write paths, and the tests that make the
determinism requirement enforceable rather than aspirational.

## 2. The ECS decision

`icebeek-sim` depends on **`bevy_ecs` alone**: the ECS library as a
direct dependency, never the `bevy` umbrella crate (spec 009 §3.2
forbids the renderer surface; this spec additionally forbids the
umbrella so a feature flag can never smuggle it in).

Why not engine-free structs: the render layers (spec 012) host the
same `World`, so presentation reads gameplay state through queries
instead of through a hand-written mirror layer, and the fixed-tick
schedule is the natural Bevy shape spec 008 chose the engine for.

The accepted cost: `bevy_ecs` does not guarantee determinism by
default. Section 4 turns that from a hazard into a discipline.

## 3. Tick model

- The simulation advances only via an explicit `tick()` on the sim
  world; there is no internal clock, timer, or thread.
- **`TICK_HZ` = 20**, a crate-level constant owned by this spec
  (amend here to tune). Saves record the tick rate they were written
  under, so a future rate change can refuse or migrate old saves
  instead of silently reinterpreting them.
- The host drives a fixed-timestep accumulator: the app (spec 012) in
  play, the test harness headless. Renderers interpolate between the
  last two ticks for presentation; the sim never reads frame time.
- Per-tick phase order is fixed and total:
  1. **Commands**: apply queued player commands.
  2. **World**: exterior systems (kinetics, ice, intake, weather,
     expeditions, sensing) run and may enqueue typed events.
  3. **Interior**: control-plane systems consume from the event queue
     and run logistics, thermodynamics, automation rules, drones,
     repair. A backlog may remain (spec 002 §3.4).
  4. **Readback**: interior outcomes are written to shared capability
     state (available thrust, turning rate, prow options, sensor
     coverage, intake capacity) that the next tick's world phase
     reads (spec 002 §3.3).

## 4. Determinism discipline

Normative rules, each backed by the test contract in section 8:

1. The tick schedule executes **single-threaded** with a **total
   system order**; schedule ambiguities are promoted to test failure
   via `bevy_ecs` ambiguity detection. No parallel iteration inside
   gameplay systems.
2. **No wall clock.** Time inside the crate is the tick counter.
   `std::time` reads in simulation logic are forbidden.
3. **No order-sensitive iteration over unordered collections.** Any
   iteration whose order can affect state uses an explicit
   deterministic key (entity spawn index, sorted key, stable queue
   order). `HashMap`/`HashSet` iteration may not feed state changes.
4. **One RNG, seeded, exterior-only.** A single seeded RNG resource
   whose state serializes with the save. Only world-phase
   event-generation systems may draw from it; interior systems have
   no randomness at all (spec 002 §4). The seed is chosen at run
   creation and is part of the state.
5. Automation rules evaluate in a **stable, player-visible order**
   (spec 005 §3); rule evaluation never observes mid-tick partial
   state from later phases.

## 5. State domains

The shared state (the "one truth" of spec 008 requirement 5) lives
entirely in this crate. Top-level domains, each grounded in its owning
design spec:

| domain | contents | design authority |
|--------|----------|------------------|
| ship kinetics | position, heading, speed, momentum, total mass, torque demand, fuel burn | 003 §2, 005 §5 |
| hull graph | nodes, ambient pressure, stress, breaches | 005 §4 |
| prow | loadout track, wear, charge | 004 §2 |
| engine core | temperature, fuel buffer, shutdown sequence state | 004 §3 |
| interior grid | decks, rooms, spine (belts, pipes, data lines) | 004 §4 |
| thermal field | heat sources, coolant flow, room temperatures | 004 §5 |
| cargo and stock | primary hold, stockpiles, machine buffers | 003 §2 |
| drones | fleet, zones, logic bindings, uptime, maintenance | 005 §2 |
| automation rules | player-authored gates, evaluation order | 005 §3 |
| world | map seed, Fog of Winter reveal state, ice field, weather, expedition sites | 006 |
| tech | research accrual, tier, blueprints | 007 |
| queues | pending events, pending commands | 002 §3, this spec |

The table pins the domain list, not field-level schemas; shapes are
implementation latitude inside the crate. Renderers own none of it.

Two engine-and-heat mechanisms are determinism surface rather than
field latitude, so they are pinned here:

- **Shutdown ladder.** The engine core's cold-shutdown sequence
  (spec 004 §3) is a total order over dependent systems, farthest
  from the core first, propulsion always last; recovery retraces the
  same ladder in reverse. Ladder movement is tick-counted with
  hysteresis between a stall threshold and a restart threshold; wall
  time never participates.
- **Thermal relaxation.** The thermal field updates by reading the
  previous tick's temperatures and writing the next (double
  buffered), so compartment iteration order cannot leak into state.
  For the vertical slice the compartment set aligns one-to-one with
  hull-graph nodes; the real room graph arrives with the interior
  grid domain.
- **Rule evaluation.** Automation rules evaluate once per tick at the
  start of the interior phase, in their stored, player-visible list
  order (spec 005 §3). When two rules write the same routing switch,
  the later rule in list order wins. Rule ids come from a monotonic
  counter that serializes with the save; evaluation reads state as of
  the end of the world phase and never observes mid-tick interior
  writes.
- **Drone determinism.** Drones act in spawn order. A repair drone
  serves the most stressed node inside its assigned zone, ties broken
  by the lowest node index; work accrues wear, and maintenance draws
  stock in whole units. Drones idle while the shutdown ladder has the
  drone bays down.
- **Weather resolution.** Weather events resolve into typed
  equipment state: valve and sensor freezes thaw on a tick countdown
  or by a manual-override command, and a drone scramble suppresses
  fleet logic for a tick-counted window while belts (the feed line)
  keep running (spec 006 §5). Onset and end travel as paired events
  (spec 011 §5); the interior never deletes queue entries to end a
  storm. Frozen valves and sensors degrade the intake-capacity and
  sensor-coverage readback, which the next tick's world phase reads.
- **Expedition resolution.** Anchoring is an ordered helm state;
  expedition emissions happen only while anchored at a site. Crush
  progression resolves as ambient stress distributed across the
  whole hull graph, and extraction hauls travel as ordinary
  Ingestion events (spec 006 §4).

## 6. Write paths

- **Typed player commands are the only outside write path**: course
  orders, build and refit orders, rule edits, drone zone edits,
  expedition orders, manual overrides. Commands are serializable
  values pushed into the command queue; renderers never mutate state
  directly.
- Exterior events (the spec 002 coupling contract) are produced by
  world-phase systems inside this crate and typed by
  `icebeek-events` (spec 011). The sim consumes its own queue; the
  render layers only observe it.
- A recorded command log plus the run seed replays a run segment
  exactly (the replay test below is this property, enforced).
- **The presentation read path is a typed snapshot.** The sim
  exposes a copy of the render-relevant surface at the last
  completed tick; renderers hold snapshot copies, never references
  into the world, and snapshots are never serialized (spec 012 §2).
  What the snapshot carries is implementation latitude; that it is a
  copy is not.

## 7. Save contract

A save is the serialized shared state plus both pending queues and
the RNG state, nothing else (spec 008 requirement 4). Loading a save
and applying the same subsequent commands produces the same states as
the uninterrupted run. Save format versioning and migration policy is
spec 017: on disk a save travels inside its versioned envelope
(format version, crate version, TICK_HZ), and loading follows its
compatibility rules. A tick-rate mismatch remains a refusal until a
migration explicitly converts it.

## 8. Test contract

Shipped with the crate, run headless (`cargo test -p icebeek-sim` on
a machine with no display or GPU):

1. **Replay determinism.** Same seed, same command script, N ticks,
   run twice: the serialized end states are byte-identical.
2. **Save/load equivalence.** Serialize at tick k, load, continue to
   N: byte-identical to the uninterrupted run at N.
3. **Schedule integrity.** `bevy_ecs` ambiguity detection reports
   zero ambiguities; the phase order of section 3 is asserted.
4. **Interior purity.** With an empty event queue and no commands,
   the interior phase is a pure function of prior state (no RNG
   access; asserted by instrumenting the RNG resource).

## 9. Territory

This spec owns `crates/icebeek-sim/` once it exists. An approved spec
may not claim units that do not exist yet (indexer I-004, recorded in
spec 009 §5), so the directory edge is added by the PR that creates
the crate; the crate manifest carries
`[package.metadata.spec-spine] spec = "010-simulation-core"` from its
first commit. Until then this spec owns
`docs/design/simulation-core.md`.

## 10. Out of scope

- The event schema and queue types: spec `011-event-bus` (this crate
  consumes them).
- Rendering, interpolation details, input capture: spec `012`.
- CI wiring for the Rust workspace: spec `013`.
- Balancing data (rates, recipes, room rosters, event tables) and the
  rule-language authoring syntax: future specs; this spec fixes
  mechanisms, not numbers.
- Save-format versioning and migration: spec `017-save-versioning`,
  per section 7.
