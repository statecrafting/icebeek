---
id: "003-core-gameplay-loop"
title: "The core gameplay loop: break, ingest, build"
status: approved
created: "2026-08-10"
depends_on:
  - "001-absolute-zero-vision"
  - "002-dual-view-architecture"
establishes:
  - "docs/design/core-loop.md"
summary: >
  The continuous three-phase cycle the whole game runs on. Navigating
  and breaking (the Macro): chart a course through ice of varying
  density, trading fuel and prow damage for speed and yield. Scavenging
  and ingestion (the intake): raw materials ground out of the ice enter
  the primary cargo hold. Building and automating (the Micro): route
  materials through refineries, fabricators, and the engine core,
  building a closed-loop control plane. The loop is a spiral, not a
  circle: each pass leaves the ship heavier, hungrier, and more capable,
  and stopping at any point begins the crush-death clock.
---

# 003: The core gameplay loop

## 1. Purpose

Define the minute-to-minute and hour-to-hour cycle that every system
spec plugs into, and the failure spiral that gives the loop its stakes.

## 2. The three phases

1. **Navigating and breaking (Macro).** The player charts a course
   through ice densities. Thicker ice yields rarer resources but
   requires more engine thrust, consumes more fuel, and damages the
   prow. Course choice is the game's strategic layer: risk-on through a
   glacial wall, or risk-off around it.
2. **Scavenging and ingestion (the intake).** As the ship grinds
   forward, raw materials (frozen scrap, ancient technology, biomass,
   ice for water and coolant) are ingested through intake valves into
   the primary cargo hold. Ingestion is passive and continuous while
   moving; its rate and mix are functions of the ice being broken.
3. **Building and automating (Micro).** The player designs the
   interior: routing raw materials via conveyor belts or transit drones
   to refineries, fabricators, and the engine core, and authoring the
   automation rules that keep those flows running unattended.

## 3. The failure spiral

Stopping is the root failure. A stationary ship takes accelerating hull
pressure until crushed. Every proximate failure (fuel starvation, core
cooldown, prow destruction, logistics gridlock) matters because it
threatens motion. Recovery mechanics exist (emergency reserves, manual
overrides) but each buys time rather than safety: the loop must be
restored or the run ends.

## 4. Pacing contract

- The Macro sets tempo: ice density ahead determines how much slack the
  player has for interior work.
- The Micro sets ceiling: interior throughput determines how aggressive
  a course the player can afford to chart.
- The game must always make "time in the other view" a meaningful cost.
  Neither view may become a pause screen for the other (see spec 002,
  neither view blocks the other).

## 5. Out of scope

- Ship subsystem details: spec `004-ship-systems`.
- Automation semantics: spec `005-automation-control-plane`.
- Ice classes, expeditions, weather: spec `006-world-and-expeditions`.
