//! The one binary (spec 012 section 5): hosts the Bevy `App`, drives
//! the sim at TICK_HZ regardless of frame rate or focused view, keeps
//! the snapshot pair renderers interpolate between, captures input
//! into typed commands, and owns the view switch.

use bevy::prelude::*;
use icebeek_render_exterior::{ExteriorCamera, ExteriorRenderPlugin};
use icebeek_render_interior::{InteriorCamera, InteriorRenderPlugin};
use icebeek_sim::{Command as SimCommand, Helm, SimHandle, SimSnapshots, SimWorld, TICK_HZ};

/// Which view has focus (spec 002 section 2). Presentation state
/// only: the unfocused view idles, the simulation never does.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ViewFocus {
    #[default]
    Exterior,
    Interior,
}

/// Ordered-throttle step per keypress.
const THROTTLE_STEP: f32 = 0.25;
/// Ordered-heading step per held frame, in radians.
const HEADING_STEP: f32 = 0.03;

fn main() {
    // Fixed seed until the run-creation flow exists; the seed is part
    // of the state (spec 010 section 4 rule 4).
    let sim = SimWorld::new(0);
    let snapshots = SimSnapshots::new(sim.snapshot());
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((ExteriorRenderPlugin, InteriorRenderPlugin))
        .insert_resource(SimHandle(sim))
        .insert_resource(snapshots)
        // The fixed-timestep accumulator of spec 010 section 3: Bevy's
        // FixedUpdate schedule is the app-side driver.
        .insert_resource(Time::<Fixed>::from_hz(f64::from(TICK_HZ)))
        .init_resource::<ViewFocus>()
        .add_systems(FixedUpdate, (tick_sim, capture_snapshot).chain())
        .add_systems(Update, (capture_input, apply_view_focus))
        .run();
}

/// The unfocused view idles; the simulation never does (spec 012
/// section 5 rule 2).
fn tick_sim(mut sim: ResMut<SimHandle>) {
    sim.0.tick();
}

/// After each completed tick, roll the pair renderers blend between
/// (spec 012 section 2 rule 3).
fn capture_snapshot(sim: Res<SimHandle>, mut snapshots: ResMut<SimSnapshots>) {
    let next = sim.0.snapshot();
    snapshots.advance(next);
}

/// Input capture: keys become typed commands; no handler reaches into
/// simulation state directly (spec 012 section 5 rule 3). W/S step
/// the throttle, A/D hold the rudder, Space toggles the anchor, Tab
/// switches views.
fn capture_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut sim: ResMut<SimHandle>,
    mut focus: ResMut<ViewFocus>,
) {
    if keys.just_pressed(KeyCode::Tab) {
        *focus = match *focus {
            ViewFocus::Exterior => ViewFocus::Interior,
            ViewFocus::Interior => ViewFocus::Exterior,
        };
    }
    let helm = sim.0.world().resource::<Helm>().clone();
    if keys.just_pressed(KeyCode::KeyW) {
        sim.0.push_command(SimCommand::SetThrottle {
            throttle: (helm.throttle + THROTTLE_STEP).min(1.0),
        });
    }
    if keys.just_pressed(KeyCode::KeyS) {
        sim.0.push_command(SimCommand::SetThrottle {
            throttle: (helm.throttle - THROTTLE_STEP).max(0.0),
        });
    }
    if keys.pressed(KeyCode::KeyA) {
        sim.0.push_command(SimCommand::SetHeading {
            heading_rad: helm.heading_rad + HEADING_STEP,
        });
    }
    if keys.pressed(KeyCode::KeyD) {
        sim.0.push_command(SimCommand::SetHeading {
            heading_rad: helm.heading_rad - HEADING_STEP,
        });
    }
    if keys.just_pressed(KeyCode::Space) {
        sim.0.push_command(SimCommand::SetAnchor {
            anchored: !helm.anchor_ordered,
        });
    }
}

/// The view switch (spec 012 section 5 rule 4): one deliberate swap
/// of camera focus. Pure presentation; it enqueues nothing and blocks
/// nothing in the sim.
fn apply_view_focus(
    focus: Res<ViewFocus>,
    mut exterior: Query<&mut Camera, (With<ExteriorCamera>, Without<InteriorCamera>)>,
    mut interior: Query<&mut Camera, (With<InteriorCamera>, Without<ExteriorCamera>)>,
) {
    if !focus.is_changed() {
        return;
    }
    for mut camera in &mut exterior {
        camera.is_active = *focus == ViewFocus::Exterior;
    }
    for mut camera in &mut interior {
        camera.is_active = *focus == ViewFocus::Interior;
    }
}
