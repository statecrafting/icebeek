# Event bus

<!-- Normative authority: specs/011-event-bus/spec.md -->

`crates/icebeek-events` is the vocabulary of what the world can do to
the ship. Everything the exterior throws at the interior travels as a
typed, serializable event; the interior can never learn anything an
event does not carry.

- **Envelope**: emission tick, deterministic sequence number, typed
  payload; spatial families carry a hull-graph node and magnitude.
  Timestamps are tick-denominated, never wall time.
- **Initial families**: Impact (stress spikes at hull nodes),
  Ingestion (intake batches into the hold), Weather (storm and flare
  onset/end, valve and sensor freeze, drone scramble), Expedition
  (anchor, ice-shift warnings, crush progression). New variants and
  families are amendments to spec 011.
- **Queue**: FIFO by (tick, seq); events emitted in the world phase
  of tick T are visible to the interior phase of tick T; backlog is
  legal gameplay pressure and persists in order; the pending queue
  serializes whole into saves.
- **Purity**: serde and std only, no engine crates of any kind, no
  systems, no callbacks, no game logic. Player commands are not in
  this crate; they live in `icebeek-sim` (spec 010 §6).

Territory: the crate directory is claimed when the crate lands; its
manifest names `011-event-bus` from the first commit.
