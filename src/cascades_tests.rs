use std::sync::Arc;

use gtfs_structures::{CalendarDate, Exception, Stop, StopTime, Trip};
use serde_json::json;

use super::*;

#[test]
fn cascades_cross_border_trains_match_the_november_2026_timetable() {
    // Amtrak Cascades daily train schedule effective November 1, 2026.
    // The GTFS fixture expresses departures on the feed's Eastern clock.
    let service_date = "2026-11-24".parse().unwrap();
    let mut gtfs = Gtfs::default();
    gtfs.calendar_dates.insert(
        "daily".into(),
        vec![CalendarDate {
            service_id: "daily".into(),
            date: service_date,
            exception_type: Exception::Added,
        }],
    );
    for (
        number,
        origin,
        origin_tz,
        local_departure,
        gtfs_departure,
        destination,
        dest_tz,
        local_arrival,
        expected_departure,
        expected_arrival,
    ) in [
        (
            "516",
            "SEA",
            "P",
            "8:30:00 AM",
            (11, 30),
            "VAC",
            "M",
            "11/24/2026 13:30:00",
            "2026-11-24T16:30:00Z",
            "2026-11-24T20:30:00Z",
        ),
        (
            "517",
            "VAC",
            "M",
            "8:25:00 AM",
            (10, 25),
            "SEA",
            "P",
            "11/24/2026 11:40:00",
            "2026-11-24T15:25:00Z",
            "2026-11-24T19:40:00Z",
        ),
        (
            "518",
            "SEA",
            "P",
            "6:00:00 PM",
            (21, 0),
            "VAC",
            "M",
            "11/24/2026 23:00:00",
            "2026-11-25T02:00:00Z",
            "2026-11-25T06:00:00Z",
        ),
        (
            "519",
            "VAC",
            "P",
            "5:55:00 PM",
            (19, 55),
            "SEA",
            "P",
            "11/24/2026 21:10:00",
            "2026-11-25T00:55:00Z",
            "2026-11-25T05:10:00Z",
        ),
    ] {
        let trip_id = format!("cascades-{number}");
        gtfs.trips.insert(
            trip_id.clone(),
            Trip {
                id: trip_id.clone(),
                service_id: "daily".into(),
                trip_short_name: Some(number.into()),
                stop_times: vec![StopTime {
                    stop: Arc::new(Stop {
                        id: origin.into(),
                        ..Default::default()
                    }),
                    departure_time: Some(gtfs_departure.0 * 3600 + gtfs_departure.1 * 60),
                    ..Default::default()
                }],
                ..Default::default()
            },
        );
        let origin_time = format!("11/24/2026 {local_departure}");
        let origin_zone = station_timezone(origin, origin_tz.chars().next().unwrap());
        assert_eq!(
            origin_departure_in_zone(&origin_time, &origin_zone),
            expected_departure.parse::<Timestamp>().unwrap(),
            "train {number} origin",
        );

        let stop = json!({
            "code": destination,
            "tz": dest_tz,
            "bus": false,
            "schcmnt": "",
            "autoarr": false,
            "autodep": false,
            "estarr": local_arrival,
        });
        let feature = serde_json::from_value(json!({
            "type": "Feature",
            "geometry": { "type": "Point", "coordinates": [-122.0, 48.0] },
            "properties": {
                "TrainNum": number,
                "OrigCode": origin,
                "OriginTZ": origin_tz,
                "OrigSchDep": origin_time,
                "Station0": stop.to_string(),
            },
        }))
        .unwrap();
        let entity = feature_to_gtfs_unified(&gtfs, &feature, None, &radar::Locations::default());
        let update = entity.trip_update.unwrap();
        assert_eq!(update.trip.trip_id.as_deref(), Some(trip_id.as_str()));
        assert_eq!(update.trip.start_date.as_deref(), Some("20261124"));
        assert_eq!(
            update.stop_time_update[0].arrival.as_ref().unwrap().time,
            Some(expected_arrival.parse::<Timestamp>().unwrap().as_second()),
            "train {number} destination",
        );
    }
}

#[test]
fn vancouver_bc_and_washington_follow_their_own_clock_changes() {
    let vancouver_bc = station_timezone("VAC", 'M');
    let vancouver_wa = station_timezone("VAN", 'P');
    for (date, bc_utc_hour, wa_utc_hour) in [
        ("11/16/2025", 16, 16),
        ("07/16/2026", 15, 15),
        ("11/16/2026", 15, 16),
    ] {
        let text = format!("{date} 8:25:00 AM");
        let bc = origin_departure_in_zone(&text, &vancouver_bc);
        let wa = origin_departure_in_zone(&text, &vancouver_wa);
        assert_eq!(jiff::tz::TimeZone::UTC.to_datetime(bc).hour(), bc_utc_hour);
        assert_eq!(jiff::tz::TimeZone::UTC.to_datetime(wa).hour(), wa_utc_hour);
    }
}
