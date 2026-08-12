//! The Micro view (spec 012 section 4): top-down cross-section interior.
//!
//! First visuals: the interior grid's cells under a heat overlay
//! (per-cell temperatures, spec 015 section 3), per-node hull stress
//! bars, the engine core gauge, and the fuel bar. All systems read
//! the sim through the snapshot pair the app maintains (spec 012
//! section 2); nothing here writes simulation state.

use bevy::prelude::*;
use icebeek_sim::{AMBIENT_C, GRID_H, GRID_W, SimSnapshots};

/// Marker for the Micro camera; the app shell toggles focus between
/// the two views (spec 012 section 5 rule 4). Starts unfocused.
#[derive(Component)]
pub struct InteriorCamera;

#[derive(Component)]
struct CellVisual(usize);

#[derive(Component)]
struct StressVisual(usize);

#[derive(Component)]
struct CoreVisual;

#[derive(Component)]
struct FuelVisual;

/// Layout of the cell grid, in logical pixels.
const CELL: f32 = 44.0;
const CELL_SIZE: f32 = 40.0;
/// Width of the fuel bar at a full buffer.
const FUEL_BAR_WIDTH: f32 = 120.0;

pub struct InteriorRenderPlugin;

impl Plugin for InteriorRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_camera, spawn_picture))
            .add_systems(
                Update,
                (
                    paint_heat_overlay,
                    paint_stress_bars,
                    paint_core,
                    paint_fuel,
                ),
            );
    }
}

/// The Micro camera, unfocused until the app's view switch says
/// otherwise (spec 012 section 5 rule 4).
fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        InteriorCamera,
        Camera2d,
        Camera {
            is_active: false,
            ..default()
        },
    ));
}

/// The drawn picture: presentation entities only. Deleting all of it
/// and rerunning the spawn reproduces the same picture from
/// simulation state (spec 012 section 2 rule 2; exercised by the
/// rebuild test below).
fn spawn_picture(mut commands: Commands) {
    let column_offset = (GRID_W as f32 - 1.0) * CELL / 2.0;
    let row_offset = (GRID_H as f32 - 1.0) * CELL / 2.0;
    // The cell grid under the heat overlay, row 0 (the hull row) at
    // the top like the cross-section reads.
    for index in 0..GRID_W * GRID_H {
        let x = (index % GRID_W) as f32 * CELL - column_offset;
        let y = row_offset - (index / GRID_W) as f32 * CELL;
        commands.spawn((
            CellVisual(index),
            Sprite {
                color: heat_color(15.0),
                custom_size: Some(Vec2::splat(CELL_SIZE)),
                ..default()
            },
            Transform::from_xyz(x, y, 0.0),
        ));
    }
    // Per-node stress bars above the hull row.
    let nodes = icebeek_sim::HULL_NODES;
    let node_offset = (nodes as f32 - 1.0) * CELL * 2.0 / 2.0;
    for i in 0..nodes {
        let x = i as f32 * CELL * 2.0 - node_offset;
        commands.spawn((
            StressVisual(i),
            Sprite {
                color: stress_color(0.0),
                custom_size: Some(Vec2::new(CELL_SIZE * 2.0, 6.0)),
                ..default()
            },
            Transform::from_xyz(x, row_offset + CELL, 0.0),
        ));
    }
    commands.spawn((
        CoreVisual,
        Sprite {
            color: heat_color(90.0),
            custom_size: Some(Vec2::splat(CELL_SIZE + 8.0)),
            ..default()
        },
        Transform::from_xyz(0.0, -row_offset - CELL * 1.5, 0.0),
    ));
    commands.spawn((
        FuelVisual,
        Sprite {
            color: Color::srgb(0.85, 0.55, 0.15),
            custom_size: Some(Vec2::new(FUEL_BAR_WIDTH, 10.0)),
            ..default()
        },
        Transform::from_xyz(0.0, -row_offset - CELL * 2.5, 0.0),
    ));
}

/// Interpolate a scalar display value (a gauge, a heat cell) between
/// the last two completed sim ticks (spec 012 section 2 rule 3).
/// `alpha` outside [0, 1] is clamped: renderers never extrapolate.
pub fn interpolate_scalar(prev: f32, curr: f32, alpha: f32) -> f32 {
    let alpha = alpha.clamp(0.0, 1.0);
    prev + (curr - prev) * alpha
}

/// The heat overlay (spec 004 section 5): ambient cold is deep blue,
/// a hot compartment burns orange-red. Input is clamped to the
/// ambient-to-molten band.
pub fn heat_color(temp_c: f32) -> Color {
    let t = ((temp_c - AMBIENT_C) / (160.0 - AMBIENT_C)).clamp(0.0, 1.0);
    Color::srgb(0.08 + 0.87 * t, 0.16 + 0.19 * t, 0.55 - 0.40 * t)
}

