//! Explicit, package-local IANA timezone data. Regenerate with tools/update-tzdata.py.
use jiff::tz::TimeZone;
use std::{collections::HashMap, sync::OnceLock};

pub fn get(name: &str) -> TimeZone {
    static ZONES: OnceLock<HashMap<&'static str, TimeZone>> = OnceLock::new();
    let zones = ZONES.get_or_init(|| {
        include_str!("tzdata/zones.json")
            .split('"')
            .filter(|name| name.starts_with("America/"))
            .map(|name| {
                (
                    name,
                    TimeZone::tzif(name, data(name)).expect("valid packaged IANA TZif"),
                )
            })
            .collect()
    });
    zones.get(name).expect("packaged timezone").clone()
}

fn data(name: &str) -> &'static [u8] {
    match name {
        "America/New_York" => include_bytes!("tzdata/America_New_York.tzif"),
        "America/Denver" => include_bytes!("tzdata/America_Denver.tzif"),
        "America/Los_Angeles" => include_bytes!("tzdata/America_Los_Angeles.tzif"),
        "America/Chicago" => include_bytes!("tzdata/America_Chicago.tzif"),
        "America/Phoenix" => include_bytes!("tzdata/America_Phoenix.tzif"),
        _ => panic!("timezone is not packaged: {name}"),
    }
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
    fn packaged_data_has_current_baseline() {
        assert!(include_str!("tzdata/version").trim() >= "2026f");
        for name in include_str!("tzdata/zones.json")
            .split('"')
            .filter(|s| s.starts_with("America/"))
        {
            assert_eq!(get(name).iana_name(), Some(name));
        }
    }
    #[test]
    fn daylight_saving_gap_and_fold() {
        let names: Vec<_> = include_str!("tzdata/zones.json")
            .split('"')
            .filter(|s| s.starts_with("America/"))
            .collect();
        let zone = get(names[0]);
        let gap = jiff::civil::date(2026, 3, 8).at(2, 30, 0, 0);
        let fold = jiff::civil::date(2026, 11, 1).at(1, 30, 0, 0);
        assert!(matches!(
            zone.to_ambiguous_timestamp(gap).offset(),
            jiff::tz::AmbiguousOffset::Gap { .. }
        ));
        let amb = zone.to_ambiguous_timestamp(fold);
        assert_eq!(
            amb.later().unwrap().as_second() - amb.earlier().unwrap().as_second(),
            3600
        );
    }
}
