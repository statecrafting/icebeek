---
id: "016-tech-tiers"
title: "Tech mechanisms: research accrual, blueprints, tier gates"
status: draft
created: "2026-08-11"
depends_on:
  - "003-core-gameplay-loop"
  - "006-world-and-expeditions"
  - "007-tech-progression"
  - "010-simulation-core"
  - "011-event-bus"
establishes:
  - "docs/design/tech-tiers.md"
summary: >
  The mechanism spec for the tech domain (spec 010 section 5, spec
  007): research accrues only from processing ingested ancient
  technology through a Refinery, never passively; tier-gating
  blueprints arrive from the world as a new Salvage event family
  (wreck salvage from pack ice, wall caches from glacial breaches,
  node vaults from expeditions), making major progression a Macro
  risk decision per spec 007 section 4. Tier state gates what the
  interior may build and what the prow may mount, and a tier
  transition deliberately re-prices heat and fuel so prior layouts
  obsolesce (the paradigm rule of spec 007 section 2). Adding the
  Salvage family is an explicit spec 011 amendment carried by this
  spec's implementation PR.
---

# 016: Tech mechanisms

## 1. Purpose

Spec 007 defines the three-tier progression arc; spec 010 §5
reserves the tech domain (research accrual, tier, blueprints). This
spec is the contract for the mechanisms: how research points accrue,
how blueprints travel and unlock, what a tier gate blocks, and what
a tier transition changes.

## 2. Research accrual

- Research accrues **only** when a Refinery processes AncientTech
  units from cargo (spec 007 §4: baseline research comes from
  processing, not possession). No passive trickle, no wall-clock
  component: accrual is per processed unit, in the interior phase,
  deterministic.
- Accrued research is spent on tier advancement and on individual
  unlocks inside the current tier; costs are balancing data.

## 3. Blueprints and the Salvage family

- Tier-gating blueprints come from the world (spec 007 §4): trapped
  shipwrecks in pack ice, caches inside breached glacial walls, and
  Iceberg Node vaults.
- They travel as a new **Salvage** event family: `WreckSalvage`,
  `WallCache`, and `NodeVault` variants carrying a blueprint id.
  Adding the family is an amendment to spec 011 §4 (the roster is
  closed until amended); the amendment lands with this spec's
  implementation PR, not silently.
- The interior resolves Salvage events into the blueprint set of the
  tech domain. Blueprints are unique flags, not stock: duplicate
  finds convert to research points.
- Tier N+1 requires its blueprint set complete plus a research
  spend: the wall between tiers stays a Macro risk decision.

## 4. Tier gates

- The tech domain's tier gates: room types placeable on the grid
  (spec 015), prow loadout tracks mountable (spec 004 §2), and rule
  vocabulary available to automation (spec 005 §3 tiers).
- A gated order is rejected in the commands phase exactly like an
  invalid build order (spec 015 §5): typed rejection, no partial
  state.
- Already-placed equipment from a lower tier keeps running after
  advancement; the paradigm rule bites through economics, not
  deletion.

## 5. Tier transitions re-price the ship

Advancing a tier applies the new paradigm's profile (spec 007 §3):
fuel chemistry and burn rates, heat emission scales, belt and drone
throughput factors. The profile swap is what makes a tier-1 layout
obsolete inside a tier-2 economy (spec 007 §2). Profile contents are
balancing data; that a transition swaps profiles atomically on one
tick is pinned here.

## 6. Test contract

Headless, alongside the spec 010 §8 suite:

1. **No passive accrual.** A stationary, non-processing run accrues
   zero research over any tick count.
2. **Deterministic accrual.** Processing N units yields the same
   research on every replay.
3. **Gate enforcement.** A gated room, prow, or rule order is
   rejected below its tier and accepted at it, with no partial
   state on rejection.
4. **Duplicate conversion.** A second identical blueprint converts
   to research; the blueprint set never double-counts.
5. **Atomic transition.** The tick of tier advancement applies the
   whole new profile; no tick observes a mixed profile.

## 7. Out of scope

- The research node graph, costs, and per-tier content lists:
  balancing data for a future spec.
- Prestige or cross-run meta-progression: spec 007 §5 already defers
  it.
- Salvage event rates and placement: spec 014's field profiles and
  balancing data.
