---
id: "014-world-content"
title: "World content: the ice field, class profiles, Fog of Winter"
status: draft
created: "2026-08-11"
depends_on:
  - "003-core-gameplay-loop"
  - "006-world-and-expeditions"
  - "010-simulation-core"
  - "012-renderers"
establishes:
  - "docs/design/world-content.md"
summary: >
  The mechanism spec that turns spec 006's world design into simulation
  state: a deterministic ice field derived purely from the map seed, the
  three ice classes as data profiles that drive break resistance, prow
  wear, fuel cost, intake yield mix, and hull-stress event profile, and
  the Fog of Winter as monotonic reveal state fed by sensor coverage.
  The field is a pure function of (seed, position), never stored;
  only reveal state serializes. Field derivation uses seed-keyed
  hashing, never the event RNG, so sensing and navigation cannot
  perturb the event stream. The exterior renderer presents topography
  and fog from the same snapshot surface it already reads.
---

# 014: World content

## 1. Purpose

Spec 006 defines what the frozen ocean is; spec 010 gives the world
domain a home and pins the determinism discipline. This spec is the
contract for the content mechanisms in between: how the ice field is
derived, what an ice class means mechanically, and how the Fog of
Winter reveal state works. It is the design authority the world-slice
code inside `crates/icebeek-sim` (spec 010 territory) and the
exterior presentation (spec 012 territory) answer to.

## 2. The ice field

- The field assigns every world position an ice class: open water,
  pancake ice, pack ice, or glacial wall (spec 006 §3).
- The assignment is a **pure function of (map seed, position)**,
  computed on demand via seed-keyed hashing. The field is never
  stored in the save; only the seed is (spec 010 §7 keeps saves
  small and replay exact).
- Field lookups draw **nothing** from the event RNG (spec 010 §4
  rule 4): querying the field, however often, cannot perturb the
  event stream. Sensing is read-only over the world.
- Glacial walls partition the map into progression regions; pancake
  and pack ice band around them. Region shape parameters are
  balancing data (section 6).

## 3. Class profiles

Each ice class carries a data profile, read by the world phase:

| axis | effect authority |
|------|------------------|
| break resistance | thrust needed to hold speed (003 §2) |
| prow wear rate | prow degradation per meter (004 §2) |
| fuel cost factor | torque multiplier while breaking (005 §5) |
| yield mix and rate | ingestion resource weights (003 §2) |
| stress event profile | impact frequency and magnitude (005 §4) |

The profile table's values are balancing data; the axes are pinned
here. A class with no profile entry is a spec violation, not a
default.

## 4. Fog of Winter

- Reveal state is a monotonic set: once seen, always seen (spec 006
  §2 rewards investment; it does not re-blind).
- Reveal grows from ship position each tick with a radius scaled by
  the sensor-coverage capability readback (spec 010 §3): frozen or
  shut-down sensors shrink present sight but never erase the map.
- Reveal state serializes with the save; it is the only stored world
  content.
- The exterior renderer presents unrevealed terrain as fog and never
  reads the field through any other path (spec 012 §3).

## 5. Test contract

Headless, alongside the spec 010 §8 suite:

1. **Field determinism.** Same seed: byte-identical class answers
   over a sampled grid; different seeds diverge.
2. **RNG isolation.** A tick that performs field lookups leaves the
   event RNG state untouched unless an event actually fired.
3. **Reveal monotonicity.** No tick shrinks the revealed set; zero
   sensor coverage halts growth without erasing it.
4. **Profile effect.** Crossing from pancake into pack ice changes
   measured fuel burn and impact frequency in the profiled
   direction.

## 6. Out of scope

- Profile numbers, region sizes, biome variety, event rate tables:
  balancing data for a future spec.
- Iceberg Node site placement details: the expedition machine of
  spec 010 §5 already governs site lifecycle; this spec only ties
  sighting probability to revealed terrain.
- Rendering style of terrain and fog: spec 012 and future art specs.
