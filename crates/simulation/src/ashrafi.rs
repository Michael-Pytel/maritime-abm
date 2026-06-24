//! Ashrafi (2024) environmental degradation of SAR performance.
//!
//! Month-conditioned severity (Barents Sea case study) degrades rescue-asset
//! transit/search speed and lengthens survivor boarding, so a rescue is hardest
//! in winter (Dec–Feb) and easiest in summer (May–Aug) — the seasonal pattern
//! reported by Ashrafi et al. (2024).
//!
//! The constants below are the seasonal month-group values of Ashrafi et al.
//! (2024) Table 4 (the aggregation their own results use), extracted from the
//! paper: speed multipliers relative to the mildest group, and absolute
//! boarding times per person. The raw metocean severity thresholds `x_l`/`x_u`
//! are expert-assigned and not tabulated in the source, so we adopt the paper's
//! month-group aggregation directly rather than re-deriving them. No no-go state
//! is fabricated (the paper quantifies degradation, not suspension).

use crate::sar::RescueKind;

/// Month groups from Ashrafi (2024), ordered by total rescue time (best → worst).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonthGroup {
    /// May–Aug — mildest conditions, shortest rescue times.
    A,
    /// Apr, Sep.
    B,
    /// Mar, Oct, Nov.
    C,
    /// Jan, Feb, Dec — harshest conditions, longest rescue times.
    D,
}

impl MonthGroup {
    /// Map a calendar month (1–12) to its Ashrafi severity group.
    #[must_use]
    pub fn from_month(month: u8) -> Self {
        match month {
            5..=8 => MonthGroup::A,
            4 | 9 => MonthGroup::B,
            3 | 10 | 11 => MonthGroup::C,
            _ => MonthGroup::D, // Jan, Feb, Dec (and any out-of-range)
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            MonthGroup::A => "A",
            MonthGroup::B => "B",
            MonthGroup::C => "C",
            MonthGroup::D => "D",
        }
    }

    /// Transit / search speed multiplier relative to the mildest group
    /// (Ashrafi 2024 Table 4 speeds normalised to Group A, per asset type).
    #[must_use]
    pub fn speed_multiplier(self, kind: RescueKind) -> f64 {
        match (kind, self) {
            // Mildest group is the nominal speed for both asset types.
            (_, MonthGroup::A) => 1.00,
            // Vessel: 39.70 / 19.78 / 10.19 km/h ÷ 49.60.
            (RescueKind::Patrol, MonthGroup::B) => 0.80,
            (RescueKind::Patrol, MonthGroup::C) => 0.399,
            (RescueKind::Patrol, MonthGroup::D) => 0.205,
            // Helicopter: 270.22 / 219.79 / 170.13 km/h ÷ 285.20.
            (RescueKind::Helicopter, MonthGroup::B) => 0.947,
            (RescueKind::Helicopter, MonthGroup::C) => 0.771,
            (RescueKind::Helicopter, MonthGroup::D) => 0.597,
        }
    }

    /// Survivor boarding time per person (minutes) — Ashrafi 2024 Table 4.
    #[must_use]
    pub fn boarding_min_per_person(self, kind: RescueKind) -> f64 {
        match (kind, self) {
            (RescueKind::Patrol, MonthGroup::A) => 6.10,
            (RescueKind::Patrol, MonthGroup::B) => 9.20,
            (RescueKind::Patrol, MonthGroup::C) => 15.50,
            (RescueKind::Patrol, MonthGroup::D) => 31.20,
            (RescueKind::Helicopter, MonthGroup::A) => 2.05,
            (RescueKind::Helicopter, MonthGroup::B) => 3.10,
            (RescueKind::Helicopter, MonthGroup::C) => 5.25,
            (RescueKind::Helicopter, MonthGroup::D) => 10.65,
        }
    }
}

/// Wind-chill index (Shaykewich 2002; Ashrafi 2024 Eq. 2).
///
/// `temp_c` is air temperature (°C) and `wind_kmh` the wind speed at 10 m
/// (km/h). Uses the canonical Environment-Canada coefficient 0.6215 (the paper
/// prints 0.612).
#[must_use]
pub fn wind_chill_index(temp_c: f64, wind_kmh: f64) -> f64 {
    let w016 = wind_kmh.powf(0.16);
    13.12 + 0.6215 * temp_c - 11.37 * w016 + 0.3965 * temp_c * w016
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_grouping_matches_paper() {
        assert_eq!(MonthGroup::from_month(7), MonthGroup::A); // July, mildest
        assert_eq!(MonthGroup::from_month(4), MonthGroup::B);
        assert_eq!(MonthGroup::from_month(11), MonthGroup::C);
        assert_eq!(MonthGroup::from_month(1), MonthGroup::D); // January, harshest
        assert_eq!(MonthGroup::from_month(12), MonthGroup::D);
    }

    #[test]
    fn winter_degrades_speed_and_lengthens_boarding() {
        let summer = MonthGroup::A;
        let winter = MonthGroup::D;
        for kind in [RescueKind::Patrol, RescueKind::Helicopter] {
            assert!(summer.speed_multiplier(kind) > winter.speed_multiplier(kind));
            assert!(winter.boarding_min_per_person(kind) > summer.boarding_min_per_person(kind));
        }
        // Mildest group is the nominal (no speed penalty).
        assert!((summer.speed_multiplier(RescueKind::Helicopter) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn wind_chill_is_colder_than_air_in_wind() {
        // −10 °C with a 30 km/h wind feels colder than −10 °C still air.
        let still = wind_chill_index(-10.0, 0.0);
        let windy = wind_chill_index(-10.0, 30.0);
        assert!(windy < still);
    }
}
