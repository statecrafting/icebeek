//! Typed exterior-traffic vocabulary (spec 011).
//!
//! Everything the world does to the ship travels as one of these values.
//! This crate is pure data: types, the queue container, and their serde
//! and ordering impls. No engine dependency, no systems, no game logic.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Simulation tick of emission. Timestamps are tick-denominated, never
/// wall time (spec 011 section 3).
pub type Tick = u64;

/// Node id on the hull graph. The graph topology is owned by the sim
/// (spec 010 section 5); events only address into it.
pub type HullNodeId = u32;

/// The envelope: emission tick, deterministic sequence number, payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub tick: Tick,
    /// Monotonic within a run, assigned deterministically at emission,
    /// so ordering is total and replay-stable even within a tick.
    pub seq: u64,
    pub payload: EventPayload,
}

/// The initial family roster (spec 011 section 4). New variants and
/// families are amendments to spec 011.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EventPayload {
    /// Prow strike or hull contact: a stress spike at a hull node.
    Impact {
        node: HullNodeId,
        magnitude: f32,
    },
    /// An intake batch entering the primary cargo hold.
    Ingestion {
        resource: ResourceKind,
        amount: u32,
    },
    Weather(WeatherEvent),
    Expedition(ExpeditionEvent),
}

/// The raw materials ground out of the ice (spec 003 section 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceKind {
    FrozenScrap,
    AncientTech,
    Biomass,
    Ice,
}

impl ResourceKind {
    pub const ALL: [ResourceKind; 4] = [
        ResourceKind::FrozenScrap,
        ResourceKind::AncientTech,
        ResourceKind::Biomass,
        ResourceKind::Ice,
    ];
}

/// Weather targets systems, not hit points (spec 006 section 5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WeatherEvent {
    StormOnset,
    StormEnd,
    ValveFreeze { node: HullNodeId },
    SensorFreeze { node: HullNodeId },
    DroneScramble,
    SolarFlareOnset,
    SolarFlareEnd,
}

/// Anchored expeditions trade extraction time against crush risk
/// (spec 006 section 4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExpeditionEvent {
    AnchorSet,
    IceShiftWarning { magnitude: f32 },
    CrushProgress { pressure: f32 },
    RoverReturn,
}

/// FIFO by `(tick, seq)`; backlog is legal and persists in order; the
/// pending queue serializes whole into saves (spec 011 section 5).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EventQueue {
    events: VecDeque<Event>,
}

impl EventQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push an event. Emitters must push in `(tick, seq)` order; the
    /// queue never reorders.
    pub fn push(&mut self, event: Event) {
        if let Some(back) = self.events.back() {
            debug_assert!(
                (event.tick, event.seq) > (back.tick, back.seq),
                "events must be pushed in (tick, seq) order"
            );
        }
        self.events.push_back(event);
    }

    /// Pop the oldest pending event.
    pub fn pop(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Event> {
        self.events.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_of_every_payload() -> Vec<EventPayload> {
        let mut payloads = vec![
            EventPayload::Impact {
                node: 3,
                magnitude: 0.75,
            },
            EventPayload::Weather(WeatherEvent::StormOnset),
            EventPayload::Weather(WeatherEvent::StormEnd),
            EventPayload::Weather(WeatherEvent::ValveFreeze { node: 1 }),
            EventPayload::Weather(WeatherEvent::SensorFreeze { node: 2 }),
            EventPayload::Weather(WeatherEvent::DroneScramble),
            EventPayload::Weather(WeatherEvent::SolarFlareOnset),
            EventPayload::Weather(WeatherEvent::SolarFlareEnd),
            EventPayload::Expedition(ExpeditionEvent::AnchorSet),
            EventPayload::Expedition(ExpeditionEvent::IceShiftWarning { magnitude: 0.5 }),
            EventPayload::Expedition(ExpeditionEvent::CrushProgress { pressure: 0.9 }),
            EventPayload::Expedition(ExpeditionEvent::RoverReturn),
        ];
        for resource in ResourceKind::ALL {
            payloads.push(EventPayload::Ingestion {
                resource,
                amount: 7,
            });
        }
        payloads
    }

    /// Spec 011 section 7 test 1: every variant round-trips to an equal
    /// value.
    #[test]
    fn round_trip_every_variant() {
        for (i, payload) in one_of_every_payload().into_iter().enumerate() {
            let event = Event {
                tick: 42,
                seq: i as u64,
                payload,
            };
            let encoded = serde_json::to_string(&event).expect("serialize");
            let decoded: Event = serde_json::from_str(&encoded).expect("deserialize");
            assert_eq!(event, decoded);
        }
    }

    /// Spec 011 section 7 test 2: the queue drains in (tick, seq) order
    /// and serialization preserves that order exactly.
    #[test]
    fn queue_preserves_order_across_serialization() {
        let mut queue = EventQueue::new();
        let mut expected = Vec::new();
        for (seq, tick) in [1u64, 1, 2, 5, 5, 5, 9].into_iter().enumerate() {
            let event = Event {
                tick,
                seq: seq as u64,
                payload: EventPayload::Impact {
                    node: (seq % 8) as HullNodeId,
                    magnitude: 0.1,
                },
            };
            queue.push(event.clone());
            expected.push(event);
        }

        let encoded = serde_json::to_string(&queue).expect("serialize");
        let mut decoded: EventQueue = serde_json::from_str(&encoded).expect("deserialize");

        let mut drained = Vec::new();
        while let Some(event) = decoded.pop() {
            drained.push(event);
        }
        assert_eq!(expected, drained);
    }
}
