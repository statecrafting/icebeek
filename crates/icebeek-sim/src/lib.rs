//! The deterministic fixed-tick simulation core (spec 010).
//!
//! One `bevy_ecs` `World` of truth, advanced only by explicit `tick()`
//! calls from the host. Four phases per tick, single-threaded, totally
//! ordered: commands, world, interior, readback (spec 010 section 3).
//! The bootstrap slice implements a thin vertical path through every
//! phase so the determinism test contract is real from the first commit.

mod grid;
mod rng;
mod save;
mod state;
mod tech;
mod view;
mod world_field;

use bevy_ecs::prelude::*;
use bevy_ecs::schedule::{LogLevel, ScheduleBuildSettings, SingleThreadedExecutor};
use bevy_ecs::system::ScheduleSystem;
use serde::{Deserialize, Serialize};

pub use grid::{
    BuildLog, BuildRejection, CellAddr, DECKS, GRID_H, GRID_W, InteriorGrid, RejectReason, Room,
    RoomKind, RoomSpec, SpineEdge, SpineKind, room_spec,
};
pub use icebeek_events as events;
pub use rng::SimRng;
pub use save::{Migration, SAVE_FORMAT_VERSION, SaveError};
pub use state::{
    AMBIENT_C, AutomationRule, AutomationRules, Capability, CargoHold, Command, CommandQueue,
    Condition, Drone, DroneFleet, DroneKind, DroneZone, EngineCore, Equipment, EventBus,
    ExpeditionState, FogOfWinter, HULL_NODES, Helm, HullGraph, OPERATING_C, ProwTrack, Routing,
    RuleAction, SHUTDOWN_LADDER, ShipKinetics, ShipSystem, SimTick, ThermalField, WeatherState,
    WorldDomain,
};
pub use tech::{
    BLUEPRINT_POOL, DUPLICATE_BLUEPRINT_RESEARCH, MAX_TIER, RESEARCH_PER_TECH, TechDomain,
    TierProfile, blueprints_required, research_cost, tier_profile,
};
pub use view::{SimSnapshot, SimSnapshots, TERRAIN_VIEW_SIDE, TerrainView};
pub use world_field::{
    CELL_UNITS, IceClass, IceClassProfile, cell_center, cell_of, class_at, class_of_cell, profile,
};

use icebeek_events::{EventPayload, ExpeditionEvent, ResourceKind, SalvageEvent, WeatherEvent};

/// Ticks per simulated second (spec 010 section 3). Amend spec 010 to
/// tune; saves record the rate they were written under.
pub const TICK_HZ: u32 = 20;
pub const TICK_SECONDS: f32 = 1.0 / TICK_HZ as f32;

/// Nominal top speed in world units per second at full throttle with an
/// undamaged hull. Balancing data placeholder (spec 010 section 10).
const MAX_SPEED: f32 = 6.0;
/// Per-second impact probability scale while moving at full speed.
const IMPACT_CHANCE_PER_SECOND: f32 = 0.8;
/// Per-second ingestion probability while moving.
const INGEST_CHANCE_PER_SECOND: f32 = 2.0;
/// Stress a repair drone at full uptime removes per second from the
/// node it serves.
const REPAIR_RATE_PER_SECOND: f32 = 0.05;
/// Frozen scrap a working repair drone consumes per second as strut
/// feedstock, metered in whole units.
const STRUT_UNITS_PER_SECOND: f32 = 0.1;
/// Wear a drone accrues per second of active work.
const WEAR_PER_SECOND: f32 = 0.01;
/// Wear level at which the bay pulls a drone in for maintenance.
const MAINTENANCE_WEAR: f32 = 0.5;
/// Frozen scrap one maintenance pass consumes.
const MAINTENANCE_SCRAP_UNITS: u64 = 2;

/// Unloaded hull mass, in mass units. Balancing placeholder, like the
/// engine and thermal constants below (spec 010 section 10).
const BASE_MASS: f32 = 1000.0;
/// Mass of one cargo unit of any resource, in mass units.
const CARGO_UNIT_MASS: f32 = 1.0;
/// Fuel demand at zero torque, in fuel units per second: the burn that
/// keeps the core warm while the ship idles.
const IDLE_BURN_PER_SECOND: f32 = 0.15;
/// Additional fuel demand per unit of normalized torque, per second.
const TORQUE_BURN_PER_SECOND: f32 = 0.35;
/// Fuel units the core-side buffer holds.
const FUEL_BUFFER_CAP: f32 = 10.0;
/// Whole cargo units the feed line moves to the buffer per tick while
/// logistics is online and the buffer has room.
const FEED_UNITS_PER_TICK: u64 = 1;
/// Core heating per fuel unit burned, in degrees C.
const HEAT_PER_FUEL: f32 = 12.0;
/// Fractional leak of core heat toward its compartment, per second.
const CORE_LEAK_PER_SECOND: f32 = 0.05;
/// Core temperature below which the shutdown ladder advances.
const STALL_C: f32 = 40.0;
/// Core temperature above which the ladder retraces. The gap against
/// STALL_C is hysteresis: between them the ladder holds.
const RESTART_C: f32 = 60.0;
/// Ticks between ladder movements while past a threshold.
const LADDER_TICKS: u32 = 40;
/// Core temperature above which the coolant loop draws intake ice.
const COOLANT_THRESHOLD_C: f32 = 110.0;
/// Ice units the coolant loop melts per second while active.
const COOLANT_UNITS_PER_SECOND: f32 = 0.5;
/// Core cooling per melted ice unit, in degrees C.
const COOLING_PER_ICE: f32 = 30.0;
/// Cell-to-cell diffusion rate over the grid, per neighbor per second.
const DIFFUSION_PER_SECOND: f32 = 0.4;
/// Cell leak toward the ambient exterior, per second.
const HULL_LEAK_PER_SECOND: f32 = 0.02;
/// Core-to-cell thermal coupling into the core room's cells, per
/// second.
const CORE_COUPLING_PER_SECOND: f32 = 0.1;

/// Per-second chance of a super-storm onset while the sky is clear.
const STORM_CHANCE_PER_SECOND: f32 = 0.02;
/// Shortest super-storm, in ticks; the RNG adds up to as much again.
const STORM_BASE_TICKS: u32 = 200;
/// Per-second chance a storm freezes a valve or sensor somewhere.
const FREEZE_CHANCE_PER_SECOND: f32 = 0.5;
/// Ticks a frozen valve or sensor takes to thaw on its own.
const THAW_TICKS: u32 = 300;
/// Per-second chance of a solar flare onset while the sky is clear.
const FLARE_CHANCE_PER_SECOND: f32 = 0.01;
/// Shortest solar flare, in ticks; the RNG adds up to as much again.
const FLARE_BASE_TICKS: u32 = 100;
/// Per-second chance an active flare scrambles drone logic.
const SCRAMBLE_CHANCE_PER_SECOND: f32 = 0.4;
/// Ticks a drone scramble lasts.
const SCRAMBLE_TICKS: u32 = 100;
/// Per-second chance of sighting an Iceberg Node while under way.
const SITE_CHANCE_PER_SECOND: f32 = 0.05;
/// Crush pressure accrued per second while anchored at a site.
const CRUSH_RATE_PER_SECOND: f32 = 0.01;
/// Ticks between CrushProgress emissions while anchored.
const CRUSH_EVENT_TICKS: u32 = 20;
/// Hull stress each CrushProgress event distributes to every node,
/// scaled by the pressure it carries.
const CRUSH_STRESS_SCALE: f32 = 0.02;
/// Per-second chance of an ice-shift warning while anchored.
const WARNING_CHANCE_PER_SECOND: f32 = 0.1;
/// Ticks between rover hauls while anchored.
const HAUL_TICKS: u32 = 40;
/// Reveal radius of the Fog of Winter at full sensor coverage, in
/// world cells (spec 014 section 4). Balancing placeholder.
const REVEAL_RADIUS_CELLS: f32 = 6.0;
/// Per-second chance of a trapped-wreck blueprint find while
/// breaking pack ice at full intensity (spec 016 section 3).
const WRECK_SALVAGE_CHANCE_PER_SECOND: f32 = 0.02;
/// Per-second chance of a wall-cache blueprint find while ramming a
/// glacial wall at full intensity.
const WALL_CACHE_CHANCE_PER_SECOND: f32 = 0.05;
/// Chance one rover haul comes back carrying a vault blueprint.
const NODE_VAULT_CHANCE: f32 = 0.15;

/// The four phases of a tick, in their fixed total order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Commands,
    World,
    Interior,
    Readback,
}

/// Phase-entry log for the schedule-integrity test. Presentation-free
/// diagnostics; not part of the save.
#[derive(Debug, Clone, Default, Resource)]
pub struct PhaseLog(pub Vec<Phase>);

/// RNG snapshots around the interior phase, captured every tick for the
/// interior-purity assertion (spec 010 section 8 test 4). Not saved.
#[derive(Debug, Clone, Default)]
pub struct TickTrace {
    pub rng_after_world: Option<SimRng>,
    pub rng_after_interior: Option<SimRng>,
    pub phases: Vec<Phase>,
}

/// A save is the serialized shared state plus both pending queues and
/// the RNG state, nothing else (spec 010 section 7). On disk it
/// travels as the payload of the spec 017 envelope, which carries the
/// format version, crate version, and tick rate; [`SimWorld::save_bytes`]
/// wraps it and [`SimWorld::from_save_bytes`] enforces the
/// compatibility rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveState {
    pub tick: SimTick,
    pub helm: Helm,
    pub kinetics: ShipKinetics,
    pub hull: HullGraph,
    pub engine: EngineCore,
    pub thermal: ThermalField,
    pub grid: InteriorGrid,
    pub drones: DroneFleet,
    pub rules: AutomationRules,
    pub routing: Routing,
    pub equipment: Equipment,
    pub world: WorldDomain,
    pub tech: TechDomain,
    pub cargo: CargoHold,
    pub capability: Capability,
    pub rng: SimRng,
    pub events: EventBus,
    pub commands: CommandQueue,
}

/// The simulation world and its four phase schedules. The host owns the
/// accumulator and calls [`SimWorld::tick`]; there is no internal clock
/// (spec 010 section 3).
pub struct SimWorld {
    world: World,
    commands_phase: Schedule,
    world_phase: Schedule,
    interior_phase: Schedule,
    readback_phase: Schedule,
    last_trace: TickTrace,
}

impl SimWorld {
    /// A fresh run from a seed. The seed is part of the state (spec 010
    /// section 4 rule 4).
    pub fn new(seed: u64) -> Self {
        let mut world = World::new();
        world.insert_resource(SimTick::default());
        world.insert_resource(Helm::default());
        world.insert_resource(ShipKinetics::default());
        world.insert_resource(HullGraph::default());
        world.insert_resource(EngineCore::default());
        world.insert_resource(ThermalField::default());
        world.insert_resource(InteriorGrid::default());
        world.insert_resource(DroneFleet::default());
        world.insert_resource(AutomationRules::default());
        world.insert_resource(Routing::default());
        world.insert_resource(Equipment::default());
        world.insert_resource(WorldDomain {
            map_seed: seed,
            ..WorldDomain::default()
        });
        world.insert_resource(TechDomain::default());
        world.insert_resource(CargoHold::starting_provisions());
        world.insert_resource(Capability::default());
        world.insert_resource(SimRng::from_seed(seed));
        world.insert_resource(EventBus::default());
        world.insert_resource(CommandQueue::default());
        world.insert_resource(PhaseLog::default());
        world.insert_resource(BuildLog::default());
        Self::with_world(world)
    }

    fn with_world(world: World) -> Self {
        Self {
            world,
            commands_phase: phase_schedule((mark_commands, apply_commands).chain()),
            world_phase: phase_schedule(
                (
                    mark_world,
                    drive_kinetics,
                    reveal_terrain,
                    generate_ice_events,
                    generate_weather,
                    run_expedition,
                )
                    .chain(),
            ),
            interior_phase: phase_schedule(
                (
                    mark_interior,
                    evaluate_rules,
                    consume_events,
                    thaw_equipment,
                    run_spine,
                    run_refineries,
                    feed_engine,
                    burn_fuel,
                    route_coolant,
                    update_thermal_field,
                    advance_shutdown_ladder,
                    run_drones,
                    maintain_drones,
                )
                    .chain(),
            ),
            readback_phase: phase_schedule((mark_readback, read_back_capability).chain()),
            last_trace: TickTrace::default(),
        }
    }

