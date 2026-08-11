//! The one binary (spec 012 section 5): hosts the Bevy `App`, drives
//! the sim at TICK_HZ regardless of frame rate or focused view, and
//! wires both render layers over the one truth.

use bevy::prelude::*;
use icebeek_render_exterior::ExteriorRenderPlugin;
use icebeek_render_interior::InteriorRenderPlugin;
use icebeek_sim::{SimHandle, SimWorld, TICK_HZ};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((ExteriorRenderPlugin, InteriorRenderPlugin))
        // Fixed seed until the run-creation flow exists; the seed is part
        // of the state (spec 010 section 4 rule 4).
        .insert_resource(SimHandle(SimWorld::new(0)))
        // The fixed-timestep accumulator of spec 010 section 3: Bevy's
        // FixedUpdate schedule is the app-side driver.
        .insert_resource(Time::<Fixed>::from_hz(f64::from(TICK_HZ)))
        .add_systems(FixedUpdate, tick_sim)
        .run();
}

/// The unfocused view idles; the simulation never does (spec 012
/// section 5 rule 2).
fn tick_sim(mut sim: ResMut<SimHandle>) {
    sim.0.tick();
}
