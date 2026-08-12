//! The Macro view (spec 012 section 3): isometric 3D exterior.
//!
//! The frozen ocean and its ice topography (spec 014): a tile window
//! of ice classes around the ship, with unrevealed terrain painted
//! as the Fog of Winter; the ship silhouette moving and turning
//! between interpolated ticks; an Iceberg Node marker when a site is
//! alongside; storm- or flare-tinted light. All systems read the sim
//! through the snapshot pair the app maintains (spec 012 section 2),
//! and the terrain only through the snapshot's terrain window (spec
//! 014 section 4); nothing here writes simulation state.

use bevy::prelude::*;
use icebeek_sim::{CELL_UNITS, IceClass, SimSnapshots, TERRAIN_VIEW_SIDE, cell_center};

/// Marker for the Macro camera; the app shell toggles focus between
/// the two views (spec 012 section 5 rule 4).
#[derive(Component)]
pub struct ExteriorCamera;

#[derive(Component)]
struct ShipVisual;

#[derive(Component)]
struct SiteVisual;

#[derive(Component)]
struct SkyLight;

/// One tile of the terrain window; `index` is row-major into the
/// snapshot's [`icebeek_sim::TerrainView`].
#[derive(Component)]
struct TerrainTile {
    index: usize,
}

/// Camera offset from the ship, chosen for an isometric read.
const CAMERA_OFFSET: Vec3 = Vec3::new(26.0, 30.0, 26.0);
/// Illuminance of a clear sky and of a storm-darkened one.
const CLEAR_LUX: f32 = 10_000.0;
const STORM_LUX: f32 = 2_500.0;
/// The Fog of Winter: what unrevealed terrain paints as.
const FOG_COLOR: Color = Color::srgb(0.06, 0.07, 0.1);
/// Height of the terrain tiles above the ocean plane.
const TILE_HEIGHT: f32 = 0.02;

/// The presentation palette for the spec 014 ice classes.
pub fn class_color(class: IceClass) -> Color {
    match class {
        IceClass::OpenWater => Color::srgb(0.1, 0.22, 0.35),
        IceClass::PancakeIce => Color::srgb(0.78, 0.85, 0.91),
        IceClass::PackIce => Color::srgb(0.92, 0.95, 0.97),
        IceClass::GlacialWall => Color::srgb(0.52, 0.66, 0.8),
    }
}

/// Unrevealed terrain is fog, whatever its class (spec 014 section
/// 4): the exterior never leaks topography the sensors have not
/// earned.
pub fn tile_color(class: IceClass, revealed: bool) -> Color {
    if revealed {
        class_color(class)
    } else {
        FOG_COLOR
    }
}

pub struct ExteriorRenderPlugin;

impl Plugin for ExteriorRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_exterior).add_systems(
            Update,
            (
                place_ship,
                follow_camera,
                place_site_marker,
                tint_sky_light,
                paint_terrain,
            ),
        );
    }
}

/// Presentation entities only: deleting all of this and rerunning the
/// spawn reproduces the same picture from simulation state (spec 012
/// section 2 rule 2).
fn spawn_exterior(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        ExteriorCamera,
        Camera3d::default(),
        Transform::from_translation(CAMERA_OFFSET).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        SkyLight,
        DirectionalLight {
            illuminance: CLEAR_LUX,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(20.0, 40.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // The backdrop plane beneath the tile window, fog-colored so the
    // world beyond sensor reach reads as the Fog of Winter.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(2000.0, 2000.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: FOG_COLOR,
            perceptual_roughness: 0.9,
            ..default()
        })),
    ));
    // The ice topography (spec 014): one tile per cell of the
    // snapshot's terrain window, repositioned and repainted per
    // frame from the snapshot alone.
    let tile_mesh = meshes.add(
        Plane3d::default()
            .mesh()
            .size(CELL_UNITS as f32, CELL_UNITS as f32),
    );
    for index in 0..TERRAIN_VIEW_SIDE * TERRAIN_VIEW_SIDE {
        commands.spawn((
            TerrainTile { index },
            Mesh3d(tile_mesh.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: FOG_COLOR,
                perceptual_roughness: 0.95,
                ..default()
            })),
            Transform::from_xyz(0.0, TILE_HEIGHT, 0.0),
        ));
    }
    commands.spawn((
        ShipVisual,
        Mesh3d(meshes.add(Cuboid::new(4.2, 1.4, 1.8))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.24, 0.27, 0.33),
            ..default()
        })),
        Transform::from_xyz(0.0, 0.7, 0.0),
    ));
    commands.spawn((
        SiteVisual,
        Mesh3d(meshes.add(Cylinder::new(1.6, 7.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.55, 0.76, 0.95),
            ..default()
        })),
        Transform::from_xyz(0.0, 3.5, 0.0),
        Visibility::Hidden,
    ));
}

