//! Shared-state resources: the first slice of the spec 010 section 5
//! domain table. Field-level shapes are implementation latitude; the
//! domain list and its design authority are pinned by the spec.

use std::collections::{BTreeSet, VecDeque};

use bevy_ecs::prelude::Resource;
use icebeek_events::{Event, EventPayload, EventQueue, ResourceKind, Tick};
use serde::{Deserialize, Serialize};

use crate::grid::{CellAddr, GRID_H, GRID_W, RoomKind, SpineKind};

/// Hull-graph node count for the bootstrap slice. The real graph grows
/// with the hull-graph domain (spec 010 section 5).
pub const HULL_NODES: usize = 8;

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
    /// Anchor order: while set, forward motion stops (spec 006
    /// section 4: anchoring trades motion for extraction time).
    pub anchor_ordered: bool,
}

/// Prow loadout tracks (spec 004 section 2), tier-gated (spec 016
/// section 4). Heavier tracks break ice with less prow wear; the
/// full loadout mechanics (charge, active use) are future spec.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProwTrack {
    #[default]
    Ram,
    HeatedRam,
    ChargedLance,
}

impl ProwTrack {
    /// The tier that unlocks mounting this track (spec 016 section 4).
    pub const fn min_tier(self) -> u8 {
        match self {
            ProwTrack::Ram => 1,
            ProwTrack::HeatedRam => 2,
            ProwTrack::ChargedLance => 3,
        }
    }

    /// Scale on the class-profiled prow wear rate. Balancing data.
    pub const fn wear_factor(self) -> f32 {
        match self {
            ProwTrack::Ram => 1.0,
            ProwTrack::HeatedRam => 0.6,
            ProwTrack::ChargedLance => 0.35,
        }
    }
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
    /// Prow degradation in [0, 1], accrued per world unit broken
    /// through at the ice class's profiled rate (spec 004 section 2
    /// via spec 014 section 3), scaled by the mounted track.
    pub prow_wear: f32,
    /// The mounted, tier-gated prow loadout track (spec 016
    /// section 4).
    pub prow_track: ProwTrack,
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

    /// Whether every per-resource entry of a cost is covered.
    pub fn can_afford(&self, cost: &[u64; ResourceKind::ALL.len()]) -> bool {
        self.amounts
            .iter()
            .zip(cost)
            .all(|(have, need)| have >= need)
    }

    /// Deduct a cost `can_afford` approved.
    pub fn deduct(&mut self, cost: &[u64; ResourceKind::ALL.len()]) {
        for (have, need) in self.amounts.iter_mut().zip(cost) {
            *have -= need;
        }
    }

    /// A fresh run's provisions: enough fuel, coolant stock, and strut
    /// feedstock to keep the core burning and the repair line working
    /// until the first intake comes in (spec 003 section 2; quantities
    /// are balancing placeholders).
    pub fn starting_provisions() -> Self {
        let mut hold = Self::default();
        hold.amounts[Self::index(ResourceKind::Biomass)] = 200;
        hold.amounts[Self::index(ResourceKind::Ice)] = 50;
        hold.amounts[Self::index(ResourceKind::FrozenScrap)] = 40;
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

/// What a drone is for (spec 005 section 2). Scout rovers arrive with
/// the expedition slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DroneKind {
    Repair,
    Logistics,
}

/// A contiguous, inclusive range of grid cell columns a drone
/// patrols (rebased from hull nodes onto the grid, spec 015 section
/// 7). The stored column-to-node mapping resolves which hull nodes
/// the zone serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroneZone {
    pub from: u32,
    pub to: u32,
}

impl DroneZone {
    pub fn contains(&self, column: u32) -> bool {
        column >= self.from && column <= self.to
    }
}

/// Drones are equipment, not characters: uptime, capacity, and
/// maintenance cost only (spec 005 section 2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Drone {
    pub kind: DroneKind,
    pub zone: DroneZone,
    /// Accumulated wear in [0, 1]; uptime is its complement.
    pub wear: f32,
    /// Fractional strut-feedstock consumption owed to the hold.
    pub strut_meter: f32,
}

impl Drone {
    /// Fraction of nominal duty currently available.
    pub fn uptime(&self) -> f32 {
        (1.0 - self.wear).clamp(0.0, 1.0)
    }
}

