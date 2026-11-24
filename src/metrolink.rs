//! Optional position overrides from Metrolink's public PTC train list.
//! Amtrak remains authoritative for trip identity, stop times, and alerts.
use std::{collections::HashMap, time::Duration};

use gtfs_realtime::Position;
use jiff::{Timestamp, civil::DateTime, tz::AmbiguousOffset};
use serde::Deserialize;

const TRAIN_LIST_URL: &str = "https://rtt.metrolinktrains.com/trainlist.json";
const MAX_AGE_SECONDS: u64 = 5 * 60;
const MAX_FUTURE_SKEW_SECONDS: u64 = 60;
const MAX_PREDEPARTURE_SECONDS: i64 = 2 * 3600;
const MAX_LOCAL_RUN_SECONDS: i64 = 24 * 3600;
const MAX_LONG_DISTANCE_TIMESTAMP_DIFFERENCE: u64 = 30 * 60;
const MAX_POSITION_DIFFERENCE_KM: f64 = 100.0;
const MPS_PER_MPH: f32 = 0.44704;

#[derive(Deserialize)]
struct Train {
    symbol: String,
    lat: String,
    long: String,
    ptc_time: String,
    ptc_status: String,
    speed: Option<String>,
    direction: Option<String>,
}

#[derive(Default)]
pub(crate) struct Locations(HashMap<u32, (Position, u64)>);

// Amtrak symbols are A + train number + optional cardinal suffix (e.g. A579S).
// Never extract digits from ordinary Metrolink commuter or freight symbols.
fn amtrak_train_number(symbol: &str) -> Option<u32> {
    let number = symbol.trim().strip_prefix('A')?;
    let number = number
        .strip_suffix(['N', 'S', 'E', 'W'])
        .unwrap_or(number);
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    number.parse::<u32>().ok().filter(|number| *number != 0)
}

// Apply the degree sign to the entire D:M:S value (including -0 degrees).
fn decimal_degrees(text: &str, maximum: f64) -> Option<f32> {
    let mut parts = text.trim().split(':');
    let degrees_text = parts.next()?;
    let degrees: i32 = degrees_text.parse().ok()?;
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    if parts.next().is_some()
        || !(0.0..60.0).contains(&minutes)
        || !(0.0..60.0).contains(&seconds)
    {
        return None;
    }
    let magnitude = f64::from(degrees).abs() + minutes / 60.0 + seconds / 3600.0;
    if magnitude > maximum {
        return None;
    }
    Some(if degrees_text.starts_with('-') {
        -magnitude as f32
    } else {
        magnitude as f32
    })
}

fn is_fresh(timestamp: u64, now: u64) -> bool {
    timestamp >= now.saturating_sub(MAX_AGE_SECONDS)
        && timestamp <= now.saturating_add(MAX_FUTURE_SKEW_SECONDS)
}

// PTC reports a local California wall clock, not Amtrak's Eastern clock.
// During the fall-back fold, choose the occurrence that is actually fresh.
fn observation_timestamp(text: &str, now: u64) -> Option<u64> {
    let dt = DateTime::strptime("%m/%d/%Y %l:%M:%S %p", text.trim()).ok()?;
    let zone = crate::timezone::get("America/Los_Angeles");
    let ambiguous = zone.to_ambiguous_timestamp(dt);
    if matches!(ambiguous.offset(), AmbiguousOffset::Gap { .. }) {
        return None;
    }
    [ambiguous.later().ok(), ambiguous.earlier().ok()]
        .into_iter()
        .flatten()
        .filter_map(|timestamp| u64::try_from(timestamp.as_second()).ok())
        .find(|&timestamp| is_fresh(timestamp, now))
}

// This is a coarse reported direction, not a measured GPS course.
fn bearing(direction: &str) -> Option<f32> {
    match direction.trim().to_ascii_uppercase().as_str() {
        "NORTH" | "N" => Some(0.0),
        "NORTHEAST" | "NE" => Some(45.0),
        "EAST" | "E" => Some(90.0),
        "SOUTHEAST" | "SE" => Some(135.0),
        "SOUTH" | "S" => Some(180.0),
        "SOUTHWEST" | "SW" => Some(225.0),
        "WEST" | "W" => Some(270.0),
        "NORTHWEST" | "NW" => Some(315.0),
        _ => None,
    }
}

fn distance_km(a: &Position, b: &Position) -> Option<f64> {
    let valid = |position: &Position| {
        (-90.0..=90.0).contains(&position.latitude)
            && (-180.0..=180.0).contains(&position.longitude)
    };
    if !valid(a) || !valid(b) {
        return None;
    }
    let lat_a = f64::from(a.latitude).to_radians();
    let lat_b = f64::from(b.latitude).to_radians();
    let lon_delta = (f64::from(b.longitude) - f64::from(a.longitude)).to_radians();
    let haversine = ((lat_b - lat_a) / 2.0).sin().powi(2)
        + lat_a.cos() * lat_b.cos() * (lon_delta / 2.0).sin().powi(2);
    Some(6371.0 * 2.0 * haversine.clamp(0.0, 1.0).sqrt().asin())
}

