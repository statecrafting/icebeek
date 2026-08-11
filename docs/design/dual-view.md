# The two views

<!-- Normative authority: specs/002-dual-view-architecture/spec.md -->

You play one game through two lenses.

**Outside (the Macro).** An isometric 3D exterior. The ship is a toy-sized
behemoth grinding across an endless white plate. You chart courses, weigh
thrust against drag, watch storms roll in, and feel every glacier impact
as a camera shake and a speed drop. Out here the world happens *to* you.

**Inside (the Micro).** A top-down cross-section of the hull, deck by
deck. The wind cuts out; machinery hums. Every tile matters: belts, pipes,
data lines, heat. In here, nothing happens that you did not build.

The trick that makes it work: the views never render each other. A
starboard impact outside becomes a stress alert on a hull node inside;
your repair line answers it. Fuel refined inside becomes thrust available
outside. One simulation, two honest windows onto it.

Concept references: `../ideas/licensed-image.jpeg` (exterior),
`../ideas/images.jpeg` (interior cross-section).
