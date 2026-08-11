---
id: "002-dual-view-architecture"
title: "Dual-view architecture: isometric event generator, cross-section control plane"
status: approved
created: "2026-08-10"
depends_on:
  - "001-absolute-zero-vision"
establishes:
  - "docs/design/dual-view.md"
summary: >
  The decided presentation split. World exploration renders as an
  isometric 3D exterior view (the Macro); ship building renders as a
  top-down cross-section interior view (the Micro), in the lineage of
  Fallout Shelter / Oxygen Not Included for the interior and Factorio /
  Dyson Sphere Program for the exterior scale feel. The two views never
  render each other's detail; they communicate exclusively through
  shared simulation state. The Macro is the unpredictable event
  generator, the Micro is the deterministic control plane that must
  absorb and resolve those events. This spec defines what each view
  owns, the event-to-state coupling contract between them, and the
  determinism requirement on the interior simulation.
---

# 002: Dual-view architecture

## 1. Purpose

Splitting the perspective resolves the classic base-builder clash
between macro-scale exploration and micro-scale logistics. Decoupling
them creates two interlocking systems with distinct jobs:

- **The Macro (isometric 3D exterior).** Scale, momentum, and
  environmental variables. The player feels the mass of the ship against
  the wasteland: charting courses, managing thrust against drag,
  deciding whether to burn fuel through a glacial wall or route around
  it. Reference image: `docs/ideas/licensed-image.jpeg`.
- **The Micro (top-down cross-section interior).** Pure efficiency and
  state-driven automation. A strictly constrained grid where every tile
  and routing decision matters: belts, pipes, data lines, heat.
  Reference image: `docs/ideas/images.jpeg`.

## 2. View responsibilities

**The Macro owns:** terrain and topography rendering, ship silhouette
and gross motion, weather presentation, expedition deployment, prow
impacts, intake events. It emits events; it never simulates interior
logistics.

**The Micro owns:** the interior grid, rooms and machines, logistics
routing, thermodynamics, drone execution, automation rules. It consumes
events and resolves them; it never renders horizon, terrain, or hull
exterior.

Audio follows the split: exterior is howling wind and grinding ice;
interior is rhythmic machinery hum. The transition between views is a
single deliberate camera/audio crossfade, fast enough to be used
constantly.

## 3. The coupling contract

The two views communicate only through shared simulation state, never by
reaching into each other's renderers or entity sets.

1. **Events flow Macro to Micro.** An exterior occurrence (heavy impact
   on the starboard bow, intake ingestion batch, sensor freeze, solar
   flare) is recorded as a typed event with a location on the hull
   graph, a magnitude, and a timestamp.
2. **State changes are the only translation.** An event becomes a state
   change in the shared model (hull stress at node N raised, cargo
   buffer credited, drone logic degraded). The Micro reacts to state,
   not to rendered exterior phenomena.
3. **Resolution flows Micro to Macro.** Interior outcomes (repair
   completed, fuel delivered to core, prow charge ready) update shared
   state that the Macro reads back as capability: available thrust,
   turning rate, prow options, sensor coverage.
4. **Neither view blocks the other.** Events queue; the interior
   simulation consumes them on its own tick. A backlog of unresolved
   events is a legitimate gameplay pressure, not an error.

## 4. Determinism requirement

The interior simulation is deterministic: given the same state and the
same event queue, it produces the same outcome, independent of frame
rate or view focus. Randomness lives in the Macro event generator only.
This is a gameplay guarantee (automation the player builds must behave
predictably) and a technical one (the Micro can be simulated headless
while the player is in the Macro view).

## 5. Out of scope

- Concrete event schema, tick model, and engine bindings: spec
  `008-simulation-substrate`.
- The content of the events (which storms, which impacts): specs `004`
  through `006`.