    /// Push a typed player command; it applies in the commands phase of
    /// the next tick, in push order.
    pub fn push_command(&mut self, command: Command) {
        self.world
            .resource_mut::<CommandQueue>()
            .pending
            .push_back(command);
    }

    /// Advance exactly one tick: commands, world, interior, readback.
    pub fn tick(&mut self) {
        self.world.resource_mut::<PhaseLog>().0.clear();

        self.commands_phase.run(&mut self.world);
        self.world_phase.run(&mut self.world);
        let rng_after_world = self.world.resource::<SimRng>().clone();
        self.interior_phase.run(&mut self.world);
        let rng_after_interior = self.world.resource::<SimRng>().clone();
        self.readback_phase.run(&mut self.world);

        self.world.resource_mut::<SimTick>().0 += 1;
        self.last_trace = TickTrace {
            rng_after_world: Some(rng_after_world),
            rng_after_interior: Some(rng_after_interior),
            phases: self.world.resource::<PhaseLog>().0.clone(),
        };
    }

    /// Read-only access to the one truth. Renderers read through this
    /// and write nothing (spec 012 section 2 rule 1).
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Diagnostics from the most recent tick (not part of the save).
    /// The presentation read path (spec 010 section 6, spec 012
    /// section 2): copy the render-relevant surface at the last
    /// completed tick. Holding a snapshot mutates nothing and never
    /// blocks the sim.
    pub fn snapshot(&self) -> SimSnapshot {
        let kinetics = self.world.resource::<ShipKinetics>();
        let helm = self.world.resource::<Helm>();
        let engine = self.world.resource::<EngineCore>();
        let domain = self.world.resource::<WorldDomain>();
        let center = world_field::cell_of(kinetics.position);
        let half = TERRAIN_VIEW_SIDE as i64 / 2;
        let mut classes = Vec::with_capacity(TERRAIN_VIEW_SIDE * TERRAIN_VIEW_SIDE);
        let mut revealed = Vec::with_capacity(TERRAIN_VIEW_SIDE * TERRAIN_VIEW_SIDE);
        for dy in -half..=half {
            for dx in -half..=half {
                let cell = (center.0 + dx, center.1 + dy);
                classes.push(world_field::class_of_cell(domain.map_seed, cell));
                revealed.push(domain.fog.is_revealed(cell));
            }
        }
        SimSnapshot {
            tick: self.world.resource::<SimTick>().0,
            position: kinetics.position,
            heading_rad: helm.heading_rad,
            speed: kinetics.speed,
            hull_stress: self.world.resource::<HullGraph>().stress,
            grid_width: GRID_W,
            grid_height: GRID_H,
            cell_temps: self.world.resource::<ThermalField>().temps.clone(),
            core_temperature: engine.temperature,
            fuel_fraction: (engine.fuel_buffer / FUEL_BUFFER_CAP).clamp(0.0, 1.0),
            shutdown_stage: engine.shutdown_stage,
            cargo: self.world.resource::<CargoHold>().amounts,
            storm_active: domain.weather.storm_ticks > 0,
            flare_active: domain.weather.flare_ticks > 0,
            site_available: domain.expedition.site_available,
            anchored_at_site: domain.expedition.anchored_at_site,
            crush_pressure: domain.expedition.crush_pressure,
            terrain: TerrainView {
                center,
                side: TERRAIN_VIEW_SIDE,
                classes,
                revealed,
            },
        }
    }

    /// Read-only sensing over the ice field (spec 014 section 2): a
    /// pure lookup in (map seed, position) that touches no RNG and no
    /// state, however often it is called.
    pub fn ice_class_at(&self, position: [f64; 2]) -> IceClass {
        world_field::class_at(self.world.resource::<WorldDomain>().map_seed, position)
    }

    pub fn last_trace(&self) -> &TickTrace {
        &self.last_trace
    }

    pub fn save(&self) -> SaveState {
        SaveState {
            tick: self.world.resource::<SimTick>().clone(),
            helm: self.world.resource::<Helm>().clone(),
            kinetics: self.world.resource::<ShipKinetics>().clone(),
            hull: self.world.resource::<HullGraph>().clone(),
            engine: self.world.resource::<EngineCore>().clone(),
            thermal: self.world.resource::<ThermalField>().clone(),
            grid: self.world.resource::<InteriorGrid>().clone(),
            drones: self.world.resource::<DroneFleet>().clone(),
            rules: self.world.resource::<AutomationRules>().clone(),
            routing: self.world.resource::<Routing>().clone(),
            equipment: self.world.resource::<Equipment>().clone(),
            world: self.world.resource::<WorldDomain>().clone(),
            tech: self.world.resource::<TechDomain>().clone(),
            cargo: self.world.resource::<CargoHold>().clone(),
            capability: self.world.resource::<Capability>().clone(),
            rng: self.world.resource::<SimRng>().clone(),
            events: self.world.resource::<EventBus>().clone(),
            commands: self.world.resource::<CommandQueue>().clone(),
        }
    }

    /// Rehydrate a world from an in-process [`SaveState`]. Envelope
    /// and version checks live on the byte path
    /// ([`SimWorld::from_save_bytes`]); a `SaveState` in hand is
    /// already current-format by construction.
    pub fn from_save(save: SaveState) -> Self {
        let mut world = World::new();
        world.insert_resource(save.tick);
        world.insert_resource(save.helm);
        world.insert_resource(save.kinetics);
        world.insert_resource(save.hull);
        world.insert_resource(save.engine);
        world.insert_resource(save.thermal);
        world.insert_resource(save.grid);
        world.insert_resource(save.drones);
        world.insert_resource(save.rules);
        world.insert_resource(save.routing);
        world.insert_resource(save.equipment);
        world.insert_resource(save.world);
        world.insert_resource(save.tech);
        world.insert_resource(save.cargo);
        world.insert_resource(save.capability);
        world.insert_resource(save.rng);
        world.insert_resource(save.events);
        world.insert_resource(save.commands);
        world.insert_resource(PhaseLog::default());
        world.insert_resource(BuildLog::default());
        Self::with_world(world)
    }

    /// Serialize the full save: the spec 017 envelope (format
    /// version, crate version, tick rate) around the state payload.
    pub fn save_bytes(&self) -> Vec<u8> {
        save::encode(&self.save())
    }

    /// Load a save from bytes under the spec 017 compatibility
    /// rules: the same format loads, an older format migrates when a
    /// complete stepwise chain exists, and everything else refuses
    /// with a typed [`SaveError`] naming the versions involved.
    pub fn from_save_bytes(bytes: &[u8]) -> Result<Self, SaveError> {
        Ok(Self::from_save(save::decode(bytes)?))
    }
}

/// Single-threaded, totally ordered, ambiguity-checked schedule (spec
/// 010 section 4 rule 1).
fn phase_schedule<M>(systems: impl IntoScheduleConfigs<ScheduleSystem, M>) -> Schedule {
    let mut schedule = Schedule::default();
    schedule.set_executor(SingleThreadedExecutor::new());
    schedule.set_build_settings(ScheduleBuildSettings {
        ambiguity_detection: LogLevel::Error,
        ..Default::default()
    });
    schedule.add_systems(systems);
    schedule
}

fn mark_commands(mut log: ResMut<PhaseLog>) {
    log.0.push(Phase::Commands);
}

fn mark_world(mut log: ResMut<PhaseLog>) {
    log.0.push(Phase::World);
}

fn mark_interior(mut log: ResMut<PhaseLog>) {
    log.0.push(Phase::Interior);
}

fn mark_readback(mut log: ResMut<PhaseLog>) {
    log.0.push(Phase::Readback);
}

/// Commands phase: apply queued player commands in push order. Build
/// and refit orders validate here (spec 015 section 5): an invalid
/// order drops with a typed rejection into the build log and touches
/// nothing; a valid one applies whole.
#[allow(clippy::too_many_arguments)]
fn apply_commands(
    mut queue: ResMut<CommandQueue>,
    mut helm: ResMut<Helm>,
    mut rules: ResMut<AutomationRules>,
    mut fleet: ResMut<DroneFleet>,
    mut equipment: ResMut<Equipment>,
    mut interior: ResMut<InteriorGrid>,
    mut cargo: ResMut<CargoHold>,
    mut kinetics: ResMut<ShipKinetics>,
    mut tech: ResMut<TechDomain>,
    mut build_log: ResMut<BuildLog>,
) {
    build_log.0.clear();
    let reject = |log: &mut BuildLog, command: Command, reason: RejectReason| {
        log.0.push(BuildRejection { command, reason });
    };
    while let Some(command) = queue.pending.pop_front() {
        match command {
            Command::SetHeading { heading_rad } => helm.heading_rad = heading_rad,
            Command::SetThrottle { throttle } => helm.throttle = throttle.clamp(0.0, 1.0),
            Command::SetAnchor { anchored } => helm.anchor_ordered = anchored,
            Command::ManualThaw { node } => {
                let node = (node as usize % HULL_NODES) as u8;
                for x in 0..GRID_W {
                    if interior.hull_node_of_column[x] == node {
                        equipment.valve_frozen[x] = 0;
                        equipment.sensor_frozen[x] = 0;
                    }
                }
            }
            Command::AddRule { condition, action } => {
                // Rule vocabulary is tier-gated (spec 005 section 3
                // tiers, spec 016 section 4).
                if condition.min_tier() > tech.tier {
                    reject(
                        &mut build_log,
                        Command::AddRule { condition, action },
                        RejectReason::TierGated,
                    );
                    continue;
                }
                let id = rules.next_rule_id;
                rules.next_rule_id += 1;
                rules.rules.push(AutomationRule {
                    id,
                    condition,
                    action,
                });
            }
            Command::RemoveRule { id } => rules.rules.retain(|rule| rule.id != id),
            Command::SetDroneZone { drone, zone } => {
                if let Some(drone) = fleet.drones.get_mut(drone as usize) {
                    let last = (GRID_W - 1) as u32;
                    let from = zone.from.min(last);
                    drone.zone = DroneZone {
                        from,
                        to: zone.to.clamp(from, last),
                    };
                }
            }
            Command::PlaceRoom { kind, origin } => {
                if let Some(reason) = interior.placement_rejection(kind, origin) {
                    reject(&mut build_log, Command::PlaceRoom { kind, origin }, reason);
                    continue;
                }
                // The tier gate (spec 016 section 4): rejected like
                // any invalid build order, no partial state.
                if room_spec(kind).min_tier > tech.tier {
                    reject(
                        &mut build_log,
                        Command::PlaceRoom { kind, origin },
                        RejectReason::TierGated,
                    );
                    continue;
                }
                let cost = room_spec(kind).build_cost;
                if !cargo.can_afford(&cost) {
                    reject(
                        &mut build_log,
                        Command::PlaceRoom { kind, origin },
                        RejectReason::Unaffordable,
                    );
                    continue;
                }
                cargo.deduct(&cost);
                let id = interior.next_room_id;
                interior.next_room_id += 1;
                interior.rooms.push(Room {
                    id,
                    kind,
                    origin,
                    input_buffer: 0,
                    output_buffer: 0,
                });
            }
            Command::RemoveRoom { room } | Command::Jettison { room } => {
                let refunds = matches!(command, Command::RemoveRoom { .. });
                let Some(index) = interior.rooms.iter().position(|r| r.id == room) else {
                    reject(&mut build_log, command, RejectReason::UnknownRoom);
                    continue;
                };
                if interior.rooms[index].kind == RoomKind::EngineCore {
                    reject(&mut build_log, command, RejectReason::FixedRoom);
                    continue;
                }
                let removed = interior.rooms.remove(index);
                if refunds {
                    // The pinned refund (spec 015 section 5): a fixed
                    // fraction of the build cost comes back; the rest
                    // is the declared loss of refit.
                    for (slot, cost) in removed.spec().build_cost.iter().enumerate() {
                        cargo.amounts[slot] += cost / grid::REFUND_DIVISOR;
                    }
                }
            }
            Command::LayEdge { kind, from, to } => {
                if let Some(reason) = interior.edge_rejection(kind, from, to) {
                    reject(&mut build_log, Command::LayEdge { kind, from, to }, reason);
                    continue;
                }
                let scrap = CargoHold::index(ResourceKind::FrozenScrap);
                if cargo.amounts[scrap] < grid::EDGE_COST_SCRAP {
                    reject(
                        &mut build_log,
                        Command::LayEdge { kind, from, to },
                        RejectReason::Unaffordable,
                    );
                    continue;
                }
                cargo.amounts[scrap] -= grid::EDGE_COST_SCRAP;
                let id = interior.next_edge_id;
                interior.next_edge_id += 1;
                interior.edges.push(SpineEdge { id, kind, from, to });
            }
            Command::RemoveEdge { edge } => {
                let before = interior.edges.len();
                interior.edges.retain(|e| e.id != edge);
                if interior.edges.len() == before {
                    reject(
                        &mut build_log,
                        Command::RemoveEdge { edge },
                        RejectReason::UnknownEdge,
                    );
                }
            }
            Command::MountProwTrack { track } => {
                if track.min_tier() > tech.tier {
                    reject(
                        &mut build_log,
                        Command::MountProwTrack { track },
                        RejectReason::TierGated,
                    );
                    continue;
                }
                kinetics.prow_track = track;
            }
            Command::AdvanceTier => {
                let next = tech.tier + 1;
                if next > tech::MAX_TIER {
                    reject(
                        &mut build_log,
                        Command::AdvanceTier,
                        RejectReason::TierGated,
                    );
                    continue;
                }
                // Tier N+1 requires its blueprint set complete plus
                // the research spend (spec 016 section 3).
                if !tech::blueprints_required(next)
                    .iter()
                    .all(|blueprint| tech.blueprints.contains(blueprint))
                {
                    reject(
                        &mut build_log,
                        Command::AdvanceTier,
                        RejectReason::IncompleteBlueprints,
                    );
                    continue;
                }
                let cost = tech::research_cost(next);
                if tech.research < cost {
                    reject(
                        &mut build_log,
                        Command::AdvanceTier,
                        RejectReason::Unaffordable,
                    );
                    continue;
                }
                // The whole paradigm profile applies from this tick:
                // every consumer derives it from the tier, so no tick
                // observes a mixed profile (spec 016 section 5).
                tech.research -= cost;
                tech.tier = next;
            }
        }
    }
}

