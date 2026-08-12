//! The deterministic fixed-tick simulation core (spec 010).
//!
//! One `bevy_ecs` `World` of truth, advanced only by explicit `tick()`
//! calls from the host. Four phases per tick, single-threaded, totally
//! ordered: commands, world, interior, readback (spec 010 section 3).
//! The bootstrap slice implements a thin vertical path through every
//! phase so the determinism test contract is real from the first commit.

mod rng;
mod save;
mod state;
mod view;
mod world_field;

use bevy_ecs::prelude::*;
use bevy_ecs::schedule::{LogLevel, ScheduleBuildSettings, SingleThreadedExecutor};
use bevy_ecs::system::ScheduleSystem;
use serde::{Deserialize, Serialize};

pub use icebeek_events as events;
pub use rng::SimRng;
pub use save::{Migration, SAVE_FORMAT_VERSION, SaveError};
pub use state::{
    AMBIENT_C, AutomationRule, AutomationRules, COMPARTMENTS, Capability, CargoHold, Command,
    CommandQueue, Condition, Drone, DroneFleet, DroneKind, DroneZone, EngineCore, Equipment,
    EventBus, ExpeditionState, FogOfWinter, HULL_NODES, Helm, HullGraph, OPERATING_C, Routing,
    RuleAction, SHUTDOWN_LADDER, ShipKinetics, ShipSystem, SimTick, ThermalField, WeatherState,
    WorldDomain,
};
pub use view::{SimSnapshot, SimSnapshots, TERRAIN_VIEW_SIDE, TerrainView};
pub use world_field::{
    CELL_UNITS, IceClass, IceClassProfile, cell_center, cell_of, class_at, class_of_cell, profile,
};

use icebeek_events::{EventPayload, ExpeditionEvent, ResourceKind, WeatherEvent};

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
/// Compartment-to-compartment diffusion rate, per second.
const DIFFUSION_PER_SECOND: f32 = 0.4;
/// Compartment leak toward the ambient exterior, per second.
const HULL_LEAK_PER_SECOND: f32 = 0.02;
/// Core-to-compartment thermal coupling, per second.
const CORE_COUPLING_PER_SECOND: f32 = 0.1;
/// Compartment hosting the engine core.
const CORE_COMPARTMENT: usize = 0;

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
    pub drones: DroneFleet,
    pub rules: AutomationRules,
    pub routing: Routing,
    pub equipment: Equipment,
    pub world: WorldDomain,
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
        world.insert_resource(DroneFleet::default());
        world.insert_resource(AutomationRules::default());
        world.insert_resource(Routing::default());
        world.insert_resource(Equipment::default());
        world.insert_resource(WorldDomain {
            map_seed: seed,
            ..WorldDomain::default()
        });
        world.insert_resource(CargoHold::starting_provisions());
        world.insert_resource(Capability::default());
        world.insert_resource(SimRng::from_seed(seed));
        world.insert_resource(EventBus::default());
        world.insert_resource(CommandQueue::default());
        world.insert_resource(PhaseLog::default());
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
            compartment_temps: self.world.resource::<ThermalField>().temps,
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
            drones: self.world.resource::<DroneFleet>().clone(),
            rules: self.world.resource::<AutomationRules>().clone(),
            routing: self.world.resource::<Routing>().clone(),
            equipment: self.world.resource::<Equipment>().clone(),
            world: self.world.resource::<WorldDomain>().clone(),
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
        world.insert_resource(save.drones);
        world.insert_resource(save.rules);
        world.insert_resource(save.routing);
        world.insert_resource(save.equipment);
        world.insert_resource(save.world);
        world.insert_resource(save.cargo);
        world.insert_resource(save.capability);
        world.insert_resource(save.rng);
        world.insert_resource(save.events);
        world.insert_resource(save.commands);
        world.insert_resource(PhaseLog::default());
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

