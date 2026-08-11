# icebeek

Working title: **Absolute Zero Architecture**.

A survival base-builder / resource-management / automation game.
Command a colossal, perpetually moving icebreaker across a frozen
ocean: ram through shifting glaciers to harvest ancient scrap, build an
automated industrial grid inside the hull, and keep the engine core
burning through the eternal winter. If the ship stops, the ice crushes
it.

Two coupled perspectives, one simulation:

- **The Macro.** Isometric 3D exterior: navigation, ice-breaking,
  expeditions, weather. The unpredictable event generator.
- **The Micro.** Top-down cross-section interior: rooms, belts, pipes,
  drones, thermodynamics. The deterministic control plane that must
  absorb what the exterior throws at it.

## Development substrate

This repository is governed by [spec-spine](https://github.com/statecrafting/spec-spine):
authored truth lives in `specs/NNN-slug/spec.md`, derived artifacts are
compiler-emitted, and code changes must be accompanied by the spec that
owns them.

- `specs/` is the design corpus; start at `specs/001-absolute-zero-vision/`.
- `AGENTS.md` carries the cross-agent session protocol.
- `spec-spine compile && spec-spine index && spec-spine lint` is the
  local loop; `spec-spine couple` is the PR gate.

Concept reference images live in `docs/ideas/`.