/// World phase: gross motion, capped by the capability the interior
/// reported last tick (spec 002 section 3 rule 3) and priced by the
/// ice class under the prow (spec 014 section 3): break resistance
/// cuts speed at a given thrust, the fuel cost factor multiplies
/// torque while breaking, and prow wear accrues per world unit
/// broken through. Mass sets torque, torque sets fuel demand (spec
/// 005 section 5); the interior burns against that demand in the
/// same tick.
fn drive_kinetics(
    mut kinetics: ResMut<ShipKinetics>,
    helm: Res<Helm>,
    capability: Res<Capability>,
    cargo: Res<CargoHold>,
    interior: Res<InteriorGrid>,
    domain: Res<WorldDomain>,
    tech: Res<TechDomain>,
) {
    let profile = world_field::profile(world_field::class_at(domain.map_seed, kinetics.position));
    let speed = if helm.anchor_ordered {
        0.0
    } else {
        helm.throttle.min(capability.available_thrust)
            * MAX_SPEED
            * (1.0 - profile.break_resistance)
    };
    kinetics.speed = speed;
    // Total mass is hull plus cargo plus every placed room: the
    // sprawl half of the spec 005 section 5 efficiency tax, which is
    // also what makes Jettison immediate mass relief (spec 015
    // section 5).
    let cargo_units: u64 = cargo.amounts.iter().sum();
    kinetics.total_mass = BASE_MASS + cargo_units as f32 * CARGO_UNIT_MASS + interior.room_mass();
    kinetics.torque_demand =
        kinetics.total_mass * speed / (BASE_MASS * MAX_SPEED) * profile.fuel_cost_factor;
    // The tier paradigm re-prices the whole burn line (spec 016
    // section 5); the mounted track scales prow wear (spec 016
    // section 4).
    kinetics.fuel_burn = (IDLE_BURN_PER_SECOND + TORQUE_BURN_PER_SECOND * kinetics.torque_demand)
        * tech.profile().fuel_burn_factor;
    let step = f64::from(speed) * f64::from(TICK_SECONDS);
    let wear_rate = profile.prow_wear_per_unit * kinetics.prow_track.wear_factor();
    kinetics.prow_wear = (kinetics.prow_wear + wear_rate * step as f32).min(1.0);
    kinetics.position[0] += f64::from(helm.heading_rad.cos()) * step;
    kinetics.position[1] += f64::from(helm.heading_rad.sin()) * step;
}

/// World phase: the Fog of Winter (spec 014 section 4). The reveal
/// set only ever grows, in deterministic cell order, from the ship's
/// cell with a radius scaled by last tick's sensor-coverage
/// readback: zero coverage halts growth and erases nothing. No RNG.
fn reveal_terrain(
    kinetics: Res<ShipKinetics>,
    capability: Res<Capability>,
    mut domain: ResMut<WorldDomain>,
) {
    let radius = REVEAL_RADIUS_CELLS * capability.sensor_coverage;
    if radius <= 0.0 {
        return;
    }
    let center = world_field::cell_of(kinetics.position);
    let reach = radius.floor() as i64;
    let radius_sq = f64::from(radius) * f64::from(radius);
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            if (dx * dx + dy * dy) as f64 <= radius_sq {
                domain.fog.revealed.insert((center.0 + dx, center.1 + dy));
            }
        }
    }
}

/// World phase: the event generator, the only RNG consumer in the
/// simulation (spec 010 section 4 rule 4). The ice class under the
/// prow sets the stress event profile and the yield mix and rate
/// (spec 014 section 3); the field lookup itself draws nothing from
/// the RNG.
fn generate_ice_events(
    tick: Res<SimTick>,
    kinetics: Res<ShipKinetics>,
    capability: Res<Capability>,
    domain: Res<WorldDomain>,
    mut rng: ResMut<SimRng>,
    mut bus: ResMut<EventBus>,
) {
    if kinetics.speed <= 0.0 {
        return;
    }
    let class = world_field::class_at(domain.map_seed, kinetics.position);
    let profile = world_field::profile(class);
    let intensity = kinetics.speed / MAX_SPEED;
    let impact_chance =
        IMPACT_CHANCE_PER_SECOND * profile.impact_chance_factor * intensity * TICK_SECONDS;
    if rng.next_f32() < impact_chance {
        let node = rng.next_range(HULL_NODES as u32);
        let magnitude =
            profile.impact_magnitude_base + profile.impact_magnitude_spread * rng.next_f32();
        bus.emit(tick.0, EventPayload::Impact { node, magnitude });
    }
    // Frozen intake valves cut yield: last tick's readback scales the
    // ingestion chance (spec 006 section 5, spec 002 section 3.3).
    let ingest_chance = INGEST_CHANCE_PER_SECOND
        * profile.yield_rate
        * intensity
        * capability.intake_capacity
        * TICK_SECONDS;
    if rng.next_f32() < ingest_chance {
        let total: u32 = profile.yield_weights.iter().sum();
        if total > 0 {
            let mut draw = rng.next_range(total);
            let mut resource = ResourceKind::ALL[0];
            for (index, weight) in profile.yield_weights.iter().enumerate() {
                if draw < *weight {
                    resource = ResourceKind::ALL[index];
                    break;
                }
                draw -= *weight;
            }
            let amount = 1 + rng.next_range(5);
            bus.emit(tick.0, EventPayload::Ingestion { resource, amount });
        }
    }
    // Blueprints come from the world (spec 016 section 3): trapped
    // wrecks while breaking pack ice, caches while ramming glacial
    // walls. Node vaults ride the expedition machine.
    let salvage_chance = match class {
        IceClass::PackIce => WRECK_SALVAGE_CHANCE_PER_SECOND,
        IceClass::GlacialWall => WALL_CACHE_CHANCE_PER_SECOND,
        _ => 0.0,
    };
    if salvage_chance > 0.0 && rng.next_f32() < salvage_chance * intensity * TICK_SECONDS {
        let blueprint = 1 + rng.next_range(BLUEPRINT_POOL);
        let payload = match class {
            IceClass::GlacialWall => SalvageEvent::WallCache { blueprint },
            _ => SalvageEvent::WreckSalvage { blueprint },
        };
        bus.emit(tick.0, EventPayload::Salvage(payload));
    }
}

/// World phase: the weather generator (spec 006 section 5). One
/// system at a time for the slice; onsets, targeted freezes, drone
/// scrambles, and paired ends all travel as typed events.
fn generate_weather(
    tick: Res<SimTick>,
    mut domain: ResMut<WorldDomain>,
    mut rng: ResMut<SimRng>,
    mut bus: ResMut<EventBus>,
) {
    let weather = &mut domain.weather;
    if weather.storm_ticks == 0 && weather.flare_ticks == 0 {
        if rng.next_f32() < STORM_CHANCE_PER_SECOND * TICK_SECONDS {
            weather.storm_ticks = STORM_BASE_TICKS + rng.next_range(STORM_BASE_TICKS);
            weather.storms_seen += 1;
            bus.emit(tick.0, EventPayload::Weather(WeatherEvent::StormOnset));
        } else if rng.next_f32() < FLARE_CHANCE_PER_SECOND * TICK_SECONDS {
            weather.flare_ticks = FLARE_BASE_TICKS + rng.next_range(FLARE_BASE_TICKS);
            weather.flares_seen += 1;
            bus.emit(tick.0, EventPayload::Weather(WeatherEvent::SolarFlareOnset));
        }
        return;
    }
    if weather.storm_ticks > 0 {
        weather.storm_ticks -= 1;
        if rng.next_f32() < FREEZE_CHANCE_PER_SECOND * TICK_SECONDS {
            let node = rng.next_range(HULL_NODES as u32);
            let payload = if rng.next_f32() < 0.5 {
                WeatherEvent::ValveFreeze { node }
            } else {
                WeatherEvent::SensorFreeze { node }
            };
            bus.emit(tick.0, EventPayload::Weather(payload));
        }
        if weather.storm_ticks == 0 {
            bus.emit(tick.0, EventPayload::Weather(WeatherEvent::StormEnd));
        }
        return;
    }
    weather.flare_ticks -= 1;
    if rng.next_f32() < SCRAMBLE_CHANCE_PER_SECOND * TICK_SECONDS {
        bus.emit(tick.0, EventPayload::Weather(WeatherEvent::DroneScramble));
    }
    if weather.flare_ticks == 0 {
        bus.emit(tick.0, EventPayload::Weather(WeatherEvent::SolarFlareEnd));
    }
}

/// World phase: Iceberg Node expeditions (spec 006 section 4). Sites
/// are sighted under way; anchoring at one deploys the rovers; the
/// crush clock and the rover hauls both run until the anchor is
/// released. Greed calibration is the player's call.
fn run_expedition(
    tick: Res<SimTick>,
    helm: Res<Helm>,
    kinetics: Res<ShipKinetics>,
    mut domain: ResMut<WorldDomain>,
    mut rng: ResMut<SimRng>,
    mut bus: ResMut<EventBus>,
) {
    let expedition = &mut domain.expedition;
    if !expedition.site_available && !expedition.anchored_at_site {
        if kinetics.speed > 0.0 && rng.next_f32() < SITE_CHANCE_PER_SECOND * TICK_SECONDS {
            expedition.site_available = true;
        }
        return;
    }
    if helm.anchor_ordered && expedition.site_available && !expedition.anchored_at_site {
        expedition.anchored_at_site = true;
        expedition.crush_pressure = 0.0;
        expedition.crush_countdown = CRUSH_EVENT_TICKS;
        expedition.haul_countdown = HAUL_TICKS;
        bus.emit(tick.0, EventPayload::Expedition(ExpeditionEvent::AnchorSet));
        return;
    }
    if !helm.anchor_ordered && expedition.anchored_at_site {
        expedition.anchored_at_site = false;
        expedition.site_available = false;
        expedition.crush_pressure = 0.0;
        bus.emit(
            tick.0,
            EventPayload::Expedition(ExpeditionEvent::RoverReturn),
        );
        return;
    }
    if !expedition.anchored_at_site {
        return;
    }
    expedition.crush_pressure += CRUSH_RATE_PER_SECOND * TICK_SECONDS;
    if rng.next_f32() < WARNING_CHANCE_PER_SECOND * TICK_SECONDS {
        let magnitude = 0.2 + 0.8 * rng.next_f32();
        bus.emit(
            tick.0,
            EventPayload::Expedition(ExpeditionEvent::IceShiftWarning { magnitude }),
        );
    }
    expedition.crush_countdown -= 1;
    if expedition.crush_countdown == 0 {
        expedition.crush_countdown = CRUSH_EVENT_TICKS;
        bus.emit(
            tick.0,
            EventPayload::Expedition(ExpeditionEvent::CrushProgress {
                pressure: expedition.crush_pressure,
            }),
        );
    }
    expedition.haul_countdown -= 1;
    if expedition.haul_countdown == 0 {
        expedition.haul_countdown = HAUL_TICKS;
        let tech = 1 + rng.next_range(3);
        let scrap = 2 + rng.next_range(5);
        bus.emit(
            tick.0,
            EventPayload::Ingestion {
                resource: ResourceKind::AncientTech,
                amount: tech,
            },
        );
        bus.emit(
            tick.0,
            EventPayload::Ingestion {
                resource: ResourceKind::FrozenScrap,
                amount: scrap,
            },
        );
        // A haul may come back carrying an Iceberg Node vault
        // blueprint (spec 016 section 3).
        if rng.next_f32() < NODE_VAULT_CHANCE {
            let blueprint = 1 + rng.next_range(BLUEPRINT_POOL);
            bus.emit(
                tick.0,
                EventPayload::Salvage(SalvageEvent::NodeVault { blueprint }),
            );
        }
    }
}