/// Drone fleet domain (spec 010 section 5). Vec order is spawn order,
/// and spawn order is action order (determinism discipline).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Resource)]
pub struct DroneFleet {
    pub drones: Vec<Drone>,
    /// Ticks of solar-flare scramble left; the whole fleet's logic is
    /// suppressed while nonzero (spec 006 section 5).
    pub scrambled_ticks: u32,
}

impl Default for DroneFleet {
    fn default() -> Self {
        // A fresh run ships with two repair drones splitting the
        // hull's cell columns.
        Self {
            drones: vec![
                Drone {
                    kind: DroneKind::Repair,
                    zone: DroneZone {
                        from: 0,
                        to: (GRID_W / 2 - 1) as u32,
                    },
                    wear: 0.0,
                    strut_meter: 0.0,
                },
                Drone {
                    kind: DroneKind::Repair,
                    zone: DroneZone {
                        from: (GRID_W / 2) as u32,
                        to: (GRID_W - 1) as u32,
                    },
                    wear: 0.0,
                    strut_meter: 0.0,
                },
            ],
            scrambled_ticks: 0,
        }
    }
}

/// Fixed intake valves and external sensors, one of each per
/// hull-row cell column: equipment locations live on grid cells
/// (spec 015 section 7). A nonzero entry is a frozen unit counting
/// down to thaw (spec 006 section 5); weather freeze events target
/// hull nodes and resolve onto the node's column band.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Resource)]
pub struct Equipment {
    pub valve_frozen: [u32; GRID_W],
    pub sensor_frozen: [u32; GRID_W],
}

impl Default for Equipment {
    fn default() -> Self {
        Self {
            valve_frozen: [0; GRID_W],
            sensor_frozen: [0; GRID_W],
        }
    }
}

impl Equipment {
    /// Fraction of units in the given bank that are not frozen.
    pub fn working_fraction(bank: &[u32; GRID_W]) -> f32 {
        let working = bank.iter().filter(|ticks| **ticks == 0).count();
        working as f32 / GRID_W as f32
    }
}

/// Weather machine state, world side (spec 006 section 5). One system
/// at a time for the vertical slice.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WeatherState {
    /// Ticks of super-storm remaining; 0 is clear.
    pub storm_ticks: u32,
    /// Ticks of solar flare remaining; 0 is clear.
    pub flare_ticks: u32,
    /// Storms seen this run (diagnostic surface for tests and views).
    pub storms_seen: u32,
    /// Flares seen this run.
    pub flares_seen: u32,
}

/// Expedition machine state, world side (spec 006 section 4).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ExpeditionState {
    /// An Iceberg Node is alongside and can be anchored to.
    pub site_available: bool,
    /// The ship is anchored at the site with rovers deployed.
    pub anchored_at_site: bool,
    /// Shifting-ice crush pressure while anchored, in [0, 1] and up.
    pub crush_pressure: f32,
    /// Ticks until the next CrushProgress emission while anchored.
    pub crush_countdown: u32,
    /// Ticks until the next rover haul while anchored.
    pub haul_countdown: u32,
}

/// The Fog of Winter (spec 014 section 4): the monotonic reveal set,
/// in world cells. Once seen, always seen; together with the map
/// seed it is the only stored world content. A `BTreeSet` keeps
/// iteration and serialization in sorted, deterministic order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FogOfWinter {
    pub revealed: BTreeSet<(i64, i64)>,
}

impl FogOfWinter {
    pub fn is_revealed(&self, cell: (i64, i64)) -> bool {
        self.revealed.contains(&cell)
    }
}

/// World domain (spec 010 section 5, spec 006): the exterior event
/// generator's state. The ice field itself is never stored: it is a
/// pure function of (`map_seed`, position) in the `world_field`
/// module (spec 014 section 2).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Resource)]
pub struct WorldDomain {
    /// The map seed the whole ice field derives from on demand.
    pub map_seed: u64,
    pub weather: WeatherState,
    pub expedition: ExpeditionState,
    pub fog: FogOfWinter,
}

/// What a tier-1 rule can test (spec 005 section 3: levels,
/// temperatures, stress values).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Condition {
    /// True unconditionally; the ELSE arm of a rule pair.
    Always,
    FuelBufferBelow(f32),
    CoreTempAbove(f32),
    CoreTempBelow(f32),
    StockBelow {
        resource: ResourceKind,
        amount: u64,
    },
    StressAbove {
        node: u32,
        level: f32,
    },
    /// A super-storm is active: tier-2 cross-system vocabulary (spec
    /// 005 section 3, gated by spec 016 section 4).
    StormActive,
}

