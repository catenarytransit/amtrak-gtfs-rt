//! Time zone resolution through Jiff's bundled IANA time zone database.
//! The `tzdb-bundle-always` feature keeps this working without system zoneinfo.
use std::sync::LazyLock;

use jiff::tz::{TimeZone, TimeZoneDatabase};

static DATABASE: LazyLock<TimeZoneDatabase> = LazyLock::new(TimeZoneDatabase::bundled);

pub fn get(name: &str) -> TimeZone {
    DATABASE.get(name).expect("known IANA timezone")
}

/// Preserve Chrono's latest(): later instant for folds, no instant for gaps.
pub fn latest_timestamp(zone: &TimeZone, dt: jiff::civil::DateTime) -> Option<jiff::Timestamp> {
    let ambiguous = zone.to_ambiguous_timestamp(dt);
    if matches!(ambiguous.offset(), jiff::tz::AmbiguousOffset::Gap { .. }) {
        return None;
    }
    ambiguous.later().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_timezones_are_available() {
        for name in [
            "America/New_York",
            "America/Denver",
            "America/Los_Angeles",
            "America/Vancouver",
            "America/Chicago",
            "America/Phoenix",
        ] {
            assert_eq!(get(name).iana_name(), Some(name));
        }
    }

    #[test]
    fn bundled_release_matches_latest_iana_when_requested() {
        if let Ok(latest) = std::env::var("LATEST_IANA_TZDB_VERSION") {
            assert_eq!(jiff_tzdb::VERSION, latest.trim());
        }
    }

    #[test]
    fn daylight_saving_gap_and_fold() {
        let zone = get("America/New_York");
        let gap = jiff::civil::date(2026, 3, 8).at(2, 30, 0, 0);
        let fold = jiff::civil::date(2026, 11, 1).at(1, 30, 0, 0);
        assert!(matches!(
            zone.to_ambiguous_timestamp(gap).offset(),
            jiff::tz::AmbiguousOffset::Gap { .. }
        ));
        assert!(latest_timestamp(&zone, gap).is_none());
        let amb = zone.to_ambiguous_timestamp(fold);
        assert_eq!(
            amb.later().unwrap().as_second() - amb.earlier().unwrap().as_second(),
            3600
        );
        assert_eq!(latest_timestamp(&zone, fold), amb.later().ok());
    }
}
