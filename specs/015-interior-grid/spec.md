---
id: "015-interior-grid"
title: "The buildable interior grid: rooms, the spine, refit"
status: approved
created: "2026-08-11"
depends_on:
  - "004-ship-systems"
  - "005-automation-control-plane"
  - "010-simulation-core"
  - "012-renderers"
establishes:
  - "docs/design/interior-grid.md"
summary: >
  The mechanism spec for the interior grid domain (spec 010 section 5,
  spec 004 section 4): a per-deck cell grid with strictly finite hull
  space, room modules placed onto cell footprints, and the logistics
  spine (belts, pipes, data lines) as typed edges between them. Build,
  tear-out, and re-route are typed player commands validated
  deterministically in the commands phase; refit refunds a fixed
  fraction of materials, making refactoring an expected activity
  rather than a penalty. Rooms carry mass (the spec 005 efficiency
  tax), heat behavior (the thermal field migrates from the bootstrap
  compartment ring onto the grid), and machine buffers. Breaches
  disable cells, and cascades follow the spine topology, so
  over-provisioning is an engineering decision the grid makes legible.
---

# 015: The interior grid

## 1. Purpose

Spec 004 §4 defines the node-based building system; spec 010 §5
reserves the interior grid domain and notes the bootstrap thermal
compartments are placeholders for it. This spec is the contract for
the grid itself: the space model, the room and spine mechanisms, the
build write path, and how existing domains (thermal, mass, hull,
drones) rebase onto it.

## 2. The space model

- Each deck is a rectangular cell grid; hull space is strictly
  finite per deck (spec 004 §4). Deck count and dimensions are
  balancing data; their existence and finiteness are pinned.
- Cells map onto hull-graph nodes for stress and breach adjacency
  (spec 005 §4); the mapping is fixed per layout, deterministic,
  and serializes with the save.
- Everything on the grid is addressed by (deck, x, y); iteration for
  any state-affecting pass runs in that lexicographic order (spec
  010 §4 rule 3).

## 3. Rooms

- A room is a placed module occupying a rectangular cell footprint:
  the spec 004 §4 roster (Foundry, Refinery, Fabricator, Hydroponics,
  Drone Bay, Heat Sink, Storage, structural strut) plus the engine
  core as a fixed pre-placed room.
- Each room type carries: footprint, mass (the spec 005 §5 tax
  feeds from placed rooms plus stockpiles), heat emission or
  absorption (spec 004 §5), machine input and output buffers, and a
  power-and-cold stall rule: an unpowered or freezing room stops
  working before it breaks (spec 004 §5: cold stalls machinery; it
  kills only biology).
- The thermal field of spec 010 §5 migrates from the bootstrap
  compartment ring to per-cell temperatures with the same
  double-buffered relaxation discipline; heat sources are room
  emissions, sinks are Heat Sinks, hull leak, and exhaust routing.

## 4. The spine

- Belts (solids), pipes (fluids), and data lines (signals) are typed
  edges over cell boundaries (spec 004 §4). Belts and pipes move
  whole units per tick along fixed directions; data lines carry
  rule signals (spec 005 §3) and fail under flare scramble while
  belts keep running (spec 006 §5, already resolved in the sim).
- Item movement processes edges in stable creation order; a full
  destination back-pressures rather than drops (nothing vanishes).
- A breach disables its cell's rooms and severs its edges; cascade
  reach follows spine topology (spec 005 §4), so routing redundancy
  is real risk management.

## 5. Write path: build and refit

Typed commands, the only mutation route (spec 010 §6):

- **Place room**, **remove room**, **lay edge**, **remove edge**:
  validated in the commands phase against footprint overlap, bounds,
  and material cost drawn from cargo; an invalid order is dropped
  with a typed rejection readable by the UI, never a partial apply.
- Removal refunds a fixed fraction of the build cost (balancing
  data; the existence of a refund is pinned: refit is an expected
  activity, spec 004 §4).
- Jettison (spec 005 §5) is remove-without-refund plus immediate
  mass relief: the emergency lever costs real material.

## 6. Test contract

Headless, alongside the spec 010 §8 suite:

1. **Placement validation.** Overlap, out-of-bounds, and
   unaffordable orders are rejected; state is untouched by a
   rejected order.
2. **Conservation.** Build, run, tear out, rebuild: no resource is
   created or destroyed except the declared refund loss.
3. **Stable routing.** Two identical layouts with identical inputs
   route identically; edge processing order is creation order.
4. **Cascade reach.** A breach disables exactly its cell and
   severed-spine dependents, deterministically.

## 7. Migration note

The bootstrap compartment ring (spec 010 §5) retires when this spec
lands in code: thermal state, drone zones, and equipment locations
rebase onto grid cells. That change is save-breaking and therefore
sequences after spec 017 (save versioning) is approved and
implemented, or ships with an explicit refuse-old-saves bump under
017's policy.

## 8. Out of scope

- Room recipes, sizes, costs, throughput numbers: balancing data.
- The rule-language surface over data lines: spec 005 §3 authority;
  syntax is a future spec.
- Crew biology modules beyond Hydroponics' heat need: future spec.
- Grid presentation and build UI: spec 012 and future UI specs.