/// Interior phase: events become state changes, the only translation
/// (spec 002 section 3 rule 2). Weather freeze events target hull
/// nodes and resolve onto the node's column band of grid-cell
/// equipment (spec 015 section 7).
fn consume_events(
    mut bus: ResMut<EventBus>,
    mut hull: ResMut<HullGraph>,
    mut cargo: ResMut<CargoHold>,
    mut equipment: ResMut<Equipment>,
    mut fleet: ResMut<DroneFleet>,
    mut tech: ResMut<TechDomain>,
    interior: Res<InteriorGrid>,
) {
    let freeze_band = |bank: &mut [u32; GRID_W], node: u32| {
        let node = (node as usize % HULL_NODES) as u8;
        for (x, slot) in bank.iter_mut().enumerate() {
            if interior.hull_node_of_column[x] == node {
                *slot = THAW_TICKS;
            }
        }
    };
    while let Some(event) = bus.queue.pop() {
        match event.payload {
            EventPayload::Impact { node, magnitude } => {
                let slot = &mut hull.stress[node as usize % HULL_NODES];
                *slot = (*slot + magnitude).min(1.0);
            }
            EventPayload::Ingestion { resource, amount } => {
                cargo.amounts[CargoHold::index(resource)] += u64::from(amount);
            }
            EventPayload::Weather(WeatherEvent::ValveFreeze { node }) => {
                freeze_band(&mut equipment.valve_frozen, node);
            }
            EventPayload::Weather(WeatherEvent::SensorFreeze { node }) => {
                freeze_band(&mut equipment.sensor_frozen, node);
            }
            EventPayload::Weather(WeatherEvent::DroneScramble) => {
                fleet.scrambled_ticks = SCRAMBLE_TICKS;
            }
            // Onset and end pairs are presentation surface; the
            // targeted freezes and scrambles above carry the
            // mechanical consequences (spec 006 section 5).
            EventPayload::Weather(
                WeatherEvent::StormOnset
                | WeatherEvent::StormEnd
                | WeatherEvent::SolarFlareOnset
                | WeatherEvent::SolarFlareEnd,
            ) => {}
            EventPayload::Expedition(ExpeditionEvent::CrushProgress { pressure }) => {
                for stress in &mut hull.stress {
                    *stress = (*stress + pressure * CRUSH_STRESS_SCALE).min(1.0);
                }
            }
            // The warning is presentation surface; the crush events
            // that follow carry the stress. Anchor set and rover
            // return mark the expedition's edges (spec 006 section 4).
            EventPayload::Expedition(
                ExpeditionEvent::AnchorSet
                | ExpeditionEvent::IceShiftWarning { .. }
                | ExpeditionEvent::RoverReturn,
            ) => {}
            EventPayload::Salvage(salvage) => {
                let blueprint = match salvage {
                    SalvageEvent::WreckSalvage { blueprint }
                    | SalvageEvent::WallCache { blueprint }
                    | SalvageEvent::NodeVault { blueprint } => blueprint,
                };
                // Blueprints are unique flags, never stock: a
                // duplicate find converts to research and the set
                // never double-counts (spec 016 section 3).
                if !tech.blueprints.insert(blueprint) {
                    tech.research += tech::DUPLICATE_BLUEPRINT_RESEARCH;
                }
            }
        }
    }
}

/// Interior phase: frozen equipment counts down to thaw on its own
/// (spec 006 section 5); the ManualThaw command is the fast path.
fn thaw_equipment(mut equipment: ResMut<Equipment>) {
    let equipment = &mut *equipment;
    for ticks in equipment
        .valve_frozen
        .iter_mut()
        .chain(equipment.sensor_frozen.iter_mut())
    {
        *ticks = ticks.saturating_sub(1);
    }
}

/// Interior phase: the logistics spine (spec 015 section 4). Belts
/// and pipes move one whole unit per tick from the room under their
/// tail cell to the room under their head cell, in stable creation
/// order; a full destination back-pressures and nothing vanishes. An
/// edge touching a breached cell or a disabled room is severed, so
/// cascade reach follows spine topology. Data lines carry rule
/// signals; their authoring surface is a future spec (spec 015
/// section 8).
fn run_spine(hull: Res<HullGraph>, tech: Res<TechDomain>, mut interior: ResMut<InteriorGrid>) {
    let units_per_tick = tech.profile().belt_units_per_tick;
    for index in 0..interior.edges.len() {
        let edge = interior.edges[index].clone();
        if edge.kind == SpineKind::DataLine {
            continue;
        }
        if interior.edge_severed(&edge, &hull) {
            continue;
        }
        let (Some(src), Some(dst)) = (
            interior.room_index_at(edge.from),
            interior.room_index_at(edge.to),
        ) else {
            continue;
        };
        if src == dst {
            continue;
        }
        // The paradigm's throughput (spec 016 section 5), still
        // whole units, still back-pressured.
        for _ in 0..units_per_tick {
            let capacity = interior.rooms[dst].spec().buffer_capacity;
            if interior.rooms[src].output_buffer == 0
                || interior.rooms[dst].input_buffer >= capacity
            {
                break;
            }
            interior.rooms[src].output_buffer -= 1;
            interior.rooms[dst].input_buffer += 1;
        }
    }
}

/// Interior phase: tier-1 rule evaluation, first thing after the
/// event queue snapshot of the world phase. Rules run in stored list
/// order; later rules win conflicting writes (spec 010 section 5,
/// spec 005 section 3).
fn evaluate_rules(
    rules: Res<AutomationRules>,
    engine: Res<EngineCore>,
    hull: Res<HullGraph>,
    cargo: Res<CargoHold>,
    domain: Res<WorldDomain>,
    mut routing: ResMut<Routing>,
) {
    for rule in &rules.rules {
        let fired = match rule.condition {
            Condition::Always => true,
            Condition::FuelBufferBelow(level) => engine.fuel_buffer < level,
            Condition::CoreTempAbove(level) => engine.temperature > level,
            Condition::CoreTempBelow(level) => engine.temperature < level,
            Condition::StockBelow { resource, amount } => cargo.amount(resource) < amount,
            Condition::StressAbove { node, level } => {
                hull.stress[node as usize % HULL_NODES] > level
            }
            Condition::StormActive => domain.weather.storm_ticks > 0,
        };
        if !fired {
            continue;
        }
        match rule.action {
            RuleAction::SetFeedEnabled(enabled) => routing.feed_enabled = enabled,
            RuleAction::SetCoolantEnabled(enabled) => routing.coolant_enabled = enabled,
        }
    }
}

/// Interior phase: research accrues only when a working Refinery
/// processes AncientTech units from cargo (spec 016 section 2): per
/// processed unit, metered in whole units, deterministic. No passive
/// trickle and no wall-clock component; a starved or stalled
/// refinery accrues nothing.
fn run_refineries(
    core: Res<EngineCore>,
    hull: Res<HullGraph>,
    field: Res<ThermalField>,
    interior: Res<InteriorGrid>,
    mut cargo: ResMut<CargoHold>,
    mut tech: ResMut<TechDomain>,
) {
    let working = interior
        .rooms
        .iter()
        .filter(|room| {
            room.kind == RoomKind::Refinery && interior.room_working(room, &core, &field, &hull)
        })
        .count();
    if working == 0 {
        return;
    }
    let slot = CargoHold::index(ResourceKind::AncientTech);
    tech.refine_meter += tech::REFINE_UNITS_PER_SECOND * working as f32 * TICK_SECONDS;
    while tech.refine_meter >= 1.0 {
        if cargo.amounts[slot] == 0 {
            // Nothing to process: the meter does not bank against
            // future stock (accrual is per unit actually processed).
            tech.refine_meter = 0.0;
            break;
        }
        tech.refine_meter -= 1.0;
        cargo.amounts[slot] -= 1;
        tech.research += tech::RESEARCH_PER_TECH;
    }
}

/// Interior phase: the feed line stages whole fuel units at the core
/// while logistics is online, the belt switch is set, and the buffer
/// has room (spec 004 section 3: a constant, uninterrupted supply is
/// the requirement).
fn feed_engine(routing: Res<Routing>, mut core: ResMut<EngineCore>, mut cargo: ResMut<CargoHold>) {
    if !routing.feed_enabled || !core.system_online(ShipSystem::Logistics) {
        return;
    }
    let biomass = CargoHold::index(ResourceKind::Biomass);
    let mut moved = 0;
    while moved < FEED_UNITS_PER_TICK
        && cargo.amounts[biomass] > 0
        && core.fuel_buffer <= FUEL_BUFFER_CAP - 1.0
    {
        cargo.amounts[biomass] -= 1;
        core.fuel_buffer += 1.0;
        moved += 1;
    }
}

/// Interior phase: the core burns against the torque-driven demand the
/// world phase computed this tick, heating itself and leaking heat
/// into its compartment (spec 004 section 3).
fn burn_fuel(
    kinetics: Res<ShipKinetics>,
    field: Res<ThermalField>,
    interior: Res<InteriorGrid>,
    mut core: ResMut<EngineCore>,
) {
    let demand = kinetics.fuel_burn * TICK_SECONDS;
    let burned = demand.min(core.fuel_buffer);
    core.fuel_buffer -= burned;
    let heat = burned * HEAT_PER_FUEL;
    let core_cell = field.temps[interior.core_cell_index()];
    let leak = CORE_LEAK_PER_SECOND * (core.temperature - core_cell) * TICK_SECONDS;
    core.temperature += heat - leak;
}

/// Interior phase: above the coolant threshold the loop melts intake
/// ice to pull core heat down; spent stock leaves the hold in whole
/// units via the coolant meter (spec 004 section 5).
fn route_coolant(
    routing: Res<Routing>,
    mut core: ResMut<EngineCore>,
    mut cargo: ResMut<CargoHold>,
) {
    let ice = CargoHold::index(ResourceKind::Ice);
    if !routing.coolant_enabled
        || core.temperature <= COOLANT_THRESHOLD_C
        || cargo.amounts[ice] == 0
    {
        return;
    }
    let melted = COOLANT_UNITS_PER_SECOND * TICK_SECONDS;
    core.coolant_meter += melted;
    core.temperature -= melted * COOLING_PER_ICE;
    if core.coolant_meter >= 1.0 {
        core.coolant_meter -= 1.0;
        cargo.amounts[ice] -= 1;
    }
}

