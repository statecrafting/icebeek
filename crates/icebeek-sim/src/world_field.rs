//! The ice field (spec 014): every world position's ice class as a
//! pure function of (map seed, position), computed on demand through
//! seed-keyed hashing. The field is never stored (only the seed is)
//! and draws nothing from the event RNG: querying it, however often,
//! cannot perturb the event stream. Glacial walls ring macro-region
//! borders and partition the map into progression regions; pack and
//! pancake ice band around them; region interiors are open water
//! scattered with seed-hashed pancake floes.

use icebeek_events::ResourceKind;

/// Side length of one world cell in world units. Balancing data.
pub const CELL_UNITS: f64 = 8.0;

/// Cells per macro-region side (spec 014 section 2: region shape is
/// balancing data; the partition itself is pinned).
const REGION_CELLS: i64 = 64;
/// Cell distance from a region border that is glacial wall.
const WALL_BAND: i64 = 1;
/// Cell distance from a region border that is pack ice.
const PACK_BAND: i64 = 5;
/// Cell distance from a region border that is pancake ice.
const PANCAKE_BAND: i64 = 12;
/// Per-mille chance an interior cell is a pancake floe rather than
/// open water; the seed-hashed scatter that makes seeds diverge.
const INTERIOR_FLOE_PER_MILLE: u64 = 220;

/// The four ice classes of spec 006 section 3, mechanized by the
/// spec 014 profile table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IceClass {
    OpenWater,
    PancakeIce,
    PackIce,
    GlacialWall,
}

/// One class's data profile (spec 014 section 3). The axes are the
/// spec's; every value is a balancing placeholder.
#[derive(Debug, Clone, PartialEq)]
pub struct IceClassProfile {
    /// Fraction of nominal speed the class resists at a given thrust
    /// (spec 003 section 2: thrust needed to hold speed).
    pub break_resistance: f32,
    /// Prow degradation per world unit broken through (spec 004
    /// section 2).
    pub prow_wear_per_unit: f32,
    /// Torque multiplier while breaking (spec 005 section 5).
    pub fuel_cost_factor: f32,
    /// Scale on the per-second ingestion chance while moving (spec
    /// 003 section 2).
    pub yield_rate: f32,
    /// Relative ingestion weights, indexed like [`ResourceKind::ALL`].
    pub yield_weights: [u32; ResourceKind::ALL.len()],
    /// Scale on the per-second impact chance while moving (spec 005
    /// section 4).
    pub impact_chance_factor: f32,
    /// Impact magnitude: base plus up to `spread` more.
    pub impact_magnitude_base: f32,
    pub impact_magnitude_spread: f32,
}

const OPEN_WATER: IceClassProfile = IceClassProfile {
    break_resistance: 0.0,
    prow_wear_per_unit: 0.0,
    fuel_cost_factor: 1.0,
    yield_rate: 0.15,
    yield_weights: [0, 0, 2, 3],
    impact_chance_factor: 0.05,
    impact_magnitude_base: 0.01,
    impact_magnitude_spread: 0.04,
};

const PANCAKE_ICE: IceClassProfile = IceClassProfile {
    break_resistance: 0.2,
    prow_wear_per_unit: 0.001,
    fuel_cost_factor: 1.25,
    yield_rate: 1.0,
    yield_weights: [3, 1, 2, 4],
    impact_chance_factor: 0.6,
    impact_magnitude_base: 0.03,
    impact_magnitude_spread: 0.1,
};

const PACK_ICE: IceClassProfile = IceClassProfile {
    break_resistance: 0.35,
    prow_wear_per_unit: 0.004,
    fuel_cost_factor: 2.0,
    yield_rate: 1.4,
    yield_weights: [5, 2, 1, 4],
    impact_chance_factor: 1.2,
    impact_magnitude_base: 0.05,
    impact_magnitude_spread: 0.2,
};

const GLACIAL_WALL: IceClassProfile = IceClassProfile {
    break_resistance: 0.92,
    prow_wear_per_unit: 0.02,
    fuel_cost_factor: 8.0,
    yield_rate: 2.0,
    yield_weights: [4, 5, 0, 2],
    impact_chance_factor: 2.5,
    impact_magnitude_base: 0.1,
    impact_magnitude_spread: 0.3,
};

