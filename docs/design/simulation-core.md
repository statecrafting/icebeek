# Simulation core

<!-- Normative authority: specs/010-simulation-core/spec.md -->

`crates/icebeek-sim` is the one crate of truth: every gameplay
mechanic of specs 003 through 007 executes here, headless, on a fixed
tick. Nothing gameplay-shaped lives in a renderer.

The load-bearing choices:

- **`bevy_ecs` alone.** The ECS library is a direct dependency; the
  `bevy` umbrella crate is forbidden inside the sim, so the renderer
  surface can never leak in through a feature flag.
- **Fixed tick, host-driven.** `TICK_HZ` = 20. The sim advances only
  when the host calls `tick()`; renderers interpolate between ticks.
  Per-tick phase order: commands, world (exterior, may enqueue
  events), interior (consumes events, runs the control plane),
  capability readback.
- **Determinism as discipline.** Single-threaded total system order
  (ambiguities are test failures), no wall clock, no iteration over
  unordered collections into state, one seeded RNG that only exterior
  event generation may touch. Interior systems have no randomness.
- **Commands are the only outside write path.** Typed, serializable
  player commands queue in; renderers read state, never write it.
- **Saves are state.** Serialized state plus pending queues plus RNG
  state; seed plus command log replays a run byte-identically, and
  the test suite enforces exactly that.

Territory: the crate directory is claimed when the crate lands (an
approved spec may not claim nonexistent units); its manifest names
`010-simulation-core` from the first commit.
