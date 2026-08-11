//! The deterministic fixed-tick simulation core (spec 010).
//!
//! One `bevy_ecs` `World` of truth, advanced only by explicit `tick()`
//! calls from the host. Four phases per tick, single-threaded, totally
//! ordered: commands, world, interior, readback (spec 010 section 3).
//! The bootstrap slice implements a thin vertical path through every
//! phase so the determinism test contract is real from the first commit.

mod rng;
mod state;

use bevy_ecs::prelude::*;
use bevy_ecs::schedule::{LogLevel, ScheduleBuildSettings, SingleThreadedExecutor};
use bevy_ecs::system::ScheduleSystem;
use serde::{Deserialize, Serialize};

pub use icebeek_events as events;
pub use rng::SimRng;
pub use state::{
    Capability, CargoHold, Command, CommandQueue, EventBus, HULL_NODES, Helm, HullGraph,
    ShipKinetics, SimTick,
};

use icebeek_events::{EventPayload, ResourceKind};

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
/// Passive stress decay from the standing repair line, per second.
const REPAIR_RATE_PER_SECOND: f32 = 0.05;

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
/// the RNG state, nothing else (spec 010 section 7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveState {
    /// Tick rate the save was written under; loading refuses a mismatch
    /// until the save-versioning spec exists.
    pub tick_hz: u32,
    pub tick: SimTick,
    pub helm: Helm,
    pub kinetics: ShipKinetics,
    pub hull: HullGraph,
    pub cargo: CargoHold,
    pub capability: Capability,
    pub rng: SimRng,
    pub events: EventBus,
    pub commands: CommandQueue,
}

#[derive(Debug)]
pub enum SaveError {
    TickRateMismatch { saved: u32, expected: u32 },
    Decode(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::TickRateMismatch { saved, expected } => {
                write!(
                    f,
                    "save written at {saved} Hz, this build runs {expected} Hz"
                )
            }
            SaveError::Decode(err) => write!(f, "save decode failed: {err}"),
        }
    }
}

impl std::error::Error for SaveError {}

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
        world.insert_resource(CargoHold::default());
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
            world_phase: phase_schedule((mark_world, drive_kinetics, generate_ice_events).chain()),
            interior_phase: phase_schedule((mark_interior, consume_events, run_repairs).chain()),
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
    pub fn last_trace(&self) -> &TickTrace {
        &self.last_trace
    }

    pub fn save(&self) -> SaveState {
        SaveState {
            tick_hz: TICK_HZ,
            tick: self.world.resource::<SimTick>().clone(),
            helm: self.world.resource::<Helm>().clone(),
            kinetics: self.world.resource::<ShipKinetics>().clone(),
            hull: self.world.resource::<HullGraph>().clone(),
            cargo: self.world.resource::<CargoHold>().clone(),
            capability: self.world.resource::<Capability>().clone(),
            rng: self.world.resource::<SimRng>().clone(),
            events: self.world.resource::<EventBus>().clone(),
            commands: self.world.resource::<CommandQueue>().clone(),
        }
    }

    pub fn from_save(save: SaveState) -> Result<Self, SaveError> {
        if save.tick_hz != TICK_HZ {
            return Err(SaveError::TickRateMismatch {
                saved: save.tick_hz,
                expected: TICK_HZ,
            });
        }
        let mut world = World::new();
        world.insert_resource(save.tick);
        world.insert_resource(save.helm);
        world.insert_resource(save.kinetics);
        world.insert_resource(save.hull);
        world.insert_resource(save.cargo);
        world.insert_resource(save.capability);
        world.insert_resource(save.rng);
        world.insert_resource(save.events);
        world.insert_resource(save.commands);
        world.insert_resource(PhaseLog::default());
        Ok(Self::with_world(world))
    }

    pub fn save_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&self.save()).expect("save state serializes")
    }

    pub fn from_save_bytes(bytes: &[u8]) -> Result<Self, SaveError> {
        let save: SaveState =
            serde_json::from_slice(bytes).map_err(|e| SaveError::Decode(e.to_string()))?;
        Self::from_save(save)
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
fn apply_commands(mut queue: ResMut<CommandQueue>, mut helm: ResMut<Helm>) {
    while let Some(command) = queue.pending.pop_front() {
        match command {
            Command::SetHeading { heading_rad } => helm.heading_rad = heading_rad,
            Command::SetThrottle { throttle } => helm.throttle = throttle.clamp(0.0, 1.0),
        }
    }
}

/// World phase: gross motion, capped by the capability the interior
/// reported last tick (spec 002 section 3 rule 3).
fn drive_kinetics(
    mut kinetics: ResMut<ShipKinetics>,
    helm: Res<Helm>,
    capability: Res<Capability>,
) {
    let speed = helm.throttle.min(capability.available_thrust) * MAX_SPEED;
    kinetics.speed = speed;
    let step = f64::from(speed) * f64::from(TICK_SECONDS);
    kinetics.position[0] += f64::from(helm.heading_rad.cos()) * step;
    kinetics.position[1] += f64::from(helm.heading_rad.sin()) * step;
}

/// World phase: the event generator, the only RNG consumer in the
/// simulation (spec 010 section 4 rule 4).
fn generate_ice_events(
    tick: Res<SimTick>,
    kinetics: Res<ShipKinetics>,
    mut rng: ResMut<SimRng>,
    mut bus: ResMut<EventBus>,
) {
    if kinetics.speed <= 0.0 {
        return;
    }
    let intensity = kinetics.speed / MAX_SPEED;
    if rng.next_f32() < IMPACT_CHANCE_PER_SECOND * intensity * TICK_SECONDS {
        let node = rng.next_range(HULL_NODES as u32);
        let magnitude = 0.05 + 0.20 * rng.next_f32();
        bus.emit(tick.0, EventPayload::Impact { node, magnitude });
    }
    if rng.next_f32() < INGEST_CHANCE_PER_SECOND * intensity * TICK_SECONDS {
        let resource = ResourceKind::ALL[rng.next_range(ResourceKind::ALL.len() as u32) as usize];
        let amount = 1 + rng.next_range(5);
        bus.emit(tick.0, EventPayload::Ingestion { resource, amount });
    }
}

/// Interior phase: events become state changes, the only translation
/// (spec 002 section 3 rule 2).
fn consume_events(
    mut bus: ResMut<EventBus>,
    mut hull: ResMut<HullGraph>,
    mut cargo: ResMut<CargoHold>,
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
            // Families are defined (spec 011); their interior resolution
            // arrives with the matching state domains of spec 010.
            EventPayload::Weather(_) | EventPayload::Expedition(_) => {}
        }
    }
}

