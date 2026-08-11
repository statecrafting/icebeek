//! The Micro view (spec 012 section 4): top-down cross-section interior.
//!
//! Visual systems arrive with the first playable; the plugin and the
//! interpolation contract land first so the boundary rules of spec 012
//! section 2 are in force from the first visual commit.

use bevy::prelude::*;

pub struct InteriorRenderPlugin;

impl Plugin for InteriorRenderPlugin {
    fn build(&self, _app: &mut App) {
        // Decks, rooms, the logistics spine, drones, and the heat overlay
        // register here as they are built. Read-only over the sim.
    }
}

/// Interpolate a scalar display value (a gauge, a heat cell) between
/// the last two completed sim ticks (spec 012 section 2 rule 3).
/// `alpha` outside [0, 1] is clamped: renderers never extrapolate.
pub fn interpolate_scalar(prev: f32, curr: f32, alpha: f32) -> f32 {
    let alpha = alpha.clamp(0.0, 1.0);
    prev + (curr - prev) * alpha
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolation_endpoints_and_midpoint() {
        assert_eq!(interpolate_scalar(1.0, 3.0, 0.0), 1.0);
        assert_eq!(interpolate_scalar(1.0, 3.0, 1.0), 3.0);
        assert_eq!(interpolate_scalar(1.0, 3.0, 0.5), 2.0);
    }

    #[test]
    fn interpolation_never_extrapolates() {
        assert_eq!(interpolate_scalar(1.0, 3.0, -0.5), 1.0);
        assert_eq!(interpolate_scalar(1.0, 3.0, 1.5), 3.0);
    }
}
