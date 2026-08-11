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

/// Thermal compartments align one-to-one with hull-graph nodes for the
/// vertical slice; the real room graph arrives with the interior grid
/// domain (spec 010 section 5).
pub const COMPARTMENTS: usize = HULL_NODES;

/// Ambient exterior temperature in degrees C: cold is the default
/// (spec 004 section 5). Balancing placeholder.
pub const AMBIENT_C: f32 = -40.0;

/// Core temperature of a healthy, fed engine, in degrees C.
pub const OPERATING_C: f32 = 90.0;

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

/// Ship kinetics domain (specs 003 section 2, 005 section 5). Total
/// mass sets the torque needed to keep breaking ice; torque sets fuel
/// burn: the efficiency tax on sprawl.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Resource)]
pub struct ShipKinetics {
    pub position: [f64; 2],
    pub speed: f32,
    /// Hull plus everything aboard, in mass units.
    pub total_mass: f32,
    /// Normalized torque demand at the current mass and speed.
    pub torque_demand: f32,
    /// Fuel demand at the current torque, in fuel units per second.
    pub fuel_burn: f32,
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

    /// A fresh run's provisions: enough fuel and coolant stock to keep
    /// the core burning until the first intake comes in (spec 003
    /// section 2; quantities are balancing placeholders).
    pub fn starting_provisions() -> Self {
        let mut hold = Self::default();
        hold.amounts[Self::index(ResourceKind::Biomass)] = 200;
        hold.amounts[Self::index(ResourceKind::Ice)] = 50;
        hold
    }
}

/// Ship systems on the cold-shutdown ladder (spec 004 section 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShipSystem {
    Sensors,
    DroneBays,
    Logistics,
    Propulsion,
}

/// The cold-shutdown ladder: a total order over dependent systems,
/// farthest from the core first, propulsion always last; recovery
/// retraces it in reverse (spec 004 section 3, spec 010 section 5).
pub const SHUTDOWN_LADDER: [ShipSystem; 4] = [
    ShipSystem::Sensors,
    ShipSystem::DroneBays,
    ShipSystem::Logistics,
    ShipSystem::Propulsion,
];

/// Engine core domain (spec 004 section 3): the fuel-burning heart.
/// Core temperature is the master health stat; as it falls, systems
/// shut down along [`SHUTDOWN_LADDER`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Resource)]
pub struct EngineCore {
    /// Degrees C; the master health stat.
    pub temperature: f32,
    /// Fuel staged at the core, in cargo units.
    pub fuel_buffer: f32,
    /// Ladder entries currently shut down, counted from the front.
    pub shutdown_stage: u8,
    /// Ticks accumulated toward the next ladder movement.
    pub ticks_at_stage: u32,
    /// Fractional intake-ice consumption owed by the coolant loop.
    pub coolant_meter: f32,
}

impl Default for EngineCore {
    fn default() -> Self {
        Self {
            temperature: OPERATING_C,
            fuel_buffer: 5.0,
            shutdown_stage: 0,
            ticks_at_stage: 0,
            coolant_meter: 0.0,
        }
    }
}

impl EngineCore {
    /// Whether a system is still powered at the current ladder stage.
    pub fn system_online(&self, system: ShipSystem) -> bool {
        let position = SHUTDOWN_LADDER
            .iter()
            .position(|s| *s == system)
            .expect("every ship system is on the ladder");
        position >= usize::from(self.shutdown_stage)
    }
}

/// Thermal field domain (spec 004 section 5): compartment temperatures
/// in degrees C, indexed like hull-graph nodes for the vertical slice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Resource)]
pub struct ThermalField {
    pub temps: [f32; COMPARTMENTS],
}

impl Default for ThermalField {
    fn default() -> Self {
        // A freshly provisioned ship starts habitable, not frozen.
        Self {
            temps: [15.0; COMPARTMENTS],
        }
    }
}

/// Capability readback (spec 002 section 3 rule 3): interior outcomes
/// the next world phase reads back as available performance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Resource)]
pub struct Capability {
    /// Fraction of nominal thrust available, degraded by hull stress
    /// and zeroed while the shutdown ladder has propulsion down.
    pub available_thrust: f32,
    /// Fraction of nominal sensor coverage, zeroed while the shutdown
    /// ladder has sensors down.
    pub sensor_coverage: f32,
}

impl Default for Capability {
    fn default() -> Self {
        Self {
            available_thrust: 1.0,
            sensor_coverage: 1.0,
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