/// Interpolate a world position between the last two completed sim
/// ticks (spec 012 section 2 rule 3). `alpha` is the accumulator
/// fraction in [0, 1]; values outside are clamped: renderers never
/// extrapolate.
pub fn interpolate_position(prev: [f64; 2], curr: [f64; 2], alpha: f64) -> [f64; 2] {
    let alpha = alpha.clamp(0.0, 1.0);
    [
        prev[0] + (curr[0] - prev[0]) * alpha,
        prev[1] + (curr[1] - prev[1]) * alpha,
    ]
}

/// Interpolate the ship's heading. The helm accumulates a continuous
/// angle (no wraparound seam), so a clamped linear blend is exact.
pub fn interpolate_heading(prev: f32, curr: f32, alpha: f32) -> f32 {
    let alpha = alpha.clamp(0.0, 1.0);
    prev + (curr - prev) * alpha
}

/// Map a sim-plane position onto the scene: sim +x is scene +x, sim
/// +y is scene -z, so heading 0 sails toward scene +x.
fn scene_position(position: [f64; 2], height: f32) -> Vec3 {
    Vec3::new(position[0] as f32, height, -(position[1] as f32))
}

/// The interpolated ship position for this frame (spec 012 section 2
/// rule 3).
fn interpolated_ship(snapshots: &SimSnapshots, alpha: f32) -> (Vec3, f32) {
    let position = interpolate_position(
        snapshots.prev.position,
        snapshots.curr.position,
        f64::from(alpha),
    );
    let heading = interpolate_heading(
        snapshots.prev.heading_rad,
        snapshots.curr.heading_rad,
        alpha,
    );
    (scene_position(position, 0.7), heading)
}

fn place_ship(
    snapshots: Res<SimSnapshots>,
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<&mut Transform, (With<ShipVisual>, Without<ExteriorCamera>)>,
) {
    let (ship_pos, heading) = interpolated_ship(&snapshots, fixed_time.overstep_fraction());
    for mut transform in &mut query {
        transform.translation = ship_pos;
        transform.rotation = Quat::from_rotation_y(heading);
    }
}

/// The camera keeps its isometric offset from the ship and never cuts
/// on its own; the deliberate view switch lives in the app shell.
fn follow_camera(
    snapshots: Res<SimSnapshots>,
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<&mut Transform, (With<ExteriorCamera>, Without<ShipVisual>)>,
) {
    let (ship_pos, _) = interpolated_ship(&snapshots, fixed_time.overstep_fraction());
    for mut transform in &mut query {
        *transform =
            Transform::from_translation(ship_pos + CAMERA_OFFSET).looking_at(ship_pos, Vec3::Y);
    }
}

/// An Iceberg Node alongside shows as a marker off the bow quarter;
/// it disappears when the site is spent (spec 006 section 4).
fn place_site_marker(
    snapshots: Res<SimSnapshots>,
    mut query: Query<(&mut Transform, &mut Visibility), With<SiteVisual>>,
) {
    let visible = snapshots.curr.site_available || snapshots.curr.anchored_at_site;
    let ship_pos = scene_position(snapshots.curr.position, 0.0);
    for (mut transform, mut visibility) in &mut query {
        *visibility = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        transform.translation = ship_pos + Vec3::new(9.0, 3.5, -9.0);
    }
}

