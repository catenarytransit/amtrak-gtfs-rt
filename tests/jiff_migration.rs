use amtrak_gtfs_rt::origin_departure;
use jiff::{civil::date, tz::TimeZone, Timestamp};

#[test]
fn origin_departure_preserves_local_weekday_and_new_york_service_date() {
    // Late Pacific departures fall on the following date in New York.
    let instant = origin_departure("11/15/2026 11:30:00 PM", 'P');
    let pacific = TimeZone::get("America/Los_Angeles").unwrap();
    let new_york = TimeZone::get("America/New_York").unwrap();

    assert_eq!(pacific.to_datetime(instant).date(), date(2026, 11, 15));
    assert_eq!(new_york.to_datetime(instant).date(), date(2026, 11, 16));
    assert_eq!(pacific.to_datetime(instant).weekday(), jiff::civil::Weekday::Sunday);
}

#[test]
fn origin_departure_uses_later_instant_for_dst_folds() {
    // 01:30 occurs twice when New York exits daylight saving time.
    let instant = origin_departure("11/01/2026 1:30:00 AM", 'E');
    let expected: Timestamp = "2026-11-01T06:30:00Z".parse().unwrap();
    assert_eq!(instant, expected);
}