/// Interior phase: thermal relaxation over the grid cells, double
/// buffered so iteration order cannot leak into state (spec 010
/// section 5; the field migrated from the bootstrap compartment ring
/// onto the grid, spec 015 section 3). Heat sources are working
/// rooms' emissions and the core's coupling into its own cells;
/// sinks are Heat Sinks (negative emission), and every cell leaks
/// toward the ambient cold (spec 004 section 5).
fn update_thermal_field(
    core: Res<EngineCore>,
    hull: Res<HullGraph>,
    interior: Res<InteriorGrid>,
    tech: Res<TechDomain>,
    mut field: ResMut<ThermalField>,
) {
    let old = field.temps.clone();
    let mut emission = vec![0.0f32; GRID_W * GRID_H];
    let mut core_cells = [false; GRID_W * GRID_H];
    for room in &interior.rooms {
        if room.kind == RoomKind::EngineCore {
            for cell in room.cells() {
                core_cells[cell.index()] = true;
            }
            continue;
        }
        // Stall and breach gate the emission: a stopped room neither
        // heats nor cools (spec 015 section 3). The stall check reads
        // the previous tick's temperatures, like the relaxation.
        if !interior.room_working(room, &core, &field, &hull) {
            continue;
        }
        let (w, h) = room.spec().footprint;
        // The paradigm's heat economy scales every emission and
        // absorption (spec 016 section 5).
        let per_cell =
            room.spec().heat_per_second * tech.profile().heat_emission_factor / f32::from(w * h);
        for cell in room.cells() {
            emission[cell.index()] += per_cell;
        }
    }
    for (i, temp) in field.temps.iter_mut().enumerate() {
        let x = i % GRID_W;
        let y = i / GRID_W;
        let mut flux = 0.0;
        if x > 0 {
            flux += old[i - 1] - old[i];
        }
        if x + 1 < GRID_W {
            flux += old[i + 1] - old[i];
        }
        if y > 0 {
            flux += old[i - GRID_W] - old[i];
        }
        if y + 1 < GRID_H {
            flux += old[i + GRID_W] - old[i];
        }
        let mut delta =
            DIFFUSION_PER_SECOND * flux + HULL_LEAK_PER_SECOND * (AMBIENT_C - old[i]) + emission[i];
        if core_cells[i] {
            delta += CORE_COUPLING_PER_SECOND * (core.temperature - old[i]);
        }
        *temp = old[i] + delta * TICK_SECONDS;
    }
}

/// Interior phase: tick-counted ladder movement with hysteresis
/// between the stall and restart thresholds (spec 010 section 5).
fn advance_shutdown_ladder(mut core: ResMut<EngineCore>) {
    let at_bottom = usize::from(core.shutdown_stage) >= SHUTDOWN_LADDER.len();
    if core.temperature < STALL_C && !at_bottom {
        core.ticks_at_stage += 1;
        if core.ticks_at_stage >= LADDER_TICKS {
            core.shutdown_stage += 1;
            core.ticks_at_stage = 0;
        }
    } else if core.temperature > RESTART_C && core.shutdown_stage > 0 {
        core.ticks_at_stage += 1;
        if core.ticks_at_stage >= LADDER_TICKS {
            core.shutdown_stage -= 1;
            core.ticks_at_stage = 0;
        }
    } else {
        core.ticks_at_stage = 0;
    }
}

/// Interior phase: the standing repair line is drone work (spec 005
/// sections 2 and 4). Each repair drone, in spawn order, serves the
/// most stressed hull node its cell-column zone maps onto (ties to
/// the lowest index; spec 015 section 7 rebased zones onto the
/// grid), drawing strut feedstock from the hold and accruing wear.
/// Drones idle while the ladder has the drone bays down.
fn run_drones(
    core: Res<EngineCore>,
    interior: Res<InteriorGrid>,
    tech: Res<TechDomain>,
    mut fleet: ResMut<DroneFleet>,
    mut hull: ResMut<HullGraph>,
    mut cargo: ResMut<CargoHold>,
) {
    if fleet.scrambled_ticks > 0 {
        // Smart routing is down; belts elsewhere keep running (spec
        // 006 section 5).
        fleet.scrambled_ticks -= 1;
        return;
    }
    if !core.system_online(ShipSystem::DroneBays) {
        return;
    }
    let scrap = CargoHold::index(ResourceKind::FrozenScrap);
    for drone in &mut fleet.drones {
        if drone.kind != DroneKind::Repair || cargo.amounts[scrap] == 0 {
            continue;
        }
        let mut target: Option<usize> = None;
        for column in drone.zone.from..=drone.zone.to {
            let column = column as usize;
            if column >= GRID_W {
                continue;
            }
            let node = interior.hull_node_of_column[column] as usize;
            if hull.stress[node] <= 0.0 {
                continue;
            }
            let better = match target {
                None => true,
                Some(current) => hull.stress[node] > hull.stress[current],
            };
            if better {
                target = Some(node);
            }
        }
        let Some(node) = target else {
            continue;
        };
        let uptime = drone.uptime();
        let repaired =
            REPAIR_RATE_PER_SECOND * uptime * tech.profile().drone_throughput_factor * TICK_SECONDS;
        hull.stress[node] = (hull.stress[node] - repaired).max(0.0);
        drone.wear = (drone.wear + WEAR_PER_SECOND * TICK_SECONDS).min(1.0);
        drone.strut_meter += STRUT_UNITS_PER_SECOND * uptime * TICK_SECONDS;
        if drone.strut_meter >= 1.0 {
            drone.strut_meter -= 1.0;
            cargo.amounts[scrap] -= 1;
        }
    }
}

/// Interior phase: the bay pulls worn drones in, spending scrap in
/// whole units to reset wear (spec 005 section 2: maintenance cost is
/// the price of uptime).
fn maintain_drones(
    core: Res<EngineCore>,
    mut fleet: ResMut<DroneFleet>,
    mut cargo: ResMut<CargoHold>,
) {
    if !core.system_online(ShipSystem::DroneBays) {
        return;
    }
    let scrap = CargoHold::index(ResourceKind::FrozenScrap);
    for drone in &mut fleet.drones {
        if drone.wear >= MAINTENANCE_WEAR && cargo.amounts[scrap] >= MAINTENANCE_SCRAP_UNITS {
            cargo.amounts[scrap] -= MAINTENANCE_SCRAP_UNITS;
            drone.wear = 0.0;
        }
    }
}

