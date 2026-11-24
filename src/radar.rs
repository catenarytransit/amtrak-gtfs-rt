use std::{collections::HashMap, time::Duration};

use gtfs_realtime::Position;
use jiff::{Timestamp, civil::Date};
use serde::Deserialize;

const LOCATIONS_URL: &str =
    "https://backend-unified.mylirr.org/locations?geometry=TRACK_TURF&railroad=BOTH";
const MAX_LOCATION_AGE_SECONDS: u64 = 5 * 60;
const MAX_FUTURE_SKEW_SECONDS: u64 = 60;
const METERS_PER_SECOND_PER_MPH: f32 = 0.44704;

#[derive(Deserialize)]
struct Train {
    train_num: String,
    run_date: String,
    realtime: bool,
    details: TrainDetails,
    status: TrainStatus,
    location: Option<Location>,
}

#[derive(Deserialize)]
struct TrainDetails {
    branch: Option<String>,
}

#[derive(Deserialize)]
struct TrainStatus {
    canceled: bool,
}

#[derive(Deserialize)]
struct Location {
    latitude: f32,
    longitude: f32,
    speed: Option<f32>,
    heading: Option<f32>,
    timestamp: Option<u64>,
}

#[derive(Default)]
pub(crate) struct Locations {
    mta: HashMap<(Date, u32), (Position, u64)>,
    metrolink: crate::metrolink::Locations,
}

impl Locations {
    fn from_trains(trains: Vec<Train>, now: u64) -> Self {
        let mut positions = HashMap::new();
        for train in trains {
            if !train.realtime
                || train.status.canceled
                || !train
                    .details
                    .branch
                    .as_deref()
                    .is_some_and(|branch| branch.eq_ignore_ascii_case("Amtrak"))
            {
                continue;
            }
            let (Ok(run_date), Ok(train_number), Some(location)) = (
                train.run_date.parse::<Date>(),
                train.train_num.parse::<u32>(),
                train.location,
            ) else {
                continue;
            };
            let Some(timestamp) = location.timestamp else {
                continue;
            };
            if timestamp < now.saturating_sub(MAX_LOCATION_AGE_SECONDS)
                || timestamp > now.saturating_add(MAX_FUTURE_SKEW_SECONDS)
                || !(-90.0..=90.0).contains(&location.latitude)
                || !(-180.0..=180.0).contains(&location.longitude)
            {
                continue;
            }

            let position = Position {
                latitude: location.latitude,
                longitude: location.longitude,
                bearing: location.heading,
                odometer: None,
                speed: location.speed.map(|mph| mph * METERS_PER_SECOND_PER_MPH),
            };
            let best = positions
                .entry((run_date, train_number))
                .or_insert((position, timestamp));
            if timestamp > best.1 {
                *best = (position, timestamp);
            }
        }
        Self {
            mta: positions,
            ..Self::default()
        }
    }

    pub(crate) fn position(&self, run_date: Date, train_number: &str) -> Option<(Position, u64)> {
        self.mta.get(&(run_date, train_number.parse().ok()?)).copied()
    }

    pub(crate) fn with_metrolink(mut self, metrolink: crate::metrolink::Locations) -> Self {
        self.metrolink = metrolink;
        self
    }

    pub(crate) fn metrolink_position(
        &self,
        train_number: &str,
        origin: Timestamp,
        route_name: Option<&str>,
        amtrak_timestamp: Option<u64>,
        amtrak_position: &Position,
    ) -> Option<(Position, u64)> {
        self.metrolink.position(train_number, origin, route_name, amtrak_timestamp, amtrak_position)
    }
}

pub(crate) async fn fetch(client: &reqwest::Client) -> Locations {
    let (mta, metrolink) = futures::join!(fetch_mta(client), crate::metrolink::fetch(client));
    mta.with_metrolink(metrolink)
}