impl Locations {
    fn from_rows(rows: Vec<serde_json::Value>, now: u64) -> Self {
        let mut positions = HashMap::new();
        for row in rows {
            // A malformed record must not discard other trains in the list.
            let Ok(train) = serde_json::from_value::<Train>(row) else {
                continue;
            };
            if !train.ptc_status.trim().eq_ignore_ascii_case("GOOD") {
                continue;
            }
            let (Some(number), Some(latitude), Some(longitude), Some(timestamp)) = (
                amtrak_train_number(&train.symbol),
                decimal_degrees(&train.lat, 90.0),
                decimal_degrees(&train.long, 180.0),
                observation_timestamp(&train.ptc_time, now),
            ) else {
                continue;
            };
            if latitude == 0.0 && longitude == 0.0 {
                continue;
            }
            let position = Position {
                latitude,
                longitude,
                bearing: train.direction.as_deref().and_then(bearing),
                odometer: None,
                speed: train.speed.as_deref()
                    .and_then(|speed| speed.trim().parse::<f32>().ok())
                    .filter(|speed| speed.is_finite() && *speed >= 0.0)
                    .map(|mph| mph * MPS_PER_MPH),
            };
            let best = positions.entry(number).or_insert((position, timestamp));
            if timestamp > best.1 {
                *best = (position, timestamp);
            }
        }
        Self(positions)
    }

    pub(crate) fn position(
        &self,
        train_number: &str,
        origin: Timestamp,
        route_name: Option<&str>,
        amtrak_timestamp: Option<u64>,
        amtrak_position: &Position,
    ) -> Option<(Position, u64)> {
        let (position, observed) = self.0.get(&train_number.parse().ok()?).copied()?;
        let elapsed = i64::try_from(observed).ok()?.checked_sub(origin.as_second())?;
        // The list has no service date. Reject observations before this run
        // (allow a train to be staged up to two hours before departure).
        if elapsed < -MAX_PREDEPARTURE_SECONDS
            || distance_km(&position, amtrak_position)? > MAX_POSITION_DIFFERENCE_KM
        {
            return None;
        }
        // A previous day's Surfliner must not receive today's train position.
        // For multi-day Amtrak runs, additionally require contemporaneous
        // Amtrak data near the PTC fix to distinguish repeated train numbers.
        if elapsed >= MAX_LOCAL_RUN_SECONDS
            && (route_name.is_none_or(|name| name.eq_ignore_ascii_case("Pacific Surfliner"))
                || amtrak_timestamp.is_none_or(|timestamp| {
                    timestamp.abs_diff(observed) > MAX_LONG_DISTANCE_TIMESTAMP_DIFFERENCE
                }))
        {
            return None;
        }
        Some((position, observed))
    }
}

