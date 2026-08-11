//! The Macro view (spec 012 section 3): isometric 3D exterior.
//!
//! First visuals: the frozen ocean plane, the ship silhouette moving
//! and turning between interpolated ticks, an Iceberg Node marker
//! when a site is alongside, and storm- or flare-tinted light. All
//! systems read the sim through the snapshot pair the app maintains
//! (spec 012 section 2); nothing here writes simulation state.

use bevy::prelude::*;
use icebeek_sim::SimSnapshots;

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

/// Camera offset from the ship, chosen for an isometric read.
const CAMERA_OFFSET: Vec3 = Vec3::new(26.0, 30.0, 26.0);
/// Illuminance of a clear sky and of a storm-darkened one.
const CLEAR_LUX: f32 = 10_000.0;
const STORM_LUX: f32 = 2_500.0;

pub struct ExteriorRenderPlugin;

impl Plugin for ExteriorRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_exterior).add_systems(
            Update,
            (place_ship, follow_camera, place_site_marker, tint_sky_light),
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
    // The frozen ocean: one large plane; real ice topography arrives
    // with the world-content slice (spec 006 section 3).
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(2000.0, 2000.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.86, 0.92, 0.96),
            perceptual_roughness: 0.9,
            ..default()
        })),
    ));
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
}
