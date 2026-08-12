# The interior grid

<!-- Normative authority: specs/015-interior-grid/spec.md -->

The Micro view's building system made real: per-deck cell grids with
strictly finite hull space, rooms placed onto rectangular footprints,
and the logistics spine (belts for solids, pipes for fluids, data
lines for signals) as typed edges between them. Every room carries
mass (the sprawl tax), heat behavior (the thermal field lives on the
grid), and machine buffers; an unpowered or freezing room stalls
before it breaks.

Build, tear-out, and re-route are typed commands validated whole in
the commands phase: an invalid order is rejected with a reason, never
half-applied. Removal refunds a fixed fraction, because refitting is
the expected activity, and jettison is the refundless emergency lever
that trades real material for immediate mass relief.

Breaches disable cells and sever spine edges, and cascades follow the
topology the player actually built: redundancy is not decoration, it
is the difference between an incident and a spiral.
