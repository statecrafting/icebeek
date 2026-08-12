//! The tech domain (spec 016, spec 007): research accrual, the
//! blueprint set, tier gates, and the tier profiles that re-price
//! the whole ship on advancement. Research accrues only when a
//! working Refinery processes AncientTech from cargo; blueprints
//! arrive from the world as Salvage events; a tier transition swaps
//! the paradigm profile atomically on one tick.

use std::collections::BTreeSet;

use bevy_ecs::prelude::Resource;
use serde::{Deserialize, Serialize};

/// The three-tier progression arc (spec 007 section 3).
pub const MAX_TIER: u8 = 3;
/// Blueprint ids the world can deliver. Balancing data.
pub const BLUEPRINT_POOL: u32 = 5;
/// Research per AncientTech unit a Refinery processes (spec 016
/// section 2). Balancing data; per-unit accrual is pinned.
pub const RESEARCH_PER_TECH: u64 = 5;
/// Research a duplicate blueprint find converts into (spec 016
/// section 3).
pub const DUPLICATE_BLUEPRINT_RESEARCH: u64 = 10;
/// AncientTech units one working Refinery processes per second,
/// metered in whole units.
pub const REFINE_UNITS_PER_SECOND: f32 = 0.5;

/// One paradigm's economy (spec 016 section 5, spec 007 section 3):
/// what a tier transition swaps, atomically, on the tick of
/// advancement. Contents are balancing data; the atomic swap is the
/// pinned mechanism.
#[derive(Debug, Clone, PartialEq)]
pub struct TierProfile {
    /// Scales the fuel demand of the whole burn line.
    pub fuel_burn_factor: f32,
    /// Scales every room's heat emission and absorption.
    pub heat_emission_factor: f32,
    /// Whole units a belt or pipe edge moves per tick.
    pub belt_units_per_tick: u32,
    /// Scales drone repair throughput.
    pub drone_throughput_factor: f32,
}

const TIER_1: TierProfile = TierProfile {
    fuel_burn_factor: 1.0,
    heat_emission_factor: 1.0,
    belt_units_per_tick: 1,
    drone_throughput_factor: 1.0,
};
const TIER_2: TierProfile = TierProfile {
    fuel_burn_factor: 1.4,
    heat_emission_factor: 1.25,
    belt_units_per_tick: 2,
    drone_throughput_factor: 1.5,
};
const TIER_3: TierProfile = TierProfile {
    fuel_burn_factor: 2.0,
    heat_emission_factor: 1.6,
    belt_units_per_tick: 3,
    drone_throughput_factor: 2.25,
};

/// The profile table, exhaustive over the tier arc: what makes a
/// tier-1 layout obsolete inside a tier-2 economy (spec 007
/// section 2).
pub const fn tier_profile(tier: u8) -> &'static TierProfile {
    match tier {
        0 | 1 => &TIER_1,
        2 => &TIER_2,
        _ => &TIER_3,
    }
}

/// The blueprint set that must be complete before advancing TO the
/// given tier (spec 016 section 3): the wall between tiers stays a
/// Macro risk decision. Balancing data.
pub fn blueprints_required(tier: u8) -> &'static [u32] {
    match tier {
        2 => &[1, 2],
        3 => &[3, 4, 5],
        _ => &[],
    }
}

/// The research spend advancing TO the given tier costs. Balancing
/// data.
pub fn research_cost(tier: u8) -> u64 {
    match tier {
        2 => 50,
        3 => 200,
        _ => 0,
    }
}

/// The tech domain (spec 010 section 5, spec 016): research accrual,
/// tier, blueprints. The blueprint set is a `BTreeSet` so iteration
/// and serialization stay sorted and deterministic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Resource)]
pub struct TechDomain {
    pub tier: u8,
    pub research: u64,
    /// Unique flags, never stock (spec 016 section 3).
    pub blueprints: BTreeSet<u32>,
    /// Fractional AncientTech consumption owed by refinery
    /// processing.
    pub refine_meter: f32,
}

impl Default for TechDomain {
    fn default() -> Self {
        Self {
            tier: 1,
            research: 0,
            blueprints: BTreeSet::new(),
            refine_meter: 0.0,
        }
    }
}

impl TechDomain {
    /// The active paradigm's economy.
    pub fn profile(&self) -> &'static TierProfile {
        tier_profile(self.tier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every reachable tier has a profile, and the arc re-prices in
    /// the profiled direction: later paradigms burn hotter and move
    /// faster (spec 007 sections 2 and 3).
    #[test]
    fn profiles_cover_the_arc_and_reprice_upward() {
        for tier in 1..MAX_TIER {
            let this = tier_profile(tier);
            let next = tier_profile(tier + 1);
            assert!(next.fuel_burn_factor > this.fuel_burn_factor);
            assert!(next.heat_emission_factor > this.heat_emission_factor);
            assert!(next.belt_units_per_tick > this.belt_units_per_tick);
            assert!(next.drone_throughput_factor > this.drone_throughput_factor);
        }
    }

    /// Every tier above 1 costs blueprints and research; every
    /// required blueprint is inside the world's deliverable pool.
    #[test]
    fn tier_walls_are_real_and_deliverable() {
        for tier in 2..=MAX_TIER {
            assert!(!blueprints_required(tier).is_empty());
            assert!(research_cost(tier) > 0);
            for blueprint in blueprints_required(tier) {
                assert!((1..=BLUEPRINT_POOL).contains(blueprint));
            }
        }
    }
}
