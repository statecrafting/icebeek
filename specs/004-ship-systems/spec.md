---
id: "004-ship-systems"
title: "The icebreaker: prow, engine core, internal grid, heat"
status: approved
created: "2026-08-10"
depends_on:
  - "003-core-gameplay-loop"
establishes:
  - "docs/design/ship-systems.md"
summary: >
  The ship as a mobile base: a massive multi-decked behemoth whose
  limited internal space makes architectural planning critical. Four
  subsystems: the Prow (the upgradeable drill/ram interface to the
  ice), the Engine Core (the fuel-burning heart whose temperature
  gates every other system), the Internal Grid (a node-based room and
  logistics-spine building system rendered in the Micro cross-section
  view), and Heat Distribution (thermodynamics as a routing problem:
  machinery must be cooled, exhaust can be harvested, biology must be
  warmed).
---

# 004: Ship systems

## 1. Purpose

Define the four subsystems of the icebreaker that the Micro view builds
and the Macro view stresses.

## 2. The Prow

The front of the ship, and the interface between hull and ice. Upgrade
tracks match ice families (spec 006):

- **Kinetic rams** for brute-force impact against pack ice.
- **Thermal lasers / lances** to soften or melt ice ahead of impact,
  effective against pure ice and frozen methane.
- **Sonic disruptors** to shatter dense glacial material by resonance.

Prow choice is loadout strategy: each track has distinct power draw,
heat output, and wear characteristics that the interior must support.
The prow degrades with use; its repair pipeline is a permanent logistics
customer.

## 3. The Engine Core

The beating heart. It requires a constant, uninterrupted supply of
refined fuel delivered by the interior logistics network.

- Core temperature is the master health stat. If it drops, ship systems
  shut down sequentially, farthest-from-core first, ending with
  propulsion (and then the crush clock of spec 003 runs).
- The core is also the ship's largest heat source, which makes it the
  anchor of the heat-distribution economy below.

## 4. The Internal Grid

A node-based building system in the Micro view (spec 002):

- **Rooms** are placed modules: Foundries, Refineries, Fabricators,
  Hydroponics, Drone Bays, Heat Sinks, Storage, and structural struts.
- **The logistical spine** connects them: pipes for fluids, belts for
  solids, data lines for automation signals.
- Hull space is strictly finite per deck. Refitting (tearing out and
  re-routing a working layout) is an expected, tooling-supported
  activity, because each tech tier (spec 007) invalidates prior
  routing assumptions.

## 5. Heat distribution

Base-building is thermodynamics, not just adjacency:

- Heavy machinery generates heat; unmanaged heat causes meltdowns and
  efficiency loss.
- Coolant is melted intake ice, routed by pipe; spent coolant returns
  as usable water.
- Exhaust heat is a resource: routed to Hydroponics and crew-biology
  modules it replaces dedicated heaters; vented overboard it is waste.
- Cold is the default. An unpowered, unheated room trends toward
  ambient exterior temperature, which stalls machinery and kills
  biology.

## 6. Out of scope

- Drone behavior and automation rules over this grid: spec `005`.
- Mass and drag consequences of grid expansion: spec `005`.
- Exact room roster, sizes, and recipes: balancing data, deferred to
  implementation specs under `008-simulation-substrate`.