/// Readback phase: interior outcomes become capability the next world
/// phase reads (spec 002 section 3 rule 3).
fn read_back_capability(
    hull: Res<HullGraph>,
    core: Res<EngineCore>,
    equipment: Res<Equipment>,
    mut capability: ResMut<Capability>,
) {
    let total: f32 = hull.stress.iter().sum();
    let average = total / HULL_NODES as f32;
    let hull_factor = (1.0 - average).clamp(0.0, 1.0);
    capability.available_thrust = if core.system_online(ShipSystem::Propulsion) {
        hull_factor
    } else {
        0.0
    };
    capability.sensor_coverage = if core.system_online(ShipSystem::Sensors) {
        Equipment::working_fraction(&equipment.sensor_frozen)
    } else {
        0.0
    };
    capability.intake_capacity = Equipment::working_fraction(&equipment.valve_frozen);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scripted_run(seed: u64, ticks: u64) -> SimWorld {
        let mut sim = SimWorld::new(seed);
        sim.push_command(Command::SetThrottle { throttle: 1.0 });
        sim.push_command(Command::SetHeading { heading_rad: 0.3 });
        for t in 0..ticks {
            if t == 50 {
                sim.push_command(Command::SetHeading { heading_rad: -0.8 });
            }
            sim.tick();
        }
        sim
    }

    /// Spec 010 section 8 test 1: same seed, same command script, twice:
    /// byte-identical end states.
    #[test]
    fn replay_determinism() {
        let a = scripted_run(42, 200);
        let b = scripted_run(42, 200);
        assert_eq!(a.save_bytes(), b.save_bytes());
    }

    #[test]
    fn different_seeds_diverge() {
        let a = scripted_run(1, 200);
        let b = scripted_run(2, 200);
        assert_ne!(a.save_bytes(), b.save_bytes());
    }

    /// Spec 010 section 8 test 2: save at tick k, load, continue:
    /// byte-identical to the uninterrupted run.
    #[test]
    fn save_load_equivalence() {
        let mut uninterrupted = SimWorld::new(7);
        uninterrupted.push_command(Command::SetThrottle { throttle: 0.9 });
        for _ in 0..100 {
            uninterrupted.tick();
        }
        let checkpoint = uninterrupted.save_bytes();
        for _ in 0..100 {
            uninterrupted.tick();
        }

        let mut resumed = SimWorld::from_save_bytes(&checkpoint).expect("load");
        for _ in 0..100 {
            resumed.tick();
        }
        assert_eq!(uninterrupted.save_bytes(), resumed.save_bytes());
    }

    /// Spec 010 section 8 test 3: the phase order of section 3 holds and
    /// the ambiguity-checked schedules built without panicking.
    #[test]
    fn schedule_integrity() {
        let mut sim = SimWorld::new(0);
        sim.tick();
        assert_eq!(
            sim.last_trace().phases,
            vec![
                Phase::Commands,
                Phase::World,
                Phase::Interior,
                Phase::Readback
            ]
        );
    }

    /// Spec 010 section 8 test 4: the interior phase never touches the
    /// RNG, asserted on every tick of a busy run.
    #[test]
    fn interior_purity() {
        let mut sim = SimWorld::new(1234);
        sim.push_command(Command::SetThrottle { throttle: 1.0 });
        for _ in 0..200 {
            sim.tick();
            let trace = sim.last_trace();
            assert_eq!(trace.rng_after_world, trace.rng_after_interior);
        }
    }

    /// The vertical slice actually flows: motion generates events, the
    /// interior turns them into hull stress and cargo, torque demand
    /// drives fuel burn, and the fed core stays online.
    #[test]
    fn vertical_slice_flows() {
        let sim = scripted_run(42, 400);
        let world = sim.world();
        let kinetics = world.resource::<ShipKinetics>();
        assert!(kinetics.position[0] != 0.0 || kinetics.position[1] != 0.0);
        assert!(kinetics.torque_demand > 0.0, "no torque demand under way");
        assert!(
            kinetics.fuel_burn > IDLE_BURN_PER_SECOND,
            "torque did not raise fuel demand"
        );
        let cargo = world.resource::<CargoHold>();
        assert!(
            cargo.amounts.iter().sum::<u64>() > 0,
            "the hold is empty after 400 ticks"
        );
        let bus = world.resource::<EventBus>();
        assert!(bus.next_seq > 0, "no events emitted in 400 ticks");
        let core = world.resource::<EngineCore>();
        assert_eq!(core.shutdown_stage, 0, "the fed core walked the ladder");
    }

    /// A fed core under way holds its operating band: fuel is drawn,
    /// temperature stays above stall, the ladder never moves (spec 004
    /// section 3).
    #[test]
    fn engine_burns_fuel_and_holds_temperature() {
        let mut sim = SimWorld::new(9);
        sim.push_command(Command::SetThrottle { throttle: 1.0 });
        for _ in 0..400 {
            sim.tick();
        }
        let core = sim.world.resource::<EngineCore>();
        assert_eq!(core.shutdown_stage, 0);
        assert!(core.fuel_buffer > 0.0, "the feed line never staged fuel");
        assert!(core.temperature > STALL_C, "the fed core stalled");
        assert!(core.temperature < 400.0, "the core ran away hot");
    }

    /// Starved of fuel, the core chills and walks the shutdown ladder
    /// in order, farthest-from-core first, propulsion last; readback
    /// zeroes thrust and sensor coverage (spec 004 section 3).
    #[test]
    fn starvation_walks_the_shutdown_ladder() {
        let mut sim = SimWorld::new(11);
        sim.world.resource_mut::<CargoHold>().amounts = [0; ResourceKind::ALL.len()];
        sim.world.resource_mut::<EngineCore>().fuel_buffer = 0.0;

        let mut stages = Vec::new();
        for _ in 0..1600 {
            sim.tick();
            let stage = sim.world.resource::<EngineCore>().shutdown_stage;
            if stages.last() != Some(&stage) {
                stages.push(stage);
            }
        }
        assert_eq!(stages, vec![0, 1, 2, 3, 4], "ladder skipped or stalled");
        let capability = sim.world.resource::<Capability>();
        assert_eq!(capability.available_thrust, 0.0);
        assert_eq!(capability.sensor_coverage, 0.0);
    }

    /// Refueled while only the sensors are down, the core reheats and
    /// the ladder retraces in reverse (spec 010 section 5).
    #[test]
    fn refueling_retraces_the_ladder() {
        let mut sim = SimWorld::new(13);
        sim.world.resource_mut::<CargoHold>().amounts = [0; ResourceKind::ALL.len()];
        sim.world.resource_mut::<EngineCore>().fuel_buffer = 0.0;

        let mut safety = 0;
        while sim.world.resource::<EngineCore>().shutdown_stage < 1 {
            sim.tick();
            safety += 1;
            assert!(safety < 2000, "ladder never reached stage 1");
        }

        let biomass = CargoHold::index(ResourceKind::Biomass);
        sim.world.resource_mut::<CargoHold>().amounts[biomass] = 200;
        sim.push_command(Command::SetThrottle { throttle: 1.0 });
        let mut safety = 0;
        while sim.world.resource::<EngineCore>().shutdown_stage > 0 {
            sim.tick();
            safety += 1;
            assert!(safety < 4000, "ladder never retraced");
        }
        assert!(sim.world.resource::<EngineCore>().temperature > RESTART_C);
        sim.tick();
        assert!(sim.world.resource::<Capability>().sensor_coverage > 0.0);
    }

    /// Cold is the default: with the core dead, every compartment
    /// trends toward ambient exterior temperature and never overshoots
    /// it (spec 004 section 5).
    #[test]
    fn thermal_field_trends_toward_ambient() {
        let mut sim = SimWorld::new(17);
        sim.world.resource_mut::<CargoHold>().amounts = [0; ResourceKind::ALL.len()];
        {
            let mut core = sim.world.resource_mut::<EngineCore>();
            core.fuel_buffer = 0.0;
            core.temperature = AMBIENT_C;
        }
        let before = sim.world.resource::<ThermalField>().temps.clone();
        for _ in 0..600 {
            sim.tick();
        }
        let after = sim.world.resource::<ThermalField>().temps.clone();
        for (b, a) in before.iter().zip(after.iter()) {
            assert!(a < b, "a compartment failed to cool");
            assert!(*a >= AMBIENT_C, "a compartment overshot ambient");
        }
    }

    /// Repair drones serve the most stressed node inside their own
    /// zone and consume strut feedstock; nodes no drone patrols stay
    /// stressed (spec 005 section 2).
    #[test]
    fn drones_repair_only_their_zones() {
        let mut sim = SimWorld::new(23);
        // Move drone 1 onto columns 4..=7 (nodes 2..=3): both drones
        // now patrol the port half, leaving node 6 (columns 12..=13)
        // orphaned.
        sim.push_command(Command::SetDroneZone {
            drone: 1,
            zone: DroneZone { from: 4, to: 7 },
        });
        sim.world.resource_mut::<HullGraph>().stress[1] = 0.8;
        sim.world.resource_mut::<HullGraph>().stress[6] = 0.8;
        let scrap = CargoHold::index(ResourceKind::FrozenScrap);
        let scrap_before = sim.world.resource::<CargoHold>().amounts[scrap];
        for _ in 0..300 {
            sim.tick();
        }
        let hull = sim.world.resource::<HullGraph>();
        assert!(hull.stress[1] < 0.8, "patrolled node was not repaired");
        assert_eq!(hull.stress[6], 0.8, "an orphaned node was repaired");
        let scrap_after = sim.world.resource::<CargoHold>().amounts[scrap];
        assert!(scrap_after < scrap_before, "repair drew no strut feedstock");
    }

    /// Worn drones get pulled in by the bay: wear resets and scrap is
    /// spent in whole units (spec 005 section 2).
    #[test]
    fn maintenance_resets_wear_for_scrap() {
        let mut sim = SimWorld::new(29);
        sim.world.resource_mut::<DroneFleet>().drones[0].wear = MAINTENANCE_WEAR + 0.1;
        let scrap = CargoHold::index(ResourceKind::FrozenScrap);
        let scrap_before = sim.world.resource::<CargoHold>().amounts[scrap];
        sim.tick();
        let fleet = sim.world.resource::<DroneFleet>();
        assert_eq!(fleet.drones[0].wear, 0.0, "maintenance never ran");
        let scrap_after = sim.world.resource::<CargoHold>().amounts[scrap];
        assert_eq!(scrap_after, scrap_before - MAINTENANCE_SCRAP_UNITS);
    }

    /// Rules evaluate in stored order and later rules win conflicting
    /// writes; removing the later rule hands the switch back (spec 010
    /// section 5, spec 005 section 3).
    #[test]
    fn rules_evaluate_in_stable_order() {
        let mut sim = SimWorld::new(31);
        sim.push_command(Command::AddRule {
            condition: Condition::Always,
            action: RuleAction::SetCoolantEnabled(false),
        });
        sim.push_command(Command::AddRule {
            condition: Condition::Always,
            action: RuleAction::SetCoolantEnabled(true),
        });
        sim.tick();
        assert!(
            sim.world.resource::<Routing>().coolant_enabled,
            "the later rule should win the conflicting write"
        );
        // Rule ids are assigned in push order: the later rule is id 1.
        sim.push_command(Command::RemoveRule { id: 1 });
        sim.tick();
        assert!(
            !sim.world.resource::<Routing>().coolant_enabled,
            "with the later rule removed, the earlier one should hold"
        );
    }

    /// A threshold gate actually routes: cutting the feed line drains
    /// the buffer and leaves the hold untouched (spec 005 section 3).
    #[test]
    fn a_rule_gates_the_feed_line() {
        let mut sim = SimWorld::new(37);
        sim.push_command(Command::AddRule {
            condition: Condition::Always,
            action: RuleAction::SetFeedEnabled(false),
        });
        let biomass = CargoHold::index(ResourceKind::Biomass);
        sim.tick();
        let stock_after_cut = sim.world.resource::<CargoHold>().amounts[biomass];
        let buffer_after_cut = sim.world.resource::<EngineCore>().fuel_buffer;
        for _ in 0..100 {
            sim.tick();
        }
        assert_eq!(
            sim.world.resource::<CargoHold>().amounts[biomass],
            stock_after_cut,
            "the cut feed line still drew from the hold"
        );
        assert!(
            sim.world.resource::<EngineCore>().fuel_buffer < buffer_after_cut,
            "the core stopped burning from the buffer"
        );
    }

    /// Freeze events resolve into equipment state, degrade the intake
    /// and sensor readback, and thaw on the tick countdown (spec 006
    /// section 5).
    #[test]
    fn freezes_degrade_readback_and_thaw() {
        let mut sim = SimWorld::new(41);
        let tick = sim.world.resource::<SimTick>().0;
        sim.world.resource_mut::<EventBus>().emit(
            tick,
            EventPayload::Weather(WeatherEvent::ValveFreeze { node: 2 }),
        );
        sim.world.resource_mut::<EventBus>().emit(
            tick,
            EventPayload::Weather(WeatherEvent::SensorFreeze { node: 3 }),
        );
        sim.tick();
        let equipment = sim.world.resource::<Equipment>();
        // Node-targeted freezes resolve onto the node's column band
        // (spec 015 section 7): node 2 is columns 4..=5, node 3 is
        // columns 6..=7.
        assert_eq!(equipment.valve_frozen[4], THAW_TICKS - 1);
        assert_eq!(equipment.valve_frozen[5], THAW_TICKS - 1);
        assert_eq!(equipment.sensor_frozen[6], THAW_TICKS - 1);
        let capability = sim.world.resource::<Capability>();
        assert!(capability.intake_capacity < 1.0);
        assert!(capability.sensor_coverage < 1.0);
        for _ in 0..THAW_TICKS {
            sim.tick();
        }
        let equipment = sim.world.resource::<Equipment>();
        assert_eq!(equipment.valve_frozen[4], 0, "the valve never thawed");
        assert_eq!(equipment.sensor_frozen[6], 0, "the sensor never thawed");
    }

    /// The emergency manual override thaws a node's equipment at once
    /// (spec 006 section 5).
    #[test]
    fn manual_thaw_overrides_the_countdown() {
        let mut sim = SimWorld::new(61);
        let tick = sim.world.resource::<SimTick>().0;
        sim.world.resource_mut::<EventBus>().emit(
            tick,
            EventPayload::Weather(WeatherEvent::ValveFreeze { node: 5 }),
        );
        sim.tick();
        // Node 5's band is columns 10..=11.
        assert!(sim.world.resource::<Equipment>().valve_frozen[10] > 0);
        sim.push_command(Command::ManualThaw { node: 5 });
        sim.tick();
        assert_eq!(sim.world.resource::<Equipment>().valve_frozen[10], 0);
        assert_eq!(sim.world.resource::<Equipment>().valve_frozen[11], 0);
    }

    /// A drone scramble suppresses fleet logic for its window while
    /// the feed belt keeps running; repair resumes afterward (spec 006
    /// section 5).
    #[test]
    fn scramble_stops_drones_but_not_belts() {
        let mut sim = SimWorld::new(43);
        sim.world.resource_mut::<HullGraph>().stress[1] = 0.9;
        let tick = sim.world.resource::<SimTick>().0;
        sim.world
            .resource_mut::<EventBus>()
            .emit(tick, EventPayload::Weather(WeatherEvent::DroneScramble));
        let buffer_before = sim.world.resource::<EngineCore>().fuel_buffer;
        for _ in 0..50 {
            sim.tick();
        }
        assert_eq!(
            sim.world.resource::<HullGraph>().stress[1],
            0.9,
            "a scrambled drone did repair work"
        );
        assert!(
            sim.world.resource::<EngineCore>().fuel_buffer > buffer_before,
            "the feed belt stopped during the scramble"
        );
        for _ in 0..SCRAMBLE_TICKS {
            sim.tick();
        }
        assert!(
            sim.world.resource::<HullGraph>().stress[1] < 0.9,
            "repair never resumed after the scramble"
        );
    }

    /// Crush pressure resolves as ambient stress across every hull
    /// node (spec 006 section 4).
    #[test]
    fn crush_progress_stresses_every_node() {
        let mut sim = SimWorld::new(47);
        let tick = sim.world.resource::<SimTick>().0;
        sim.world.resource_mut::<EventBus>().emit(
            tick,
            EventPayload::Expedition(ExpeditionEvent::CrushProgress { pressure: 1.0 }),
        );
        sim.tick();
        for stress in &sim.world.resource::<HullGraph>().stress {
            assert!(*stress > 0.0, "a node escaped the ambient crush");
        }
    }

    /// The expedition lifecycle: sight a site under way, anchor,
    /// accrue crush pressure and rover hauls, release, rovers return
    /// and the site is spent (spec 006 section 4).
    #[test]
    fn expedition_lifecycle_flows() {
        let mut sim = SimWorld::new(53);
        sim.push_command(Command::SetThrottle { throttle: 1.0 });
        let mut safety = 0;
        while !sim
            .world
            .resource::<WorldDomain>()
            .expedition
            .site_available
        {
            sim.tick();
            safety += 1;
            assert!(safety < 6000, "no site sighted under way");
        }
        sim.push_command(Command::SetAnchor { anchored: true });
        sim.tick();
        sim.tick();
        assert!(
            sim.world
                .resource::<WorldDomain>()
                .expedition
                .anchored_at_site
        );
        assert_eq!(
            sim.world.resource::<ShipKinetics>().speed,
            0.0,
            "the anchored ship kept moving"
        );
        let tech_before = sim
            .world
            .resource::<CargoHold>()
            .amount(ResourceKind::AncientTech);
        for _ in 0..200 {
            sim.tick();
        }
        assert!(
            sim.world
                .resource::<WorldDomain>()
                .expedition
                .crush_pressure
                > 0.0
        );
        assert!(
            sim.world
                .resource::<CargoHold>()
                .amount(ResourceKind::AncientTech)
                > tech_before,
            "no rover hauls landed"
        );
        sim.push_command(Command::SetAnchor { anchored: false });
        sim.tick();
        let domain = sim.world.resource::<WorldDomain>();
        assert!(!domain.expedition.anchored_at_site);
        assert!(!domain.expedition.site_available, "the site was not spent");
        assert_eq!(domain.expedition.crush_pressure, 0.0);
    }

    /// Weather happens on its own: a long moving run sees at least one
    /// super-storm (spec 006 section 5).
    #[test]
    fn storms_eventually_happen() {
        let mut sim = SimWorld::new(59);
        sim.push_command(Command::SetThrottle { throttle: 1.0 });
        for _ in 0..6000 {
            sim.tick();
        }
        let weather = &sim.world.resource::<WorldDomain>().weather;
        assert!(weather.storms_seen > 0, "no storm in 6000 ticks");
    }

    /// Place a working Refinery next to the warm core; the refinery
    /// costs AncientTech, so the hold is topped up first.
    fn sim_with_refinery(seed: u64) -> SimWorld {
        let mut sim = SimWorld::new(seed);
        let slot = CargoHold::index(ResourceKind::AncientTech);
        sim.world.resource_mut::<CargoHold>().amounts[slot] = 5;
        sim.push_command(Command::PlaceRoom {
            kind: RoomKind::Refinery,
            origin: CellAddr {
                deck: 0,
                x: 5,
                y: 3,
            },
        });
        sim.tick();
        assert!(
            sim.world().resource::<BuildLog>().0.is_empty(),
            "the refinery placement was rejected"
        );
        sim
    }

    /// Spec 016 section 6 test 1: no passive accrual. A stationary
    /// run accrues zero research over any tick count, with no
    /// refinery and with a refinery that has nothing to process.
    #[test]
    fn research_never_accrues_passively() {
        let mut idle = SimWorld::new(211);
        for _ in 0..400 {
            idle.tick();
        }
        assert_eq!(idle.world().resource::<TechDomain>().research, 0);

        let mut starved = sim_with_refinery(211);
        let slot = CargoHold::index(ResourceKind::AncientTech);
        starved.world.resource_mut::<CargoHold>().amounts[slot] = 0;
        for _ in 0..400 {
            starved.tick();
        }
        assert_eq!(
            starved.world().resource::<TechDomain>().research,
            0,
            "a starved refinery accrued research"
        );
    }

    /// Spec 016 section 6 test 2: processing N units yields the same
    /// research on every replay, and exactly per unit processed.
    #[test]
    fn refinery_accrual_is_deterministic_and_per_unit() {
        let run = || {
            let mut sim = sim_with_refinery(213);
            let slot = CargoHold::index(ResourceKind::AncientTech);
            sim.world.resource_mut::<CargoHold>().amounts[slot] = 10;
            for _ in 0..600 {
                sim.tick();
            }
            (
                sim.world().resource::<TechDomain>().research,
                sim.world().resource::<CargoHold>().amounts[slot],
            )
        };
        let (research, leftover) = run();
        assert_eq!((research, leftover), run(), "replay diverged");
        assert_eq!(leftover, 0, "the refinery never chewed the stock");
        assert_eq!(
            research,
            10 * RESEARCH_PER_TECH,
            "accrual is not per processed unit"
        );
    }

    /// Spec 016 section 6 test 3: a gated room, prow, or rule order
    /// is rejected below its tier with no partial state, and
    /// accepted at it after a valid advancement.
    #[test]
    fn tier_gates_reject_below_and_accept_at_tier() {
        let mut sim = SimWorld::new(217);
        let gated_orders = |sim: &mut SimWorld| {
            sim.push_command(Command::PlaceRoom {
                kind: RoomKind::Fabricator,
                origin: CellAddr {
                    deck: 0,
                    x: 1,
                    y: 1,
                },
            });
            sim.push_command(Command::AddRule {
                condition: Condition::StormActive,
                action: RuleAction::SetFeedEnabled(false),
            });
            sim.push_command(Command::MountProwTrack {
                track: ProwTrack::HeatedRam,
            });
        };
        gated_orders(&mut sim);
        sim.tick();
        let reasons: Vec<RejectReason> = sim
            .world()
            .resource::<BuildLog>()
            .0
            .iter()
            .map(|r| r.reason)
            .collect();
        assert_eq!(
            reasons,
            vec![
                RejectReason::TierGated,
                RejectReason::TierGated,
                RejectReason::TierGated
            ]
        );
        assert_eq!(sim.world().resource::<InteriorGrid>().rooms.len(), 1);
        assert!(sim.world().resource::<AutomationRules>().rules.is_empty());
        assert_eq!(
            sim.world().resource::<ShipKinetics>().prow_track,
            ProwTrack::Ram
        );

        // Advancement itself validates: blueprints first, then the
        // research spend (spec 016 section 3).
        sim.push_command(Command::AdvanceTier);
        sim.tick();
        assert_eq!(
            sim.world().resource::<BuildLog>().0[0].reason,
            RejectReason::IncompleteBlueprints
        );
        for blueprint in blueprints_required(2) {
            sim.world
                .resource_mut::<TechDomain>()
                .blueprints
                .insert(*blueprint);
        }
        sim.push_command(Command::AdvanceTier);
        sim.tick();
        assert_eq!(
            sim.world().resource::<BuildLog>().0[0].reason,
            RejectReason::Unaffordable
        );
        assert_eq!(sim.world().resource::<TechDomain>().tier, 1);

        sim.world.resource_mut::<TechDomain>().research = research_cost(2);
        sim.push_command(Command::AdvanceTier);
        sim.tick();
        let tech = sim.world().resource::<TechDomain>();
        assert_eq!(tech.tier, 2);
        assert_eq!(tech.research, 0, "the spend was not drawn");

        // The same three orders now pass whole.
        gated_orders(&mut sim);
        sim.tick();
        assert!(sim.world().resource::<BuildLog>().0.is_empty());
        assert_eq!(sim.world().resource::<InteriorGrid>().rooms.len(), 2);
        assert_eq!(sim.world().resource::<AutomationRules>().rules.len(), 1);
        assert_eq!(
            sim.world().resource::<ShipKinetics>().prow_track,
            ProwTrack::HeatedRam
        );
    }

    /// Spec 016 section 6 test 4: a second identical blueprint
    /// converts to research; the set never double-counts.
    #[test]
    fn duplicate_blueprints_convert_to_research() {
        let mut sim = SimWorld::new(219);
        let emit = |sim: &mut SimWorld| {
            let tick = sim.world().resource::<SimTick>().0;
            sim.world.resource_mut::<EventBus>().emit(
                tick,
                EventPayload::Salvage(SalvageEvent::WreckSalvage { blueprint: 3 }),
            );
        };
        emit(&mut sim);
        sim.tick();
        {
            let tech = sim.world().resource::<TechDomain>();
            assert!(tech.blueprints.contains(&3));
            assert_eq!(tech.research, 0);
        }
        // The duplicate arrives from a different family member and
        // still converts (the id is what is unique).
        let tick = sim.world().resource::<SimTick>().0;
        sim.world.resource_mut::<EventBus>().emit(
            tick,
            EventPayload::Salvage(SalvageEvent::NodeVault { blueprint: 3 }),
        );
        sim.tick();
        let tech = sim.world().resource::<TechDomain>();
        assert_eq!(tech.blueprints.len(), 1, "the set double-counted");
        assert_eq!(tech.research, DUPLICATE_BLUEPRINT_RESEARCH);
    }

    /// Spec 016 section 6 test 5: the tick of tier advancement
    /// applies the whole new profile: fuel and belt throughput flip
    /// together, and no tick observes a mixed profile.
    #[test]
    fn tier_transition_applies_the_whole_profile_atomically() {
        let build = || {
            let mut sim = SimWorld::new(223);
            sim.push_command(Command::SetThrottle { throttle: 1.0 });
            for (x, kind) in [(1u8, RoomKind::Storage), (3, RoomKind::Storage)] {
                sim.push_command(Command::PlaceRoom {
                    kind,
                    origin: CellAddr { deck: 0, x, y: 1 },
                });
            }
            sim.push_command(Command::LayEdge {
                kind: SpineKind::Belt,
                from: CellAddr {
                    deck: 0,
                    x: 2,
                    y: 1,
                },
                to: CellAddr {
                    deck: 0,
                    x: 3,
                    y: 1,
                },
            });
            sim.tick();
            sim.world.resource_mut::<InteriorGrid>().rooms[1].output_buffer = 8;
            for _ in 0..3 {
                sim.tick();
            }
            sim
        };
        let mut held = build();
        let mut advancing = build();
        {
            let mut tech = advancing.world.resource_mut::<TechDomain>();
            for blueprint in blueprints_required(2) {
                tech.blueprints.insert(*blueprint);
            }
            tech.research = research_cost(2);
        }
        advancing.push_command(Command::AdvanceTier);
        held.tick();
        advancing.tick();

        // Same tick, both axes at once: the burn line re-priced...
        let held_burn = held.world().resource::<ShipKinetics>().fuel_burn;
        let advancing_burn = advancing.world().resource::<ShipKinetics>().fuel_burn;
        let expected = held_burn * tier_profile(2).fuel_burn_factor;
        assert!(
            (advancing_burn - expected).abs() < 1e-4,
            "fuel did not re-price on the advancement tick \
             ({advancing_burn} vs expected {expected})"
        );
        // ...and the belts at the new throughput.
        let moved = |sim: &SimWorld| sim.world().resource::<InteriorGrid>().rooms[2].input_buffer;
        let held_units = moved(&held);
        let advancing_units = moved(&advancing);
        assert_eq!(
            advancing_units - held_units,
            tier_profile(2).belt_units_per_tick - tier_profile(1).belt_units_per_tick,
            "belts did not switch throughput on the advancement tick"
        );
    }

    /// Spec 015 section 6 test 1: overlap, out-of-bounds,
    /// unaffordable, and fixed-room orders are rejected with a typed
    /// reason, and a rejected order touches nothing.
    #[test]
    fn build_orders_validate_and_reject_atomically() {
        let mut sim = SimWorld::new(101);
        let scrap = CargoHold::index(ResourceKind::FrozenScrap);
        let grid_before = sim.world().resource::<InteriorGrid>().clone();
        let scrap_before = sim.world().resource::<CargoHold>().amounts[scrap];
        // Storage is 2x2: origin (15, 7) runs off both edges.
        sim.push_command(Command::PlaceRoom {
            kind: RoomKind::Storage,
            origin: CellAddr {
                deck: 0,
                x: 15,
                y: 7,
            },
        });
        // The pre-placed engine core occupies (7..=8, 3..=4).
        sim.push_command(Command::PlaceRoom {
            kind: RoomKind::Storage,
            origin: CellAddr {
                deck: 0,
                x: 8,
                y: 4,
            },
        });
        // The engine core is fixed: never player-placed.
        sim.push_command(Command::PlaceRoom {
            kind: RoomKind::EngineCore,
            origin: CellAddr {
                deck: 0,
                x: 1,
                y: 1,
            },
        });
        sim.tick();
        let reasons: Vec<RejectReason> = sim
            .world()
            .resource::<BuildLog>()
            .0
            .iter()
            .map(|rejection| rejection.reason)
            .collect();
        assert_eq!(
            reasons,
            vec![
                RejectReason::OutOfBounds,
                RejectReason::Overlap,
                RejectReason::FixedRoom
            ]
        );
        assert_eq!(*sim.world().resource::<InteriorGrid>(), grid_before);
        assert_eq!(
            sim.world().resource::<CargoHold>().amounts[scrap],
            scrap_before,
            "a rejected order moved the build currency"
        );

        // Unaffordable: an empty hold rejects the order untouched.
        sim.world.resource_mut::<CargoHold>().amounts = [0; ResourceKind::ALL.len()];
        sim.push_command(Command::PlaceRoom {
            kind: RoomKind::Storage,
            origin: CellAddr {
                deck: 0,
                x: 1,
                y: 1,
            },
        });
        sim.tick();
        assert_eq!(
            sim.world().resource::<BuildLog>().0[0].reason,
            RejectReason::Unaffordable
        );
        assert_eq!(*sim.world().resource::<InteriorGrid>(), grid_before);
    }

    /// Spec 015 section 6 test 2: build, run, tear out, rebuild: no
    /// resource is created or destroyed except the declared refund
    /// loss. Jettison refunds nothing and sheds mass at once. The
    /// ship stays still so nothing else moves frozen scrap.
    #[test]
    fn refit_conserves_materials_except_the_declared_loss() {
        let mut sim = SimWorld::new(103);
        let scrap = CargoHold::index(ResourceKind::FrozenScrap);
        let start = sim.world().resource::<CargoHold>().amounts[scrap];
        let cost = room_spec(RoomKind::Foundry).build_cost[scrap];
        let refund = cost / grid::REFUND_DIVISOR;

        sim.push_command(Command::PlaceRoom {
            kind: RoomKind::Foundry,
            origin: CellAddr {
                deck: 0,
                x: 1,
                y: 1,
            },
        });
        sim.tick();
        let placed_id = sim.world().resource::<InteriorGrid>().rooms[1].id;
        assert_eq!(
            sim.world().resource::<CargoHold>().amounts[scrap],
            start - cost
        );
        for _ in 0..50 {
            sim.tick();
        }
        sim.push_command(Command::RemoveRoom { room: placed_id });
        sim.tick();
        assert_eq!(
            sim.world().resource::<CargoHold>().amounts[scrap],
            start - cost + refund,
            "tear-out lost something other than the declared refund loss"
        );

        // Rebuild, then jettison: no refund, immediate mass relief.
        sim.push_command(Command::PlaceRoom {
            kind: RoomKind::Foundry,
            origin: CellAddr {
                deck: 0,
                x: 1,
                y: 1,
            },
        });
        sim.tick();
        let rebuilt_id = sim.world().resource::<InteriorGrid>().rooms[1].id;
        let mass_loaded = sim.world().resource::<ShipKinetics>().total_mass;
        sim.push_command(Command::Jettison { room: rebuilt_id });
        sim.tick();
        assert_eq!(
            sim.world().resource::<CargoHold>().amounts[scrap],
            start - cost + refund - cost,
            "jettison refunded material"
        );
        assert!(
            sim.world().resource::<ShipKinetics>().total_mass
                < mass_loaded - room_spec(RoomKind::Foundry).mass / 2.0,
            "jettison did not shed the room's mass"
        );
    }

    /// Spec 015 section 6 test 3: two identical layouts with
    /// identical inputs route identically; edge processing order is
    /// creation order, and a full destination back-pressures with
    /// nothing vanishing.
    #[test]
    fn spine_routes_in_creation_order_with_back_pressure() {
        let build = || {
            let mut sim = SimWorld::new(107);
            // Two suppliers flanking one destination, all adjacent.
            for (kind, x) in [
                (RoomKind::Storage, 1u8),
                (RoomKind::Storage, 5),
                (RoomKind::Storage, 3),
            ] {
                sim.push_command(Command::PlaceRoom {
                    kind,
                    origin: CellAddr { deck: 0, x, y: 1 },
                });
            }
            sim.push_command(Command::LayEdge {
                kind: SpineKind::Belt,
                from: CellAddr {
                    deck: 0,
                    x: 2,
                    y: 1,
                },
                to: CellAddr {
                    deck: 0,
                    x: 3,
                    y: 1,
                },
            });
            sim.push_command(Command::LayEdge {
                kind: SpineKind::Belt,
                from: CellAddr {
                    deck: 0,
                    x: 5,
                    y: 1,
                },
                to: CellAddr {
                    deck: 0,
                    x: 4,
                    y: 1,
                },
            });
            sim.tick();
            assert!(
                sim.world().resource::<BuildLog>().0.is_empty(),
                "the layout itself was rejected: {:?}",
                sim.world().resource::<BuildLog>().0
            );
            {
                let mut interior = sim.world.resource_mut::<InteriorGrid>();
                interior.rooms[1].output_buffer = 5;
                interior.rooms[2].output_buffer = 5;
                // One slot left in the destination's input buffer.
                let capacity = interior.rooms[3].spec().buffer_capacity;
                interior.rooms[3].input_buffer = capacity - 1;
            }
            sim
        };
        let mut a = build();
        let mut b = build();

        a.tick();
        let interior = a.world().resource::<InteriorGrid>();
        assert_eq!(
            interior.rooms[1].output_buffer, 4,
            "the first-created edge should win the last slot"
        );
        assert_eq!(
            interior.rooms[2].output_buffer, 5,
            "the later edge should back-pressure, not drop"
        );
        let total: u32 = interior
            .rooms
            .iter()
            .map(|room| room.input_buffer + room.output_buffer)
            .sum();

        for _ in 0..30 {
            a.tick();
            b.tick();
        }
        b.tick();
        let after: u32 = a
            .world()
            .resource::<InteriorGrid>()
            .rooms
            .iter()
            .map(|room| room.input_buffer + room.output_buffer)
            .sum();
        assert_eq!(total, after, "units vanished or appeared in transit");
        assert_eq!(
            a.save_bytes(),
            b.save_bytes(),
            "identical layouts routed differently"
        );
    }

    /// Spec 015 section 6 test 4: a breach disables exactly its
    /// cell's room and the dependents its severed spine cut off,
    /// deterministically; a redundant route keeps its consumer fed.
    #[test]
    fn breach_disables_its_cell_and_severed_dependents() {
        let mut sim = SimWorld::new(109);
        // Supplier S on the hull row over node 1 (columns 2..=3),
        // consumer D safely below it, and a redundant supplier R
        // beside D, off the hull row.
        sim.push_command(Command::PlaceRoom {
            kind: RoomKind::Storage,
            origin: CellAddr {
                deck: 0,
                x: 2,
                y: 0,
            },
        });
        sim.push_command(Command::PlaceRoom {
            kind: RoomKind::Storage,
            origin: CellAddr {
                deck: 0,
                x: 2,
                y: 2,
            },
        });
        sim.push_command(Command::PlaceRoom {
            kind: RoomKind::Storage,
            origin: CellAddr {
                deck: 0,
                x: 4,
                y: 2,
            },
        });
        sim.push_command(Command::LayEdge {
            kind: SpineKind::Belt,
            from: CellAddr {
                deck: 0,
                x: 2,
                y: 1,
            },
            to: CellAddr {
                deck: 0,
                x: 2,
                y: 2,
            },
        });
        sim.push_command(Command::LayEdge {
            kind: SpineKind::Belt,
            from: CellAddr {
                deck: 0,
                x: 4,
                y: 2,
            },
            to: CellAddr {
                deck: 0,
                x: 3,
                y: 2,
            },
        });
        sim.tick();
        assert!(
            sim.world().resource::<BuildLog>().0.is_empty(),
            "the layout itself was rejected: {:?}",
            sim.world().resource::<BuildLog>().0
        );
        {
            let mut interior = sim.world.resource_mut::<InteriorGrid>();
            interior.rooms[1].output_buffer = 10;
            interior.rooms[3].output_buffer = 10;
        }
        sim.tick();
        sim.tick();
        let fed_by_both = sim.world().resource::<InteriorGrid>().rooms[2].input_buffer;
        assert_eq!(fed_by_both, 4, "both routes should feed the consumer");

        // Breach node 1: its hull-row cells (2..=3, 0) disable S and
        // sever the S-to-D belt; R's redundant route keeps feeding.
        sim.world.resource_mut::<HullGraph>().stress[1] = 1.0;
        // Drones would repair the breach; this test wants it held.
        sim.world.resource_mut::<DroneFleet>().drones.clear();
        for _ in 0..4 {
            sim.tick();
        }
        let interior = sim.world().resource::<InteriorGrid>();
        let hull = sim.world().resource::<HullGraph>();
        assert!(interior.room_disabled(&interior.rooms[1], hull));
        assert!(!interior.room_disabled(&interior.rooms[2], hull));
        assert!(!interior.room_disabled(&interior.rooms[3], hull));
        assert_eq!(
            interior.rooms[1].output_buffer, 8,
            "the severed route kept moving units"
        );
        assert_eq!(
            interior.rooms[2].input_buffer,
            fed_by_both + 4,
            "the redundant route stopped feeding the consumer"
        );
    }

    /// Spec 014 section 5 test 2: field lookups draw nothing from
    /// the event RNG. Two identical runs, one sensing the field
    /// heavily every tick: byte-identical end states.
    #[test]
    fn field_queries_never_perturb_the_event_stream() {
        let mut plain = SimWorld::new(77);
        let mut sensing = SimWorld::new(77);
        for sim in [&mut plain, &mut sensing] {
            sim.push_command(Command::SetThrottle { throttle: 1.0 });
        }
        for t in 0..150 {
            plain.tick();
            sensing.tick();
            for probe in 0..40 {
                let position = [f64::from(t) * 3.0, f64::from(probe) * 5.0];
                let _ = sensing.ice_class_at(position);
            }
        }
        assert_eq!(plain.save_bytes(), sensing.save_bytes());
    }

    /// Spec 014 section 5 test 3: reveal is monotonic, and zero
    /// sensor coverage halts growth without erasing the map.
    #[test]
    fn fog_reveal_is_monotonic_and_halts_without_sensors() {
        let mut sim = SimWorld::new(71);
        sim.push_command(Command::SetThrottle { throttle: 1.0 });
        let mut last = 0usize;
        for _ in 0..200 {
            sim.tick();
            let revealed = &sim.world().resource::<WorldDomain>().fog.revealed;
            assert!(revealed.len() >= last, "the revealed set shrank");
            last = revealed.len();
        }
        assert!(last > 0, "nothing was revealed under way");
        // Freeze every sensor: the coverage readback drops to zero,
        // growth halts, and the map is not erased. The capability
        // lag means the tick after the freeze still reveals, so the
        // baseline is captured after it.
        sim.world.resource_mut::<Equipment>().sensor_frozen = [u32::MAX; GRID_W];
        sim.tick();
        let frozen = sim.world().resource::<WorldDomain>().fog.revealed.clone();
        assert!(!frozen.is_empty(), "zero coverage erased the map");
        for _ in 0..50 {
            sim.tick();
            assert_eq!(
                sim.world().resource::<WorldDomain>().fog.revealed,
                frozen,
                "reveal state changed with zero sensor coverage"
            );
        }
    }

    /// Spec 014 section 5 test 4: crossing from pancake into pack
    /// ice raises measured fuel burn and impact stress in the
    /// profiled direction, and breaking ice wears the prow.
    #[test]
    fn class_profiles_price_the_crossing() {
        let seed = 5;
        let find = |wanted: IceClass| -> (i64, i64) {
            for y in -200..200 {
                for x in -200..200 {
                    if class_of_cell(seed, (x, y)) == wanted {
                        return (x, y);
                    }
                }
            }
            panic!("no {wanted:?} cell in the search window");
        };
        let pancake = find(IceClass::PancakeIce);
        let pack = find(IceClass::PackIce);

        let burn_in = |cell: (i64, i64)| -> f32 {
            let mut sim = SimWorld::new(seed);
            sim.push_command(Command::SetThrottle { throttle: 1.0 });
            sim.world.resource_mut::<ShipKinetics>().position = cell_center(cell);
            sim.tick();
            sim.world().resource::<ShipKinetics>().fuel_burn
        };
        assert!(
            burn_in(pack) > burn_in(pancake),
            "pack ice should burn more fuel per second than pancake"
        );

        let stress_in = |cell: (i64, i64)| -> (f32, f32) {
            let mut sim = SimWorld::new(seed);
            sim.push_command(Command::SetThrottle { throttle: 1.0 });
            sim.world.resource_mut::<DroneFleet>().drones.clear();
            for _ in 0..600 {
                // Pin the ship inside the class under test each tick.
                sim.world.resource_mut::<ShipKinetics>().position = cell_center(cell);
                sim.tick();
            }
            let stress: f32 = sim.world().resource::<HullGraph>().stress.iter().sum();
            (stress, sim.world().resource::<ShipKinetics>().prow_wear)
        };
        let (pancake_stress, pancake_wear) = stress_in(pancake);
        let (pack_stress, pack_wear) = stress_in(pack);
        assert!(
            pack_stress > pancake_stress,
            "pack ice should stress the hull more ({pack_stress} vs {pancake_stress})"
        );
        assert!(
            pack_wear > pancake_wear,
            "heavier ice should wear the prow faster"
        );
        assert!(pancake_wear > 0.0, "breaking ice should wear the prow");
    }

    /// An overheating core draws melted intake ice as coolant: the
    /// stock drains in whole units and the temperature comes down
    /// (spec 004 section 5).
    #[test]
    fn coolant_draws_intake_ice() {
        let mut sim = SimWorld::new(19);
        let ice = CargoHold::index(ResourceKind::Ice);
        let before = sim.world.resource::<CargoHold>().amounts[ice];
        sim.world.resource_mut::<EngineCore>().temperature = 200.0;
        for _ in 0..200 {
            sim.tick();
        }
        let after = sim.world.resource::<CargoHold>().amounts[ice];
        assert!(after < before, "no ice drawn for coolant");
        assert!(
            sim.world.resource::<EngineCore>().temperature < 200.0,
            "coolant failed to pull the core down"
        );
    }
}

/// A `bevy_ecs` resource handle for hosts that store the sim inside a
/// larger `World` (the app of spec 012). The sim stays the one truth;
/// the wrapper is just transport.
#[derive(Resource)]
pub struct SimHandle(pub SimWorld);
