---
id: "005-automation-control-plane"
title: "Automation: drones, logic gates, hull integrity, mass budget"
status: approved
created: "2026-08-10"
depends_on:
  - "004-ship-systems"
establishes:
  - "docs/design/automation.md"
summary: >
  The player acts as the central Architect, building systems that run
  themselves instead of micromanaging crew. Four mechanics: autonomous
  drones (repair, logistics, scouting) with player-defined zones and
  operational logic; state-driven automation expressed as strict logic
  gates over resource state; hull integrity as a continuous
  manufacturing and repair-routing obligation that scales with ice
  pressure; and the weight-and-drag economy, where every module adds
  mass, mass raises the torque needed to break ice, and torque raises
  fuel burn, taxing sprawl and rewarding ruthless efficiency.
---

# 005: The automation control plane

## 1. Purpose

Define how the interior runs unattended, and the two pressure systems
(hull integrity, mass budget) that keep automation under permanent
tension.

## 2. Autonomous drones

The player manufactures specialized drones and defines their logic and
operating zones; drones execute without individual orders.

- **Repair drones** patrol assigned hull zones and consume struts and
  plating from logistics.
- **Logistics drones** move solids where belts do not reach, at higher
  energy cost per unit than belts.
- **Scout/crawler rovers** deploy in the Macro view for expeditions
  (spec 006) but are manufactured and maintained by the interior.

Drones are equipment, not characters: no needs, no moods, only uptime,
capacity, and maintenance cost.

## 3. State-driven automation

Resource flow is governed by strict, player-authored logic gates over
shared state, for example: IF engine fuel is below 20 percent, DIVERT
all carbon to the Refinery; ELSE DIVERT to Storage.

- Rules read state (levels, temperatures, stress values, event flags)
  and set routing (belt switches, valve positions, drone priorities).
- Rule evaluation is deterministic and ordered (spec 002 determinism
  requirement); two players with the same rules and inputs get the
  same factory behavior.
- The rule surface starts as simple threshold gates (tier 1) and grows
  toward full signal networks with localized AI control modules
  (tier 3, spec 007).

## 4. Hull integrity

The deeper into the frozen wastes, the higher the ambient ice pressure.

- Hull stress is tracked per node on the hull graph; Macro impact
  events (spec 002) add localized stress spikes on top of ambient
  pressure.
- The player must continuously manufacture structural struts and route
  them, via repair drones, to stressed nodes. Repair is a standing
  production line, not an occasional errand.
- An unserved breach cascades: flooding/freeze-in disables adjacent
  grid nodes, which typically disables some of the automation that
  would have handled the repair. Robust players over-provision.

## 5. Weight and drag

Every module, machine, and stockpile adds mass.

- Total mass sets the engine torque required to maintain ice-breaking
  momentum; torque sets fuel burn.
- The design intent is an explicit efficiency tax: a sprawling,
  redundant interior is paid for on every meter of ice broken.
  Refactoring for compactness is a core late-game activity.
- Jettisoning (scrapping modules or dumping stockpiles overboard) is a
  legitimate emergency lever with real resource loss.

## 6. Out of scope

- The external events that stress the hull: spec `006`.
- Tier-specific automation unlocks: spec `007`.
- Rule-language syntax and evaluation order details: implementation
  specs under `008-simulation-substrate`.