pub(crate) async fn fetch(client: &reqwest::Client) -> Locations {
    let result = async {
        client
            .get(TRAIN_LIST_URL)
            .timeout(Duration::from_secs(5))
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<serde_json::Value>>()
            .await
    }
    .await;

    match result {
        Ok(rows) => Locations::from_rows(rows, Timestamp::now().as_second() as u64),
        Err(error) => {
            eprintln!("Error fetching Metrolink PTC positions, skipping Metrolink overrides: {error}");
            Locations::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn unix(text: &str) -> u64 {
        text.parse::<Timestamp>().unwrap().as_second() as u64
    }

    fn observation() -> Value {
        json!({
            "symbol": "A579S",
            "direction": "WEST",
            "lat": "33:59:25.93",
            "long": "-118:8:34.37",
            "speed": "59",
            "line": "PAC SURF",
            "ptc_time": "11/24/2026 3:50:01 PM",
            "ptc_status": "GOOD",
            "delay_status": "ON TIME"
        })
    }

    fn origin() -> Timestamp {
        "2026-11-24T21:00:00Z".parse().unwrap()
    }

    fn amtrak_position() -> Position {
        Position {
            latitude: 33.98,
            longitude: -118.13,
            speed: Some(20.0),
            bearing: Some(225.0),
            odometer: None,
        }
    }

    #[test]
    fn decodes_sample_and_preserves_amtrak_trip_updates() {
        let observed = unix("2026-11-24T23:50:01Z");
        let locations = Locations::from_rows(vec![observation()], observed + 20);
        let sources = crate::radar::Locations::default().with_metrolink(locations);
        let feature = serde_json::from_value(json!({
            "type": "Feature",
            "geometry": {"type": "Point", "coordinates": [-118.13, 33.98]},
            "properties": {
                "TrainNum": "0579",
                "RouteName": "Pacific Surfliner",
                "OriginTZ": "P",
                "OrigSchDep": "11/24/2026 1:00:00 PM",
                "Velocity": "60",
                "Heading": "SW",
                "updated_at": "11/24/2026 6:50:10 PM"
            }
        })).unwrap();
        let gtfs = gtfs_structures::Gtfs::default();
        let baseline = crate::feature_to_gtfs_unified(
            &gtfs, &feature, None, &crate::radar::Locations::default(),
        );
        let actual = crate::feature_to_gtfs_unified(&gtfs, &feature, None, &sources);
        assert_eq!(actual.id, baseline.id);
        assert_eq!(actual.trip_update, baseline.trip_update);
        assert_eq!(actual.alert, baseline.alert);
        let vehicle = actual.vehicle.unwrap();
        assert_eq!(vehicle.trip, baseline.vehicle.unwrap().trip);
        assert_eq!(vehicle.timestamp, Some(observed));
        let position = vehicle.position.unwrap();
        assert!((position.latitude - 33.990536).abs() < 0.00001);
        assert!((position.longitude - -118.14288).abs() < 0.00001);
        assert!((position.speed.unwrap() - 26.37536).abs() < 0.0001);
        assert_eq!(position.bearing, Some(270.0));
    }

    #[test]
    fn rejects_invalid_stale_future_and_non_amtrak_rows() {
        let now = unix("2026-11-24T23:50:01Z");
        let mut commuter = observation();
        commuter["symbol"] = json!("M579S");
        let mut stale = observation();
        stale["ptc_time"] = json!("11/24/2026 3:45:00 PM");
        let mut future = observation();
        future["ptc_time"] = json!("11/24/2026 3:51:02 PM");
        let mut invalid = observation();
        invalid["long"] = json!("-118:60:34.37");
        let mut bad_ptc = observation();
        bad_ptc["ptc_status"] = json!("BAD");
        let invalid_rows = vec![commuter, stale, future, invalid, bad_ptc, json!({})];
        assert!(Locations::from_rows(invalid_rows, now).0.is_empty());
        assert_eq!(amtrak_train_number("A0579S"), Some(579));
        assert_eq!(amtrak_train_number("A579X"), None);
        assert_eq!(amtrak_train_number("M579S"), None);
        assert_eq!(amtrak_train_number("579"), None);
        assert_eq!(decimal_degrees("-0:30:00", 180.0), Some(-0.5));
        assert!(decimal_degrees("90:00:01", 90.0).is_none());
        assert!(decimal_degrees("33:00:NaN", 90.0).is_none());
    }

    #[test]
    fn latest_valid_duplicate_wins_and_missing_fields_remain_unset() {
        let now = unix("2026-11-24T23:50:01Z");
        let mut older = observation();
        older["ptc_time"] = json!("11/24/2026 3:49:00 PM");
        let mut latest = observation();
        latest["speed"] = json!("");
        latest["direction"] = json!("UNKNOWN");
        let locations = Locations::from_rows(vec![latest, older], now);
        let (position, timestamp) = locations.0.get(&579).unwrap();
        assert_eq!(*timestamp, now);
        assert_eq!(position.speed, None);
        assert_eq!(position.bearing, None);
    }

    #[test]
    fn timezone_handles_pacific_summer_and_both_fall_back_occurrences() {
        let summer = unix("2026-07-01T22:50:01Z");
        assert_eq!(observation_timestamp("7/1/2026 3:50:01 PM", summer), Some(summer));
        for instant in ["2026-11-01T08:30:00Z", "2026-11-01T09:30:00Z"] {
            let now = unix(instant);
            assert_eq!(observation_timestamp("11/1/2026 1:30:00 AM", now), Some(now));
        }
        assert_eq!(
            observation_timestamp("3/8/2026 2:30:00 AM", unix("2026-03-08T10:30:00Z")),
            None
        );
    }

    #[test]
    fn matches_the_current_run_instead_of_relying_on_eastern_calendar_date() {
        let observed = unix("2026-11-25T05:50:01Z");
        let mut evening = observation();
        evening["ptc_time"] = json!("11/24/2026 9:50:01 PM");
        let locations = Locations::from_rows(vec![evening], observed);
        let near = amtrak_position();
        let route = Some("Pacific Surfliner");
        assert!(locations.position("0579", origin(), route, None, &near).is_some());
        assert!(locations.position("580", origin(), route, None, &near).is_none());
        let previous_day = "2026-11-23T21:00:00Z".parse().unwrap();
        assert!(locations.position("579", previous_day, route, Some(observed), &near).is_none());
        let tomorrow = "2026-11-25T21:00:00Z".parse().unwrap();
        assert!(locations.position("579", tomorrow, route, None, &near).is_none());
        let far = Position { latitude: 40.0, longitude: -73.0, ..near };
        assert!(locations.position("579", origin(), route, None, &far).is_none());
    }

    #[test]
    fn multi_day_train_requires_nearby_contemporaneous_amtrak_observation() {
        let observed = unix("2026-11-24T23:50:01Z");
        let locations = Locations::from_rows(vec![observation()], observed);
        let previous_day = "2026-11-23T21:00:00Z".parse().unwrap();
        let near = amtrak_position();
        let route = Some("Southwest Chief");
        assert!(locations.position("579", previous_day, route, Some(observed), &near).is_some());
        assert!(locations.position("579", previous_day, route, Some(observed - 1801), &near).is_none());
        assert!(locations.position("579", previous_day, route, None, &near).is_none());
    }
}