impl Condition {
    /// The tier that unlocks authoring a rule with this condition
    /// (spec 005 section 3 tiers, enforced per spec 016 section 4).
    pub const fn min_tier(&self) -> u8 {
        match self {
            Condition::StormActive => 2,
            _ => 1,
        }
    }
}

/// What a tier-1 rule can set: routing switches, never state directly
/// (spec 005 section 3).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RuleAction {
    SetFeedEnabled(bool),
    SetCoolantEnabled(bool),
}

/// One player-authored gate: IF condition THEN action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomationRule {
    /// Assigned from `next_rule_id` at add; stable across reorders.
    pub id: u32,
    pub condition: Condition,
    pub action: RuleAction,
}

/// Automation rules domain (spec 010 section 5): the stored list order
/// is the player-visible evaluation order; later rules win conflicting
/// writes (spec 010 section 5, spec 005 section 3).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Resource)]
pub struct AutomationRules {
    pub rules: Vec<AutomationRule>,
    pub next_rule_id: u32,
}

/// The routing switches rules act on. Written by rule evaluation at
/// the start of the interior phase, read by the systems behind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Resource)]
pub struct Routing {
    /// Belt switch: the hold-to-core feed line.
    pub feed_enabled: bool,
    /// Valve: the coolant loop.
    pub coolant_enabled: bool,
}

impl Default for Routing {
    fn default() -> Self {
        Self {
            feed_enabled: true,
            coolant_enabled: true,
        }
    }
}

/// Thermal field domain (spec 004 section 5): per-cell temperatures
/// in degrees C over the interior grid, row-major (spec 015 section
/// 3: the field migrated here from the bootstrap compartment ring).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Resource)]
pub struct ThermalField {
    pub temps: Vec<f32>,
}

impl Default for ThermalField {
    fn default() -> Self {
        // A freshly provisioned ship starts habitable, not frozen.
        Self {
            temps: vec![15.0; GRID_W * GRID_H],
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
    /// ladder has sensors down and degraded by frozen sensors.
    pub sensor_coverage: f32,
    /// Fraction of intake valves working; scales intake yield in the
    /// next tick's world phase (spec 006 section 5).
    pub intake_capacity: f32,
}

impl Default for Capability {
    fn default() -> Self {
        Self {
            available_thrust: 1.0,
            sensor_coverage: 1.0,
            intake_capacity: 1.0,
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
    SetHeading {
        heading_rad: f32,
    },
    SetThrottle {
        throttle: f32,
    },
    /// Append a rule; its id comes from the monotonic counter.
    AddRule {
        condition: Condition,
        action: RuleAction,
    },
    /// Remove the rule with this id; unknown ids are ignored.
    RemoveRule {
        id: u32,
    },
    /// Reassign a drone's patrol zone; out-of-range indices are
    /// ignored, node bounds are clamped to the hull.
    SetDroneZone {
        drone: u32,
        zone: DroneZone,
    },
    /// Order or release the anchor (spec 006 section 4).
    SetAnchor {
        anchored: bool,
    },
    /// Emergency manual override: thaw the frozen valves and sensors
    /// across a hull node's column band immediately (spec 006
    /// section 5).
    ManualThaw {
        node: u32,
    },
    /// Place a room module (spec 015 section 5); validated against
    /// bounds, overlap, and cargo cost.
    PlaceRoom {
        kind: RoomKind,
        origin: CellAddr,
    },
    /// Tear a room out, refunding the fixed fraction of its cost.
    RemoveRoom {
        room: u32,
    },
    /// Emergency remove-without-refund plus immediate mass relief
    /// (spec 005 section 5): the lever costs real material.
    Jettison {
        room: u32,
    },
    /// Lay a typed spine edge across one cell boundary.
    LayEdge {
        kind: SpineKind,
        from: CellAddr,
        to: CellAddr,
    },
    /// Remove a spine edge (no refund at the edge's unit cost).
    RemoveEdge {
        edge: u32,
    },
    /// Mount a prow loadout track; tier-gated like any build order
    /// (spec 016 section 4).
    MountProwTrack {
        track: ProwTrack,
    },
    /// Advance to the next tier: requires the complete blueprint set
    /// plus the research spend (spec 016 section 3); rejected typed
    /// otherwise.
    AdvanceTier,
}

/// Pending commands, applied by the commands phase in push order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Resource)]
pub struct CommandQueue {
    pub pending: VecDeque<Command>,
}