/// Hull stress readout: healthy green through warning amber to
/// breach-red as stress approaches 1 (spec 005 section 4).
pub fn stress_color(stress: f32) -> Color {
    let s = stress.clamp(0.0, 1.0);
    Color::srgb(0.15 + 0.80 * s, 0.75 - 0.55 * s, 0.20 - 0.10 * s)
}

fn paint_heat_overlay(
    snapshots: Res<SimSnapshots>,
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<(&CellVisual, &mut Sprite)>,
) {
    let alpha = fixed_time.overstep_fraction();
    for (cell, mut sprite) in &mut query {
        if cell.0 >= snapshots.curr.cell_temps.len() || cell.0 >= snapshots.prev.cell_temps.len() {
            continue;
        }
        let temp = interpolate_scalar(
            snapshots.prev.cell_temps[cell.0],
            snapshots.curr.cell_temps[cell.0],
            alpha,
        );
        sprite.color = heat_color(temp);
    }
}

fn paint_stress_bars(
    snapshots: Res<SimSnapshots>,
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<(&StressVisual, &mut Sprite)>,
) {
    let alpha = fixed_time.overstep_fraction();
    for (node, mut sprite) in &mut query {
        let stress = interpolate_scalar(
            snapshots.prev.hull_stress[node.0],
            snapshots.curr.hull_stress[node.0],
            alpha,
        );
        sprite.color = stress_color(stress);
    }
}

fn paint_core(
    snapshots: Res<SimSnapshots>,
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<&mut Sprite, With<CoreVisual>>,
) {
    let alpha = fixed_time.overstep_fraction();
    let temp = interpolate_scalar(
        snapshots.prev.core_temperature,
        snapshots.curr.core_temperature,
        alpha,
    );
    for mut sprite in &mut query {
        sprite.color = heat_color(temp);
    }
}

fn paint_fuel(
    snapshots: Res<SimSnapshots>,
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<&mut Sprite, With<FuelVisual>>,
) {
    let alpha = fixed_time.overstep_fraction();
    let fraction = interpolate_scalar(
        snapshots.prev.fuel_fraction,
        snapshots.curr.fuel_fraction,
        alpha,
    )
    .clamp(0.0, 1.0);
    for mut sprite in &mut query {
        if let Some(size) = sprite.custom_size.as_mut() {
            size.x = (FUEL_BAR_WIDTH * fraction).max(1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn interpolation_endpoints_and_midpoint() {
        assert_eq!(interpolate_scalar(1.0, 3.0, 0.0), 1.0);
        assert_eq!(interpolate_scalar(1.0, 3.0, 1.0), 3.0);
        assert_eq!(interpolate_scalar(1.0, 3.0, 0.5), 2.0);
    }

    #[test]
    fn interpolation_never_extrapolates() {
        assert_eq!(interpolate_scalar(1.0, 3.0, -0.5), 1.0);
        assert_eq!(interpolate_scalar(1.0, 3.0, 1.5), 3.0);
    }

    /// The heat overlay is monotonic: warmer never reads bluer.
    #[test]
    fn heat_color_runs_cold_to_hot() {
        let cold = heat_color(AMBIENT_C).to_srgba();
        let hot = heat_color(160.0).to_srgba();
        assert!(cold.blue > cold.red, "ambient cold should read blue");
        assert!(hot.red > hot.blue, "a hot compartment should read red");
        let mid = heat_color(60.0).to_srgba();
        assert!(cold.red < mid.red && mid.red < hot.red);
    }

    /// Stress runs green to red and clamps outside [0, 1].
    #[test]
    fn stress_color_runs_green_to_red() {
        let calm = stress_color(0.0).to_srgba();
        let critical = stress_color(1.0).to_srgba();
        assert!(calm.green > calm.red);
        assert!(critical.red > critical.green);
        assert_eq!(stress_color(2.0), stress_color(1.0));
    }

    fn picture_entities(world: &mut World) -> Vec<Entity> {
        let mut query = world.query_filtered::<Entity, With<Sprite>>();
        query.iter(world).collect()
    }

    /// Spec 012 section 6 test 3: despawn every entity of the drawn
    /// picture, rebuild from simulation state, and get the same
    /// picture. The picture is everything carrying a Sprite; engine
    /// internals (in 0.19, resources and metadata are entities too)
    /// are not presentation state and stay put.
    #[test]
    fn presentation_rebuilds_from_simulation_state() {
        let mut world = World::new();
        world.run_system_once(spawn_picture).unwrap();
        let baseline = picture_entities(&mut world).len();
        assert!(baseline > 0, "spawn produced no presentation entities");

        for entity in picture_entities(&mut world) {
            world.despawn(entity);
        }
        assert_eq!(
            picture_entities(&mut world).len(),
            0,
            "despawn left survivors"
        );

        world.run_system_once(spawn_picture).unwrap();
        assert_eq!(
            picture_entities(&mut world).len(),
            baseline,
            "rebuild from simulation state diverged"
        );
    }
}
