---
id: "006-world-and-expeditions"
title: "The world: Fog of Winter, ice topography, expeditions, weather"
status: approved
created: "2026-08-10"
depends_on:
  - "002-dual-view-architecture"
  - "003-core-gameplay-loop"
establishes:
  - "docs/design/world.md"
summary: >
  The frozen ocean as the game's event generator. The map is
  procedurally generated and obscured by a Fog of Winter. Ice
  topography comes in classes with distinct resistance and yield:
  pancake ice (easy, poor), pack ice (moderate, holds trapped
  shipwrecks), and glacial walls (massive momentum or advanced tech to
  breach, hiding high-tier blueprints). Iceberg Nodes (frozen
  skyscrapers, dormant volcanoes) are anchor-and-strip-mine expedition
  sites under shifting-ice time pressure. Dynamic weather attacks the
  ship's systems directly: super-storms freeze intake valves and force
  manual overrides; solar flares scramble drone logic and force
  mechanical fallbacks.
---

# 006: The world and expeditions

## 1. Purpose

Define the exterior content that generates the events the interior must
absorb: terrain, ice, expedition sites, and weather.

## 2. The map and the Fog of Winter

The map is procedurally generated per run and fully obscured by the Fog
of Winter. Revealing it costs something: scout drones, sensor power, or
hull-risking proximity. Long-range planning is a reward for investment
in sensing, never free information.

## 3. Ice topography

Ice classes are the Macro's difficulty and economy dial:

- **Pancake ice.** Easy to break, low resource yield. Safe transit and
  breathing room.
- **Pack ice.** Moderate resistance; holds trapped shipwrecks worth
  scavenging. The bread-and-butter risk/reward band.
- **Glacial walls.** Require massive momentum or advanced prow tech
  (spec 004) to breach; hide ancient, high-tier technological
  blueprints (spec 007). The deliberate wall between progression
  tiers.

Class determines break resistance, prow wear, fuel cost, intake yield
mix, and hull-stress event profile (spec 005).

## 4. Iceberg Node expeditions

Occasionally the ship reaches a massive Iceberg Node: a frozen
skyscraper, a dormant volcano, an entombed installation.

- The ship anchors. Anchoring stops forward motion, so the crush clock
  of spec 003 becomes the expedition timer: the surrounding ice
  shifts and will eventually crush the vessel.
- Automated crawler rovers (manufactured interior-side, spec 005)
  deploy to strip-mine the node.
- The player decision is greed calibration: every additional minute of
  extraction is bought with hull stress and escape risk.

## 5. Dynamic weather

Weather events target systems, not hit points:

- **Super-storms** freeze intake valves and external sensors,
  requiring emergency manual overrides (a deliberate, temporary break
  from the automation fantasy that makes the automation feel earned).
- **Solar flares** scramble drone logic and data lines, forcing
  reliance on mechanical fallbacks: belts keep running when smart
  routing dies. Players who build purely drone-based logistics learn
  why belts still exist.

Weather is generated in the Macro and delivered to the interior
exclusively as typed events and state changes per the spec 002 coupling
contract.

## 6. Out of scope

- Blueprint contents and tier gating: spec `007-tech-progression`.
- Procedural generation algorithms, biome tables, and event rates:
  implementation specs under `008-simulation-substrate`.