/// The profile table (spec 014 section 3). Exhaustive by
/// construction: a class with no entry is a compile error here,
/// never a runtime default.
pub const fn profile(class: IceClass) -> &'static IceClassProfile {
    match class {
        IceClass::OpenWater => &OPEN_WATER,
        IceClass::PancakeIce => &PANCAKE_ICE,
        IceClass::PackIce => &PACK_ICE,
        IceClass::GlacialWall => &GLACIAL_WALL,
    }
}

/// The cell containing a world position.
pub fn cell_of(position: [f64; 2]) -> (i64, i64) {
    (
        (position[0] / CELL_UNITS).floor() as i64,
        (position[1] / CELL_UNITS).floor() as i64,
    )
}

/// The world position at a cell's center.
pub fn cell_center(cell: (i64, i64)) -> [f64; 2] {
    [
        (cell.0 as f64 + 0.5) * CELL_UNITS,
        (cell.1 as f64 + 0.5) * CELL_UNITS,
    ]
}

/// The class at a world position (spec 014 section 2).
pub fn class_at(seed: u64, position: [f64; 2]) -> IceClass {
    class_of_cell(seed, cell_of(position))
}

/// The class of a world cell: pure in (seed, cell), no other input.
pub fn class_of_cell(seed: u64, cell: (i64, i64)) -> IceClass {
    let border = border_distance(cell);
    if border < WALL_BAND {
        return IceClass::GlacialWall;
    }
    if border < PACK_BAND {
        return IceClass::PackIce;
    }
    // Pancake bands the pack; interior floes are the seed-hashed
    // scatter that makes seeds diverge.
    if border < PANCAKE_BAND || cell_hash(seed, cell) % 1000 < INTERIOR_FLOE_PER_MILLE {
        IceClass::PancakeIce
    } else {
        IceClass::OpenWater
    }
}

/// Cell distance to the nearest macro-region border. The region grid
/// is offset by half a region so the origin (a fresh run's spawn)
/// sits in a region interior, not inside a wall.
fn border_distance(cell: (i64, i64)) -> i64 {
    let axis = |c: i64| {
        let in_region = (c + REGION_CELLS / 2).rem_euclid(REGION_CELLS);
        in_region.min(REGION_CELLS - 1 - in_region)
    };
    axis(cell.0).min(axis(cell.1))
}

/// Seed-keyed cell hash (a splitmix64 finalizer over seed and
/// coordinates): the field's only source of variety, and never the
/// event RNG (spec 014 section 2, spec 010 section 4 rule 4).
fn cell_hash(seed: u64, cell: (i64, i64)) -> u64 {
    let mut x = seed
        ^ (cell.0 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (cell.1 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(seed: u64) -> Vec<IceClass> {
        let mut classes = Vec::new();
        for y in -48..48 {
            for x in -48..48 {
                classes.push(class_of_cell(seed, (x, y)));
            }
        }
        classes
    }

    /// Spec 014 section 5 test 1: same seed, byte-identical class
    /// answers over a sampled grid; different seeds diverge.
    #[test]
    fn field_is_pure_in_seed_and_position() {
        assert_eq!(sample(7), sample(7));
        assert_ne!(sample(7), sample(8), "different seeds never diverged");
    }

    /// Spec 014 section 2: glacial walls ring region borders, pack
    /// and pancake band around them, and the spawn cell is not
    /// walled in.
    #[test]
    fn walls_partition_and_bands_surround() {
        // The region grid is offset by half a region: borders sit at
        // cell coordinates congruent to REGION_CELLS/2.
        let border = REGION_CELLS / 2;
        assert_eq!(class_of_cell(3, (border, 0)), IceClass::GlacialWall);
        assert_eq!(class_of_cell(3, (border - 3, 0)), IceClass::PackIce);
        assert_eq!(class_of_cell(3, (border - 8, 0)), IceClass::PancakeIce);
        assert_ne!(class_of_cell(3, (0, 0)), IceClass::GlacialWall);
        // Every class appears somewhere in a sampled window.
        let classes = sample(3);
        for class in [
            IceClass::OpenWater,
            IceClass::PancakeIce,
            IceClass::PackIce,
            IceClass::GlacialWall,
        ] {
            assert!(classes.contains(&class), "{class:?} never appears");
        }
    }
}
