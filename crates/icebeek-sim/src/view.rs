//! The presentation read API (spec 010 section 6, spec 012 section 2).
//!
//! Renderers interpolate between the last two completed ticks. The
//! unit of that contract is the snapshot: a copy of the
//! render-relevant surface, taken by the host after each tick.
//! Holding one mutates nothing and blocks nothing, and snapshots are
//! never serialized.

use bevy_ecs::prelude::Resource;
use icebeek_events::ResourceKind;

use crate::state::HULL_NODES;
use crate::world_field::IceClass;

/// Side length, in cells, of the terrain window a snapshot carries.
pub const TERRAIN_VIEW_SIDE: usize = 33;

/// The terrain the exterior draws (spec 014, spec 012 section 3): a
/// square window of ice classes and reveal flags centered on the
/// ship's cell. The renderer reads the field and the Fog of Winter
/// through this surface and no other path.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainView {
    /// The cell at the window center (the ship's cell).
    pub center: (i64, i64),
    /// Window side length in cells; the vectors hold `side * side`
    /// row-major entries.
    pub side: usize,
    pub classes: Vec<IceClass>,
    pub revealed: Vec<bool>,
}

impl TerrainView {
    /// The cell offset from the window's top-left corner for a
    /// row-major index.
    pub fn cell_at(&self, index: usize) -> (i64, i64) {
        let half = self.side as i64 / 2;
        let row = (index / self.side) as i64;
        let col = (index % self.side) as i64;
        (self.center.0 + col - half, self.center.1 + row - half)
    }
}

/// A copy of the render-relevant surface at one completed tick.
#[derive(Debug, Clone, PartialEq)]
pub struct SimSnapshot {
    pub tick: u64,
    pub position: [f64; 2],
    pub heading_rad: f32,
    pub speed: f32,
    pub hull_stress: [f32; HULL_NODES],
    /// Interior grid dimensions and the per-cell temperatures the
    /// Micro's heat overlay reads (spec 015 section 3), row-major.
    pub grid_width: usize,
    pub grid_height: usize,
    pub cell_temps: Vec<f32>,
    pub core_temperature: f32,
    /// Fill fraction of the core-side fuel buffer, in [0, 1].
    pub fuel_fraction: f32,
    pub shutdown_stage: u8,
    pub cargo: [u64; ResourceKind::ALL.len()],
    pub storm_active: bool,
    pub flare_active: bool,
    pub site_available: bool,
    pub anchored_at_site: bool,
    pub crush_pressure: f32,
    pub terrain: TerrainView,
}

/// The pair of consecutive snapshots the app maintains and render
/// systems blend between (spec 012 section 2 rule 3). Presentation
/// state: disposable, never part of a save.
#[derive(Debug, Clone, Resource)]
pub struct SimSnapshots {
    pub prev: SimSnapshot,
    pub curr: SimSnapshot,
}

impl SimSnapshots {
    /// Seed both slots from the same snapshot; interpolation between
    /// equal endpoints is the identity, so the first frames are still.
    pub fn new(snapshot: SimSnapshot) -> Self {
        Self {
            prev: snapshot.clone(),
            curr: snapshot,
        }
    }

    /// Roll the pair forward after a completed tick.
    pub fn advance(&mut self, next: SimSnapshot) {
        self.prev = std::mem::replace(&mut self.curr, next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_at_tick(tick: u64) -> SimSnapshot {
        SimSnapshot {
            tick,
            position: [tick as f64, 0.0],
            heading_rad: 0.0,
            speed: 0.0,
            hull_stress: [0.0; HULL_NODES],
            grid_width: 2,
            grid_height: 1,
            cell_temps: vec![0.0; 2],
            core_temperature: 0.0,
            fuel_fraction: 0.0,
            shutdown_stage: 0,
            cargo: [0; ResourceKind::ALL.len()],
            storm_active: false,
            flare_active: false,
            site_available: false,
            anchored_at_site: false,
            crush_pressure: 0.0,
            terrain: TerrainView {
                center: (0, 0),
                side: 1,
                classes: vec![IceClass::OpenWater],
                revealed: vec![false],
            },
        }
    }

    /// A terrain window's row-major indexing centers on the ship's
    /// cell: the middle index is the center, corners are offset by
    /// half the side.
    #[test]
    fn terrain_window_indexing_centers_on_the_ship() {
        let side = 5;
        let view = TerrainView {
            center: (10, -4),
            side,
            classes: vec![IceClass::OpenWater; side * side],
            revealed: vec![false; side * side],
        };
        assert_eq!(view.cell_at(side * side / 2), (10, -4));
        assert_eq!(view.cell_at(0), (8, -6));
        assert_eq!(view.cell_at(side * side - 1), (12, -2));
    }

    /// The pair always holds two consecutive captures: advance moves
    /// curr to prev and installs the new snapshot as curr.
    #[test]
    fn advance_rolls_the_pair() {
        let mut snapshots = SimSnapshots::new(snapshot_at_tick(0));
        assert_eq!(snapshots.prev, snapshots.curr);
        snapshots.advance(snapshot_at_tick(1));
        assert_eq!(snapshots.prev.tick, 0);
        assert_eq!(snapshots.curr.tick, 1);
        snapshots.advance(snapshot_at_tick(2));
        assert_eq!(snapshots.prev.tick, 1);
        assert_eq!(snapshots.curr.tick, 2);
    }
}
