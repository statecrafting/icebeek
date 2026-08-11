---
id: "001-absolute-zero-vision"
title: "Absolute Zero Architecture: the game vision"
status: approved
created: "2026-08-10"
depends_on:
  - "000-bootstrap"
establishes:
  - "README.md"
  - { kind: directory, path: "docs/ideas/" }
summary: >
  The founding design thesis for icebeek (working title: Absolute Zero
  Architecture). A survival base-builder / resource-management /
  automation game in which the player commands a colossal, perpetually
  moving icebreaker across a frozen ocean: ram through shifting glaciers
  to harvest ancient scrap, build an automated industrial grid inside the
  hull, and keep the engine core burning through an eternal winter. The
  core tension is forward momentum versus internal stability; the core
  aesthetic contrast is a chaotic, hostile frozen exterior against a
  deterministic, efficient, automated interior. This spec is the
  authority every downstream system spec refines; a system that does not
  serve the tension or the contrast does not ship.
---

# 001: Absolute Zero Architecture, the game vision

## 1. Purpose

icebeek is a survival base-builder about a ship that is also a factory
and can never stop moving. The player is not a crew member; they are the
Architect, the central intelligence that designs systems which run
themselves.

**Logline.** Command a colossal, perpetually moving icebreaker ship
across a frozen ocean. Ram through shifting glaciers to harvest ancient
scrap, build out a complex automated industrial grid within the ship's
hull, and keep the engine core burning to survive the eternal winter.

## 2. The core tension

Forward momentum versus internal stability.

- If the ship stops, the ice closes and crushes the hull. Motion is
  survival.
- Motion needs fuel. Fuel comes from breaking ice and scavenging.
- Breaking ice damages the prow and hull, which demands continuous
  internal repair, manufacturing, and expansion.
- Expansion adds mass; mass raises the thrust needed to break ice, which
  raises fuel burn.

Every system spec must close back into this loop. A mechanic that does
not feed the loop (or deliberately stress it) is out of scope.

## 3. Design pillars

1. **Chaotic exterior, deterministic interior.** The frozen world is an
   unpredictable event generator; the ship interior is a control plane
   the player engineers to absorb those events. The contrast is the
   identity of the game.
2. **Systems, not crew.** No individual-colonist micromanagement. The
   player authors logic, zones, and flows; drones and machinery execute.
3. **Efficiency under constraint.** Hull space is finite and mass is
   taxed. Sprawl is punished; refactoring the interior is a first-class
   activity, not a failure state.
4. **Thermodynamics as terrain.** Heat is a resource and a hazard.
   Machinery must be cooled; exhaust can be harvested; cold kills.

## 4. Territory

This spec owns the project's public identity surface: `README.md` and
the concept material under `docs/ideas/` (the two reference images that
anchor the two visual perspectives).

## 5. Out of scope

- Presentation and view coupling: spec `002-dual-view-architecture`.
- The moment-to-moment loop: spec `003-core-gameplay-loop`.
- Engine, language, and runtime selection: owned by spec
  `008-simulation-substrate` (decided at its human checkpoint: Bevy,
  Rust).
- Monetization, platform targets, multiplayer, and narrative campaign
  structure: deliberately unaddressed at this stage.
