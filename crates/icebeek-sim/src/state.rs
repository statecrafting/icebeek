//! Shared-state resources: the first slice of the spec 010 section 5
//! domain table. Field-level shapes are implementation latitude; the
//! domain list and its design authority are pinned by the spec.

use std::collections::VecDeque;

use bevy_ecs::prelude::Resource;
use icebeek_events::{Event, EventPayload, EventQueue, ResourceKind, Tick};
use serde::{Deserialize, Serialize};

/// Hull-graph node count for the bootstrap slice. The real graph grows
/// with the hull-graph domain (spec 010 section 5).
pub const HULL_NODES: usize = 8;

/// The tick counter: the only clock the simulation has.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Resource)]
pub struct SimTick(pub Tick);

/// Ordered helm state, written only by the commands phase.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Resource)]
pub struct Helm {
    pub heading_rad: f32,
    /// Ordered throttle in [0, 1]; actual speed is capped by capability.
    pub throttle: f32,
}

/// Ship kinetics domain (specs 003 section 2, 005 section 5).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Resource)]
pub struct ShipKinetics {
    pub position: [f64; 2],
    pub speed: f32,
}

/// Hull graph domain (spec 005 section 4): stress per node in [0, 1].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Resource)]
pub struct HullGraph {
    pub stress: [f32; HULL_NODES],
}

impl Default for HullGraph {
    fn default() -> Self {
        Self {
            stress: [0.0; HULL_NODES],
        }
    }
}

/// Cargo domain (spec 003 section 2), indexed by `ResourceKind`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Resource)]
pub struct CargoHold {
    pub amounts: [u64; ResourceKind::ALL.len()],
}

impl CargoHold {
    pub fn index(resource: ResourceKind) -> usize {
        match resource {
            ResourceKind::FrozenScrap => 0,
            ResourceKind::AncientTech => 1,
            ResourceKind::Biomass => 2,
            ResourceKind::Ice => 3,
        }
    }

    pub fn amount(&self, resource: ResourceKind) -> u64 {
        self.amounts[Self::index(resource)]
    }
}

/// Capability readback (spec 002 section 3 rule 3): interior outcomes
/// the next world phase reads back as available performance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Resource)]
pub struct Capability {
    /// Fraction of nominal thrust available, degraded by hull stress.
    pub available_thrust: f32,
}

impl Default for Capability {
    fn default() -> Self {
        Self {
            available_thrust: 1.0,
        }
    }
}

/// The sim-owned wrapper around the spec 011 event queue, holding the
/// deterministic sequence counter. Serializes whole into saves.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Resource)]
pub struct EventBus {
    pub queue: EventQueue,
    pub next_seq: u64,
}

impl EventBus {
    pub fn emit(&mut self, tick: Tick, payload: EventPayload) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.queue.push(Event { tick, seq, payload });
    }
}

/// Typed player commands: the only outside write path (spec 010
/// section 6). Command types live here in the sim, not in the events
/// crate (spec 011 section 2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Command {
    SetHeading { heading_rad: f32 },
    SetThrottle { throttle: f32 },
}

/// Pending commands, applied by the commands phase in push order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Resource)]
pub struct CommandQueue {
    pub pending: VecDeque<Command>,
}
