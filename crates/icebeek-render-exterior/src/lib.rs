//! The Macro view (spec 012 section 3): isometric 3D exterior.
//!
//! Visual systems arrive with the first playable; the plugin and the
//! interpolation contract land first so the boundary rules of spec 012
//! section 2 are in force from the first visual commit.

use bevy::prelude::*;

pub struct ExteriorRenderPlugin;

impl Plugin for ExteriorRenderPlugin {
    fn build(&self, _app: &mut App) {
        // Terrain, ship silhouette, weather, and expedition presentation
        // register here as they are built. Read-only over the sim.
    }
}

/// Interpolate a world position between the last two completed sim
/// ticks (spec 012 section 2 rule 3). `alpha` is the accumulator
/// fraction in [0, 1]; values outside are clamped: renderers never
/// extrapolate.
pub fn interpolate_position(prev: [f64; 2], curr: [f64; 2], alpha: f64) -> [f64; 2] {
    let alpha = alpha.clamp(0.0, 1.0);
    [
        prev[0] + (curr[0] - prev[0]) * alpha,
        prev[1] + (curr[1] - prev[1]) * alpha,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolation_endpoints_and_midpoint() {
        let prev = [0.0, -2.0];
        let curr = [10.0, 2.0];
        assert_eq!(interpolate_position(prev, curr, 0.0), prev);
        assert_eq!(interpolate_position(prev, curr, 1.0), curr);
        assert_eq!(interpolate_position(prev, curr, 0.5), [5.0, 0.0]);
    }

    #[test]
    fn interpolation_never_extrapolates() {
        let prev = [0.0, 0.0];
        let curr = [10.0, 10.0];
        assert_eq!(interpolate_position(prev, curr, -1.0), prev);
        assert_eq!(interpolate_position(prev, curr, 2.0), curr);
    }
}
