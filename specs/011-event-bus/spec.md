---
id: "011-event-bus"
title: "Event bus: the typed exterior-traffic vocabulary"
status: approved
created: "2026-08-11"
depends_on:
  - "002-dual-view-architecture"
  - "008-simulation-substrate"
  - "009-cargo-workspace"
  - "010-simulation-core"
establishes:
  - "docs/design/event-bus.md"
  - { kind: directory, path: "crates/icebeek-events/" }
summary: >
  The contract for crates/icebeek-events, the engine-free vocabulary
  crate for the spec 002 coupling contract. It defines the event
  envelope (emission tick, deterministic sequence number, and, where
  the family calls for it, hull-graph location and magnitude), the
  initial event families (Impact, Ingestion, Weather, Expedition),
  and the queue semantics: FIFO ordered by tick and sequence, visible
  to the interior phase of the same tick, backlog legal, serialized
  whole into saves. Events are pure data: the crate contains types
  and the queue container, no systems, no callbacks, no game logic,
  and depends on serde and the standard library only. Player command
  types are explicitly not here; they live in icebeek-sim per spec
  010 section 6. The crate directory edge is added when the crate
  lands; until then this spec owns its design brief.
---

# 011: Event bus

## 1. Purpose

Spec 002 §3 requires that everything the exterior does to the
interior travel as typed events; spec 008 requirement 3 requires
those events be serializable data, never callbacks; spec 009 §3.1
gives the vocabulary a home (`crates/icebeek-events`) and forbids it
any engine dependency. This spec fixes the envelope, the initial
family roster, and the queue semantics so that spec 010's world and
interior phases, and spec 012's observing renderers, all agree on
one schema.

## 2. Scope boundary: events, not commands

This crate carries exterior traffic only: things the world does to
the ship. Player command types (the other queue of spec 010 §6) live
in `icebeek-sim`, not here: renderers already depend on the sim
crate, and keeping this crate single-purpose keeps it the stable
schema surface for what the world can do. If a future spec wants a
shared no-engine vocabulary crate for commands too, it amends this
boundary explicitly.

## 3. The envelope

Every event carries:

- **`tick`**: the simulation tick of emission (spec 010 §3; the
  timestamp of spec 002 §3.1 is tick-denominated, never wall time).
- **`seq`**: a monotonic sequence number assigned deterministically
  at emission, so ordering is total and replay-stable even within a
  tick.
- **payload**: one variant of a typed event enum. Families whose
  effects are spatial (Impact, and weather that targets equipment)
  carry a hull-graph node id and a magnitude (spec 002 §3.1);
  families that are not spatial (a solar flare is ship-wide) carry
  what they need instead. Stringly-typed payloads are forbidden.

## 4. Initial event families

The starting roster, grounded in the design specs; variants within a
family, and new families, are added by amending this spec:

- **Impact** (specs 003, 005 §4): prow strike or hull contact at a
  node, with magnitude; feeds stress spikes on the hull graph.
- **Ingestion** (spec 003 §2): an intake batch entering the primary
  hold, with resource mix and quantity.
- **Weather** (spec 006 §5): onset and end of super-storms and solar
  flares, valve and sensor freeze at specific equipment nodes, drone
  logic scramble. Weather targets systems, not hit points.
- **Expedition** (spec 006 §4): anchor set, ice-shift warnings,
  crush-pressure progression while anchored, rover return.

The interior may not receive information any event does not carry:
if a design needs the Micro to react to something new, that something
becomes an event variant here first.

## 5. Queue semantics

1. **FIFO by `(tick, seq)`.** Total order, deterministic under
   replay.
2. **Same-tick visibility.** Events enqueued during the world phase
   of tick T are visible to the interior phase of tick T (the phase
   order of spec 010 §3).
3. **Backlog is legal.** The interior consumes what it can; an
   unconsumed remainder persists in order (spec 002 §3.4). Nothing
   expires implicitly; if a family wants expiry (a storm that ends),
   it models it as a paired end event, not as queue deletion.
4. **Saves take the queue whole.** The pending queue serializes into
   the save exactly as it stands (spec 008 requirement 4, spec 010
   §7).

## 6. Dependency and purity rules

- Dependencies: serde and the standard library. No Bevy crate of any
  kind, including `bevy_ecs` (spec 009 §3.1: no engine, ever; the
  sim wraps the queue in its own resource type on its side).
- The crate contains types, the queue container, and their
  serde/ordering impls. No systems, no game logic, no I/O.
- Events serialize into saves, so the schema is save-format surface:
  renaming or removing a variant is a save-breaking change and waits
  on the save-versioning spec (spec 010 §7); adding a variant is an
  amendment here.

## 7. Test contract

Shipped with the crate, headless:

1. **Round-trip.** Every event variant serializes and deserializes
   to an equal value.
2. **Ordering.** A queue populated across ticks and within a tick
   drains in `(tick, seq)` order, and serializing then deserializing
   the queue preserves that order exactly.

## 8. Territory

This spec owns `crates/icebeek-events/` once it exists; the
directory edge is added by the PR that creates the crate (the I-004
pattern recorded in spec 009 §5), and the crate manifest carries
`[package.metadata.spec-spine] spec = "011-event-bus"` from its
first commit. Until then this spec owns `docs/design/event-bus.md`.

## 9. Out of scope

- Which concrete storms, impacts, and yields occur, and how often:
  design authority of specs 003 through 006; rates and tables are
  balancing data for a future spec.
- The command queue and its types: spec 010 §6.
- How the interior resolves events into state changes: spec 010.
- Save-format versioning and migration: future spec.
