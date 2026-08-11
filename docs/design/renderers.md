# Renderers

<!-- Normative authority: specs/012-renderers/spec.md -->

Two views, one shell, all read-only over the one simulation:

- **`icebeek-render-exterior`** (the Macro): isometric 3D terrain,
  ice, ship silhouette, weather, expeditions, prow impacts. Audio:
  wind and grinding ice.
- **`icebeek-render-interior`** (the Micro): top-down cross-section
  decks, rooms, belts and pipes, drones, heat overlay. Audio:
  machinery hum. No horizon, no terrain, no hull exterior.
- **`icebeek-app`**: the one binary (`icebeek`). Owns the window,
  drives the sim tick at TICK_HZ regardless of frame rate or focused
  view, captures input into typed commands, orchestrates save/load,
  and owns the single fast camera/audio crossfade between views.

The rules that matter:

- Renderers read state through queries and push typed commands; they
  never mutate simulation state. Presentation entities are disposable
  and never serialized: despawn everything and the picture rebuilds
  from simulation state.
- Visuals interpolate between the last two sim ticks; no
  extrapolation, no frame time in gameplay.
- The unfocused view idles; the simulation never does.
- Only these three crates may depend on the `bevy` umbrella.

Territory: the three crate directories are claimed as the crates
land; each manifest names `012-renderers` from its first commit.
