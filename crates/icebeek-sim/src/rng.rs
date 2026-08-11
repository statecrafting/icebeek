//! The one seeded RNG (spec 010 section 4 rule 4).
//!
//! xoshiro256++ with a SplitMix64 seeder: deterministic, serializable,
//! dependency-free. Only world-phase event-generation systems may draw
//! from it; interior systems have no randomness at all. The algorithm
//! is implementation latitude; changing it is a save-breaking change.

use bevy_ecs::prelude::Resource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Resource)]
pub struct SimRng {
    s: [u64; 4],
}

impl SimRng {
    pub fn from_seed(seed: u64) -> Self {
        // SplitMix64 expansion of the u64 seed into the full state, the
        // reference-recommended seeding for xoshiro generators.
        let mut x = seed;
        let mut next = || {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        Self {
            s: [next(), next(), next(), next()],
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let [s0, s1, s2, s3] = self.s;
        let result = s0.wrapping_add(s3).rotate_left(23).wrapping_add(s0);
        let t = s1 << 17;
        let mut s = [s0, s1, s2, s3];
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        self.s = s;
        result
    }

    /// Uniform in [0, 1) with 24 bits of precision.
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u32 << 24) as f32
    }

    /// Uniform in [0, n). Modulo bias is negligible for gameplay-scale n
    /// and irrelevant to determinism.
    pub fn next_range(&mut self, n: u32) -> u32 {
        (self.next_u64() % u64::from(n)) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = SimRng::from_seed(7);
        let mut b = SimRng::from_seed(7);
        for _ in 0..64 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn state_round_trips_mid_stream() {
        let mut a = SimRng::from_seed(99);
        for _ in 0..10 {
            a.next_u64();
        }
        let encoded = serde_json::to_string(&a).expect("serialize");
        let mut b: SimRng = serde_json::from_str(&encoded).expect("deserialize");
        for _ in 0..64 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }
}
