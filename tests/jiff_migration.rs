use amtrak_gtfs_rt::origin_departure;
use jiff::Timestamp;

#[test]
fn origin_departure_preserves_pacific_to_new_york_date_boundary() {
    // 23:30 Pacific is 02:30 New York the following day.
    let instant = origin_departure("11/15/2026 11:30:00 PM", 'P');
    let expected: Timestamp = "2026-11-16T07:30:00Z".parse().unwrap();
    assert_eq!(instant, expected);
}

#[test]
fn origin_departure_uses_later_instant_for_dst_folds() {
    // 01:30 occurs twice when New York exits daylight saving time.
    let instant = origin_departure("11/01/2026 1:30:00 AM", 'E');
    let expected: Timestamp = "2026-11-01T06:30:00Z".parse().unwrap();
    assert_eq!(instant, expected);
}
