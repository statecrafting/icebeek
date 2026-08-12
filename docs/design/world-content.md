# World content

<!-- Normative authority: specs/014-world-content/spec.md -->

The frozen ocean becomes state. The ice field is a pure function of
the map seed: ask any position, get its class (open water, pancake,
pack, glacial wall), and no save ever stores a tile. Only the Fog of
Winter's reveal state persists: a monotonic once-seen-always-seen set
that grows around the ship at a radius sensor coverage buys.

Each ice class is a mechanical profile, not a texture: break
resistance against thrust, prow wear per meter, a fuel-cost factor on
torque, the ingestion yield mix, and the impact event profile. Course
choice through the field is the strategic layer spec 003 promised;
glacial walls partition the map into the progression regions spec 007
gates behind.

Field lookups never touch the event RNG: sensing is read-only over
the world, so scouting cannot perturb what the world does next. Same
seed, same ocean, byte for byte.