async fn fetch_mta(client: &reqwest::Client) -> Locations {
    let result = async {
        client
            .get(LOCATIONS_URL)
            .header("accept-version", "3.0")
            .header("origin", "https://radar.mta.info")
            .timeout(Duration::from_secs(5))
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<Train>>()
            .await
    }
    .await;

    match result {
        Ok(trains) => Locations::from_trains(trains, Timestamp::now().as_second() as u64),
        Err(error) => {
            eprintln!("Error fetching MTA Radar positions, using Amtrak positions: {error}");
            Locations::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const OBSERVED_AT: u64 = 1795562686;

    fn radar_train() -> Value {
        json!({
            "train_id": "MNR_2026-11-24_2171",
            "railroad": "MNR",
            "run_date": "2026-11-24",
            "train_num": "2171",
            "realtime": true,
            "details": {"headsign": "AMTRAK 2171", "branch": "Amtrak"},
            "location": {
                "latitude": 41.021117,
                "longitude": -73.625574,
                "source": "SIGNAL",
                "timestamp": OBSERVED_AT
            },
            "status": {"canceled": false}
        })
    }

    #[test]
    fn radar_overrides_only_the_matched_amtrak_vehicle_observation() {
        let locations = Locations::from_trains(
            vec![serde_json::from_value(radar_train()).unwrap()],
            OBSERVED_AT + 20,
        );
        let feature = serde_json::from_value(json!({
            "type": "Feature",
            "geometry": {"type": "Point", "coordinates": [-73.63, 41.01]},
            "properties": {
                "TrainNum": "02171",
                "OriginTZ": "E",
                "OrigSchDep": "11/24/2026 3:00:00 PM",
                "Velocity": "60",
                "Heading": "SW",
                "updated_at": "11/24/2026 5:03:00 PM"
            }
        }))
        .unwrap();
        let gtfs = gtfs_structures::Gtfs::default();
        let baseline = crate::feature_to_gtfs_unified(&gtfs, &feature, None, &Locations::default());
        let actual = crate::feature_to_gtfs_unified(&gtfs, &feature, None, &locations);

        assert_eq!(actual.id, baseline.id);
        assert_eq!(actual.trip_update, baseline.trip_update);
        let vehicle = actual.vehicle.unwrap();
        assert_eq!(vehicle.trip, baseline.vehicle.unwrap().trip);
        assert_eq!(vehicle.timestamp, Some(OBSERVED_AT));
        let position = vehicle.position.unwrap();
        assert_eq!(position.latitude, 41.021117);
        assert_eq!(position.longitude, -73.625574);
        assert_eq!(position.speed, None);
        assert_eq!(position.bearing, None);
        assert!(
            locations
                .position(jiff::civil::date(2026, 11, 25), "2171")
                .is_none()
        );
        assert!(
            locations
                .position(jiff::civil::date(2026, 11, 24), "2172")
                .is_none()
        );
    }

    #[test]
    fn radar_prefers_the_latest_usable_amtrak_record_and_converts_speed() {
        let mut gps = radar_train();
        gps["location"]["source"] = json!("GPS");
        gps["location"]["speed"] = json!(60.0);
        gps["location"]["heading"] = json!(225.0);
        let mut older = radar_train();
        older["location"]["timestamp"] = json!(OBSERVED_AT - 60);
        let mut stale = radar_train();
        stale["train_num"] = json!("2172");
        stale["location"]["timestamp"] = json!(OBSERVED_AT - 301);
        let mut commuter = radar_train();
        commuter["details"]["branch"] = json!("New Haven");
        commuter["location"]["timestamp"] = json!(OBSERVED_AT + 1);
        let locations = Locations::from_trains(
            [gps, older, stale, commuter]
                .into_iter()
                .map(|train| serde_json::from_value(train).unwrap())
                .collect(),
            OBSERVED_AT,
        );

        let date = jiff::civil::date(2026, 11, 24);
        let (position, timestamp) = locations.position(date, "2171").unwrap();
        assert_eq!(timestamp, OBSERVED_AT);
        assert!((position.speed.unwrap() - 26.8224).abs() < 0.0001);
        assert_eq!(position.bearing, Some(225.0));
        assert!(locations.position(date, "2172").is_none());
    }
}
