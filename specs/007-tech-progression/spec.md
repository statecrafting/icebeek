---
id: "007-tech-progression"
title: "Tech tree: three tiers of architectural paradigm"
status: approved
created: "2026-08-10"
depends_on:
  - "004-ship-systems"
  - "005-automation-control-plane"
  - "006-world-and-expeditions"
establishes:
  - "docs/design/progression.md"
summary: >
  Progression is achieved by unlocking new architectural paradigms, not
  by incremental stat bumps. Tier 1, Mechanical Grinding: coal and
  wood-burning engines, manual routing, brute-force hull smashing.
  Tier 2, Chemical and Thermal: oil and methane processing, basic
  conveyor belts, thermal lances that melt ice before impact. Tier 3,
  Autonomous and Nuclear: nuclear reactors, complete logistics
  automation, localized AI control modules, sonic resonance
  ice-shattering. Each tier deliberately invalidates the previous
  tier's interior layout assumptions, making refactoring the ship the
  signature progression activity.
---

# 007: Tech tree and progression

## 1. Purpose

Define the three-tier progression arc and the rule that ties
progression to architecture rather than numbers.

## 2. The paradigm rule

A tier is a new way the ship works, not a better version of the old
way. Advancing must force layout decisions: new fuel logistics, new
heat profiles, new routing topology. A player who unlocks tier 2 and
changes nothing about their interior should feel the obsolescence
immediately (bottlenecks, waste heat, dead capacity), per the refit
expectation of spec 004.

## 3. The tiers

1. **Tier 1: Mechanical Grinding.** Coal and wood-burning engines,
   manual routing, brute-force hull smashing. High labor, low
   information: the player is the logistics network.
2. **Tier 2: Chemical and Thermal.** Oil and methane processing, basic
   conveyor belts, thermal lances to melt ice before impact. Flow
   replaces hauling; heat becomes an economy instead of a nuisance.
3. **Tier 3: Autonomous and Nuclear.** Nuclear reactors, complete
   logistics automation, localized AI control modules, sonic resonance
   ice-shattering. The ship approaches a closed self-regulating
   organism; the player's job shifts fully from operating to
   governing.

## 4. Unlock sources

- Baseline research accrues from processing ancient technology
  ingested from the ice (spec 003).
- Tier-gating blueprints come from the world: trapped shipwrecks in
  pack ice and, for the highest tier, the interiors of glacial walls
  and Iceberg Nodes (spec 006). Major progression is always paid for
  with a risk decision in the Macro.

## 5. Out of scope

- Full research-node graph, costs, and balancing: implementation
  specs under `008-simulation-substrate`.
- Prestige/meta-progression across runs: future spec if wanted.