/// Interior phase: the standing repair line decays stress (spec 005
/// section 4, minimal placeholder).
fn run_repairs(mut hull: ResMut<HullGraph>) {
    for stress in &mut hull.stress {
        *stress = (*stress - REPAIR_RATE_PER_SECOND * TICK_SECONDS).max(0.0);
    }
}

/// Readback phase: interior outcomes become capability the next world
/// phase reads (spec 002 section 3 rule 3).
fn read_back_capability(hull: Res<HullGraph>, mut capability: ResMut<Capability>) {
    let total: f32 = hull.stress.iter().sum();
    let average = total / HULL_NODES as f32;
    capability.available_thrust = (1.0 - average).clamp(0.0, 1.0);
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
    /// interior turns them into hull stress and cargo, and capability
    /// feeds back below 1.0 once stress exists.
    #[test]
    fn vertical_slice_flows() {
        let sim = scripted_run(42, 400);
        let world = sim.world();
        let kinetics = world.resource::<ShipKinetics>();
        assert!(kinetics.position[0] != 0.0 || kinetics.position[1] != 0.0);
        let cargo = world.resource::<CargoHold>();
        assert!(
            cargo.amounts.iter().sum::<u64>() > 0,
            "no ingestion in 400 ticks"
        );
        let bus = world.resource::<EventBus>();
        assert!(bus.next_seq > 0, "no events emitted in 400 ticks");
    }

    /// Spec 010 section 7: a tick-rate mismatch is refused, not
    /// reinterpreted.
    #[test]
    fn tick_rate_mismatch_refused() {
        let mut save = SimWorld::new(5).save();
        save.tick_hz = TICK_HZ + 1;
        match SimWorld::from_save(save) {
            Err(SaveError::TickRateMismatch { saved, expected }) => {
                assert_eq!(saved, TICK_HZ + 1);
                assert_eq!(expected, TICK_HZ);
            }
            Err(other) => panic!("wrong error: {other:?}"),
            Ok(_) => panic!("expected TickRateMismatch"),
        }
    }
}

/// A `bevy_ecs` resource handle for hosts that store the sim inside a
/// larger `World` (the app of spec 012). The sim stays the one truth;
/// the wrapper is just transport.
#[derive(Resource)]
pub struct SimHandle(pub SimWorld);
