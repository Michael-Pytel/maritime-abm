//! Simulation clock resolution.
//!
//! One tick is five simulated minutes. Scenario horizons (14 d Calm exposure,
//! ~4 d Storm passage, 7 d Blind/Deep) are expressed in ticks via the helpers
//! below so physical rates stay consistent when Δt changes.

/// Simulated hours represented by one engine tick (5 minutes).
pub const TICK_HOURS: f64 = 1.0 / 12.0;

/// Ticks in one simulated hour.
pub const TICKS_PER_HOUR: f64 = 12.0;

/// Ticks in one simulated day (24 × 12).
pub const TICKS_PER_DAY: u32 = 288;

/// Distance covered in one tick at `speed_kn` knots (nm).
#[must_use]
pub fn nm_per_tick(speed_kn: f64) -> f64 {
    speed_kn * TICK_HOURS
}

/// Round `hours` of simulated time to an integer tick count.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn ticks_for_hours(hours: f64) -> u32 {
    (hours * TICKS_PER_HOUR)
        .round()
        .clamp(0.0, f64::from(u32::MAX)) as u32
}

/// Round `days` of simulated time to an integer tick count.
#[must_use]
pub fn ticks_for_days(days: f64) -> u32 {
    ticks_for_hours(days * 24.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_minute_tick_basics() {
        assert!((TICK_HOURS - 1.0 / 12.0).abs() < 1e-12);
        assert_eq!(ticks_for_days(1.0), TICKS_PER_DAY);
        assert_eq!(ticks_for_days(14.0), 4032);
        assert_eq!(ticks_for_days(4.0), 1152);
        assert_eq!(ticks_for_days(7.0), 2016);
        assert!((nm_per_tick(12.0) - 1.0).abs() < 1e-12);
    }
}