/// Commands phase: apply queued player commands in push order.
fn apply_commands(
    mut queue: ResMut<CommandQueue>,
    mut helm: ResMut<Helm>,
    mut rules: ResMut<AutomationRules>,
    mut fleet: ResMut<DroneFleet>,
    mut equipment: ResMut<Equipment>,
) {
    while let Some(command) = queue.pending.pop_front() {
        match command {
            Command::SetHeading { heading_rad } => helm.heading_rad = heading_rad,
            Command::SetThrottle { throttle } => helm.throttle = throttle.clamp(0.0, 1.0),
            Command::SetAnchor { anchored } => helm.anchor_ordered = anchored,
            Command::ManualThaw { node } => {
                let node = node as usize % HULL_NODES;
                equipment.valve_frozen[node] = 0;
                equipment.sensor_frozen[node] = 0;
            }
            Command::AddRule { condition, action } => {
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
                    let last = (HULL_NODES - 1) as u32;
                    let from = zone.from.min(last);
                    drone.zone = DroneZone {
                        from,
                        to: zone.to.clamp(from, last),
                    };
                }
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
    domain: Res<WorldDomain>,
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
    let cargo_units: u64 = cargo.amounts.iter().sum();
    kinetics.total_mass = BASE_MASS + cargo_units as f32 * CARGO_UNIT_MASS;
    kinetics.torque_demand =
        kinetics.total_mass * speed / (BASE_MASS * MAX_SPEED) * profile.fuel_cost_factor;
    kinetics.fuel_burn = IDLE_BURN_PER_SECOND + TORQUE_BURN_PER_SECOND * kinetics.torque_demand;
    let step = f64::from(speed) * f64::from(TICK_SECONDS);
    kinetics.prow_wear = (kinetics.prow_wear + profile.prow_wear_per_unit * step as f32).min(1.0);
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
    let profile = world_field::profile(world_field::class_at(domain.map_seed, kinetics.position));
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
    }
}

/// Interior phase: events become state changes, the only translation
/// (spec 002 section 3 rule 2).
fn consume_events(
    mut bus: ResMut<EventBus>,
    mut hull: ResMut<HullGraph>,
    mut cargo: ResMut<CargoHold>,
    mut equipment: ResMut<Equipment>,
    mut fleet: ResMut<DroneFleet>,
) {
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
                equipment.valve_frozen[node as usize % HULL_NODES] = THAW_TICKS;
            }
            EventPayload::Weather(WeatherEvent::SensorFreeze { node }) => {
                equipment.sensor_frozen[node as usize % HULL_NODES] = THAW_TICKS;
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

/// Interior phase: tier-1 rule evaluation, first thing after the
/// event queue snapshot of the world phase. Rules run in stored list
/// order; later rules win conflicting writes (spec 010 section 5,
/// spec 005 section 3).
fn evaluate_rules(
    rules: Res<AutomationRules>,
    engine: Res<EngineCore>,
    hull: Res<HullGraph>,
    cargo: Res<CargoHold>,
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
fn burn_fuel(kinetics: Res<ShipKinetics>, field: Res<ThermalField>, mut core: ResMut<EngineCore>) {
    let demand = kinetics.fuel_burn * TICK_SECONDS;
    let burned = demand.min(core.fuel_buffer);
    core.fuel_buffer -= burned;
    let heat = burned * HEAT_PER_FUEL;
    let leak =
        CORE_LEAK_PER_SECOND * (core.temperature - field.temps[CORE_COMPARTMENT]) * TICK_SECONDS;
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

/// Interior phase: thermal relaxation over the compartment ring,
/// double buffered so iteration order cannot leak into state (spec 010
/// section 5). Cold is the default (spec 004 section 5).
fn update_thermal_field(core: Res<EngineCore>, mut field: ResMut<ThermalField>) {
    let old = field.temps;
    for (i, temp) in field.temps.iter_mut().enumerate() {
        let left = old[(i + COMPARTMENTS - 1) % COMPARTMENTS];
        let right = old[(i + 1) % COMPARTMENTS];
        let mut delta = DIFFUSION_PER_SECOND * (left + right - 2.0 * old[i])
            + HULL_LEAK_PER_SECOND * (AMBIENT_C - old[i]);
        if i == CORE_COMPARTMENT {
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
/// most stressed node in its zone (ties to the lowest index), drawing
/// strut feedstock from the hold and accruing wear. Drones idle while
/// the ladder has the drone bays down.
fn run_drones(
    core: Res<EngineCore>,
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
        for node in drone.zone.from..=drone.zone.to {
            let node = node as usize;
            if node >= HULL_NODES || hull.stress[node] <= 0.0 {
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
        let repaired = REPAIR_RATE_PER_SECOND * uptime * TICK_SECONDS;
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
        let before = sim.world.resource::<ThermalField>().temps;
        for _ in 0..600 {
            sim.tick();
        }
        let after = sim.world.resource::<ThermalField>().temps;
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
        // Shrink both zones onto nodes 0..=3, leaving node 6 orphaned.
        sim.push_command(Command::SetDroneZone {
            drone: 1,
            zone: DroneZone { from: 2, to: 3 },
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
        assert_eq!(equipment.valve_frozen[2], THAW_TICKS - 1);
        assert_eq!(equipment.sensor_frozen[3], THAW_TICKS - 1);
        let capability = sim.world.resource::<Capability>();
        assert!(capability.intake_capacity < 1.0);
        assert!(capability.sensor_coverage < 1.0);
        for _ in 0..THAW_TICKS {
            sim.tick();
        }
        let equipment = sim.world.resource::<Equipment>();
        assert_eq!(equipment.valve_frozen[2], 0, "the valve never thawed");
        assert_eq!(equipment.sensor_frozen[3], 0, "the sensor never thawed");
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
        assert!(sim.world.resource::<Equipment>().valve_frozen[5] > 0);
        sim.push_command(Command::ManualThaw { node: 5 });
        sim.tick();
        assert_eq!(sim.world.resource::<Equipment>().valve_frozen[5], 0);
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
        sim.world.resource_mut::<Equipment>().sensor_frozen = [u32::MAX; HULL_NODES];
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