/// Terrain presentation (spec 014, spec 012 section 3): every tile
/// takes its cell and color from the current snapshot's terrain
/// window, fog for anything unrevealed. The window recenters with
/// the ship, so tiles reposition rather than respawn.
fn paint_terrain(
    snapshots: Res<SimSnapshots>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut query: Query<(
        &TerrainTile,
        &mut Transform,
        &MeshMaterial3d<StandardMaterial>,
    )>,
) {
    let terrain = &snapshots.curr.terrain;
    for (tile, mut transform, material) in &mut query {
        if tile.index >= terrain.classes.len() {
            continue;
        }
        let cell = terrain.cell_at(tile.index);
        transform.translation = scene_position(cell_center(cell), TILE_HEIGHT);
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.base_color =
                tile_color(terrain.classes[tile.index], terrain.revealed[tile.index]);
        }
    }
}

/// Weather presentation: storms darken the sky, flares warm its color
/// (spec 012 section 3; the mechanics travel as events, spec 006).
fn tint_sky_light(
    snapshots: Res<SimSnapshots>,
    mut query: Query<&mut DirectionalLight, With<SkyLight>>,
) {
    for mut light in &mut query {
        light.illuminance = if snapshots.curr.storm_active {
            STORM_LUX
        } else {
            CLEAR_LUX
        };
        light.color = if snapshots.curr.flare_active {
            Color::srgb(1.0, 0.85, 0.7)
        } else {
            Color::WHITE
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolation_endpoints_and_midpoint() {
        let prev = [0.0, -2.0];
        let curr = [10.0, 2.0];
        assert_eq!(interpolate_position(prev, curr, 0.0), prev);
        assert_eq!(interpolate_position(prev, curr, 1.0), curr);
        assert_eq!(interpolate_position(prev, curr, 0.5), [5.0, 0.0]);
    }

    #[test]
    fn interpolation_never_extrapolates() {
        let prev = [0.0, 0.0];
        let curr = [10.0, 10.0];
        assert_eq!(interpolate_position(prev, curr, -1.0), prev);
        assert_eq!(interpolate_position(prev, curr, 2.0), curr);
        assert_eq!(interpolate_heading(1.0, 2.0, -1.0), 1.0);
        assert_eq!(interpolate_heading(1.0, 2.0, 5.0), 2.0);
    }

    /// Sim +y maps to scene -z so a positive heading turns the same
    /// way in both spaces.
    #[test]
    fn scene_mapping_flips_the_lateral_axis() {
        let mapped = scene_position([3.0, 4.0], 0.5);
        assert_eq!(mapped, Vec3::new(3.0, 0.5, -4.0));
    }

    /// Spec 014 section 4: unrevealed terrain paints as fog whatever
    /// its class; no class leaks through the Fog of Winter.
    #[test]
    fn fog_hides_every_class() {
        for class in [
            IceClass::OpenWater,
            IceClass::PancakeIce,
            IceClass::PackIce,
            IceClass::GlacialWall,
        ] {
            assert_eq!(tile_color(class, false), FOG_COLOR);
            assert_ne!(
                tile_color(class, true),
                FOG_COLOR,
                "{class:?} is indistinguishable from fog when revealed"
            );
        }
    }

    /// The four class colors read as four different terrains.
    #[test]
    fn class_colors_are_distinct() {
        let classes = [
            IceClass::OpenWater,
            IceClass::PancakeIce,
            IceClass::PackIce,
            IceClass::GlacialWall,
        ];
        for a in classes {
            for b in classes {
                if a != b {
                    assert_ne!(class_color(a), class_color(b), "{a:?} and {b:?} collide");
                }
            }
        }
    }
}
