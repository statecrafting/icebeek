//! The presentation read API (spec 010 section 6, spec 012 section 2).
//!
//! Renderers interpolate between the last two completed ticks. The
//! unit of that contract is the snapshot: a copy of the
//! render-relevant surface, taken by the host after each tick.
//! Holding one mutates nothing and blocks nothing, and snapshots are
//! never serialized.

use bevy_ecs::prelude::Resource;
use icebeek_events::ResourceKind;

use crate::state::{COMPARTMENTS, HULL_NODES};

/// A copy of the render-relevant surface at one completed tick.
#[derive(Debug, Clone, PartialEq)]
pub struct SimSnapshot {
    pub tick: u64,
    pub position: [f64; 2],
    pub heading_rad: f32,
    pub speed: f32,
    pub hull_stress: [f32; HULL_NODES],
    pub compartment_temps: [f32; COMPARTMENTS],
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
            compartment_temps: [0.0; COMPARTMENTS],
            core_temperature: 0.0,
            fuel_fraction: 0.0,
            shutdown_stage: 0,
            cargo: [0; ResourceKind::ALL.len()],
            storm_active: false,
            flare_active: false,
            site_available: false,
            anchored_at_site: false,
            crush_pressure: 0.0,
        }
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
