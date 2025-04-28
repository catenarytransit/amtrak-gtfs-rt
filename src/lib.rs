//! # amtrak-gtfs-rt
//!Decrypts Amtrak's GTFS-RT
//!
//!This software package decrypts the Amtrak track-a-train json data and performs lookups of trip information in the GTFS schedule to match each vehicle with it's route_id and trip_id.
//!Pull requests are welcome!
//!
//!A valid Amtrak GTFS structure must be passed into the function to work.
//!
//!Here's an example of some working code!
//!```rust
//!extern crate amtrak_gtfs_rt;
//!
//!use prost::Message;
//!use gtfs_structures::Gtfs;
//!use amtrak_gtfs_rt::fetch_amtrak_gtfs_rt;

//!#[tokio::main]
//!async fn main() {
//!    let gtfs = Gtfs::from_url_async("https://content.amtrak.com/content/gtfs/GTFS.zip")
//!    .await
//!    .unwrap();
//!
//!    let client = reqwest::Client::new();
//!    let amtrak_gtfs_rt = fetch_amtrak_gtfs_rt(&gtfs, &client).await.unwrap();
//!
//!    //extract the binary data
//!    let vehicle_data = amtrak_gtfs_rt.vehicle_positions.encode_to_vec();
//!    let trip_data = amtrak_gtfs_rt.trip_updates.encode_to_vec();
//!}
//!```
//!
//! Note that the Metropolitan Transportation Commission also publishes Capital Corridor in their own feed.
//! https://511.org/open-data/transit provides Capital Corridor as "CC". This data refreshes more often (and is closer in location & time), and shows locomotive numbers.
//! For this reason, you may wish to remove Capital Corridor from this feed.
//! Thus, we've included a function `filter_capital_corridor()` which takes in any `FeedMessage` and removes CC vehicles and trips.

use asm::asm_alert_to_gtfs_rt;
use chrono::{Datelike, NaiveDate, NaiveDateTime, TimeZone, Weekday};
use geojson::FeatureCollection;
use gtfs_realtime::FeedEntity;
use gtfs_realtime::FeedMessage;
use gtfs_structures::Gtfs;
use std::collections::HashMap;
use std::time::SystemTime;
pub mod asm;

//Written by Kyler Chin - Catenary Transit Initiatives.
pub fn filter_capital_corridor(input: FeedMessage) -> FeedMessage {
    let cc_route_id = "84";

    FeedMessage {
        entity: input
            .entity
            .into_iter()
            .filter(|item| {
                if item.vehicle.is_some() {
                    let vehicle = item.vehicle.as_ref().unwrap();
                    if vehicle.trip.is_some() {
                        let trip = vehicle.trip.as_ref().unwrap();
                        if trip.route_id.is_some()
                            && trip.route_id.as_ref().unwrap().as_str() == cc_route_id
                        {
                            return false;
                        }
                    }
                }

                if item.trip_update.is_some() {
                    let trip_update = item.trip_update.as_ref().unwrap();
                    let trip = &trip_update.trip;

                    if trip.route_id.is_some() {
                        let route_id = trip.route_id.as_ref().unwrap();

                        if route_id == cc_route_id {
                            return false;
                        }
                    }
                }

                true
            })
            .collect::<Vec<FeedEntity>>(),
        header: input.header,
    }
}

#[derive(Clone, Debug)]
pub struct GtfsAmtrakResults {
    pub trip_updates: FeedMessage,
    pub vehicle_positions: FeedMessage,
    pub alerts: FeedMessage,
}

#[derive(Clone, Debug)]
pub struct GtfsAmtrakResultsJoined {
    pub unified_feed: FeedMessage,
}

#[derive(serde::Deserialize, Debug)]
pub struct AmtrakArrivalJson {
    //{"code":"CTL",
    code: String,
    //"tz":"P",
    tz: char,
    //"bus":false,
    bus: bool,
    // "scharr":"12/11/2023 17:33:00",
    scharr: Option<String>,
    // "schdep":"12/11/2023 17:36:00",
    schdep: Option<String>,
    // "schcmnt":"",
    schcmnt: String,
    //"autoarr":true,
    autoarr: bool,
    //"autodep":true,
    autodep: bool,
    // "estarr":"12/11/2023 17:33:00",
    estarr: Option<String>,
    //"estdep":"12/11/2023 17:36:00",
    estdep: Option<String>,
    //same format as estimate but it's actual historical time
    postarr: Option<String>,
    postdep: Option<String>,
    //"estarrcmnt":"ON TIME",
    estarrcmnt: Option<String>,
    //"estdepcmnt":"ON TIME"
    estdepcmnt: Option<String>,
}

fn feature_to_amtrak_arrival_structs(feature: &geojson::Feature) -> Vec<AmtrakArrivalJson> {
    let mut amtrak_arrival_jsons = vec![];

    for i in 0i32..100i32 {
        let mut key = String::from("Station");
        key.push_str(&i.to_string());

        if let Some(station_text) = feature.properties.as_ref().unwrap().get(key.as_str()) {
            if let serde_json::value::Value::String(station_text) = station_text {
                let amtrak_arrival: Result<AmtrakArrivalJson, serde_json::Error> =
                    serde_json::from_str(station_text);

                if amtrak_arrival.is_ok() {
                    amtrak_arrival_jsons.push(amtrak_arrival.unwrap());
                } else {
                    println!(
                        "Error parsing amtrak arrival json, {}\n{}",
                        station_text,
                        amtrak_arrival.unwrap_err()
                    );
                }
            }
        };
    }

    amtrak_arrival_jsons
}

fn get_speed(feature: &geojson::Feature) -> Option<f32> {
    match feature.properties.as_ref().unwrap().get("Velocity") {
        Some(speed_text) => match speed_text {
            serde_json::value::Value::String(x) => {
                Some(x.as_str().parse::<f32>().unwrap() * 0.44704)
            }
            _ => None,
        },
        _ => None,
    }
}

fn get_bearing(feature: &geojson::Feature) -> Option<f32> {
    match feature.properties.as_ref().unwrap().get("Heading") {
        Some(bearing_text) => match bearing_text {
            serde_json::value::Value::String(x) => match x.as_str() {
                "N" => Some(0.001),
                "NE" => Some(45.0),
                "E" => Some(90.0),
                "SE" => Some(135.0),
                "S" => Some(180.0),
                "SW" => Some(225.0),
                "W" => Some(270.0),
                "NW" => Some(315.0),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

fn feature_to_gtfs_unified(
    gtfs: &Gtfs,
    feature: &geojson::Feature,
    asm_lookup_table: Option<&HashMap<(NaiveDate, String), Vec<asm::AsmAlert>>>,
) -> FeedEntity {
    let geometry = feature.geometry.as_ref().unwrap();
    let point: Option<geojson::PointType> = match geometry.value.clone() {
        geojson::Value::Point(x) => Some(x),
        _ => None,
    };

    let long_name_to_route_id_hashmap: HashMap<String, String> = HashMap::from_iter(
        gtfs.routes
            .iter()
            .filter(|(_, route)| route.long_name.is_some())
            .map(|(_string, route)| (route.long_name.as_ref().unwrap().clone(), route.id.clone())),
    );

    let mut trip_name_to_id_hashmap: HashMap<String, Vec<String>> = HashMap::new();

    for (trip_id, trip) in gtfs.trips.iter() {
        if trip.trip_short_name.is_some() {
            trip_name_to_id_hashmap
                .entry(trip.trip_short_name.as_ref().unwrap().clone())
                .and_modify(|list| list.push(trip_id.clone()))
                .or_insert(vec![trip_id.clone()]);
        }
    }

    let trip_name_to_id_hashmap = trip_name_to_id_hashmap;

    let point = point.unwrap();

    let speed: Option<f32> = get_speed(feature);

    //unix time seconds
    let timestamp: Option<u64> = match feature.properties.as_ref().unwrap().get("updated_at") {
        Some(timestamp_text) => match timestamp_text {
            serde_json::value::Value::String(timestamp_text) => 
                process_timestamp_text(timestamp_text).map(|x| x as u64),
            _ => None,
        },
        _ => None,
    };

    let trip_name: Option<String> = match feature.properties.as_ref().unwrap().get("TrainNum") {
        Some(a) => match a {
            serde_json::value::Value::String(x) => Some(x.clone()),
            _ => None,
        },
        _ => None,
    };

    let origin_tz = match feature.properties.as_ref().unwrap().get("OriginTZ") {
        Some(a) => match a {
            serde_json::value::Value::String(x) if x.len() == 1 => Some(x.chars().next().unwrap()),
            _ => None,
        },
        _ => None,
    }
    .unwrap();

    let origin_time_string = match feature.properties.as_ref().unwrap().get("OrigSchDep") {
        Some(a) => match a {
            serde_json::value::Value::String(x) => Some(x.clone()),
            _ => None,
        },
        _ => None,
    }
    .unwrap();

    let features_list = feature_to_amtrak_arrival_structs(feature);

    let arrivals: Vec<gtfs_realtime::trip_update::StopTimeUpdate> =
    features_list
        .iter()
        .enumerate()
        .map(|(i, feature)| gtfs_realtime::trip_update::StopTimeUpdate {
            stop_sequence: None,
            stop_id: Some(feature.code.clone()),
            arrival: match &feature.postarr {
                Some(postarr) => Some(gtfs_realtime::trip_update::StopTimeEvent {
                    delay: None,
                    time: time_and_tz_to_unix(postarr, feature.tz),
                    uncertainty: None,
                }),
                None => match &feature.estarr {
                    Some(estarr) => Some(gtfs_realtime::trip_update::StopTimeEvent {
                        delay: None,
                        time: time_and_tz_to_unix(estarr, feature.tz),
                        uncertainty: None,
                    }),
                    //There is no provided arrival time, interpolate it from the previous stop
                    None => match i {
                            0 => None,
                            _ => {
                                let previous = features_list.get(i - 1);

                                match previous {
                                    Some(previous) => {
                                        let previous_departure_time = match &previous.postdep {
                                            Some(postdep) => Some(time_and_tz_to_unix(postdep, feature.tz)),
                                            None => previous.estdep.as_ref().map(|estdep| time_and_tz_to_unix(estdep, feature.tz)),
                                        };

                                        match previous_departure_time {
                                            None => None,
                                            Some(previous_departure_time) => {
                                                match &previous.schdep {
                                                    Some(previous_schdep) => {
                                                        let prev_sch_dep = time_and_tz_to_unix(previous_schdep, feature.tz);
                                                        let delay = match (previous_departure_time,prev_sch_dep) {
                                                            (Some(a), Some(b)) => Some(a - b),
                                                            _ => None
                                                        };

                                                        match delay {
                                                            Some( delay) => {
                                                                match &feature.scharr {
                                                                    Some(scharr) => {
                                                                        let arrival_time = time_and_tz_to_unix(scharr, feature.tz);

                                                                        match arrival_time {
                                                                            Some(arrival_time) => {
                                                                                let arrival_time = arrival_time + delay;
        
                                                                        Some(gtfs_realtime::trip_update::StopTimeEvent {
                                                                            delay: Some(delay.try_into().unwrap()),
                                                                            time: Some(arrival_time),
                                                                            uncertainty: None,
                                                                        })
                                                                            },
                                                                            None => None
                                                                        }
        
                                                                        
                                                                    },
                                                                    None => None,
                                                                }
                                                            },
                                                            None => None
                                                        }
                                                    },
                                                    None => None
                                                }
                                            }
                                        }
                                    },
                                    None => None,
                                }
                        },
                }
            }},
            departure: match &feature.postdep {
                Some(postdep) => Some(gtfs_realtime::trip_update::StopTimeEvent {
                    delay: None,
                    time: time_and_tz_to_unix(postdep, feature.tz),
                    uncertainty: None,
                }),
                None => feature.estdep.as_ref().map(|estdep| gtfs_realtime::trip_update::StopTimeEvent {
                    delay: None,
                    time: time_and_tz_to_unix(estdep, feature.tz),
                    uncertainty: None,
                })},
            departure_occupancy_status: None,
            schedule_relationship: None,
            stop_time_properties: None,
        })
        .collect::<Vec<gtfs_realtime::trip_update::StopTimeUpdate>>();

    let origin_local_time = origin_departure(&origin_time_string, origin_tz);

    let starting_yyyy_mm_dd_in_new_york = origin_local_time
        .with_timezone(&chrono_tz::America::New_York)
        .format("%Y%m%d")
        .to_string();

    let origin_weekday = origin_local_time.weekday();

    let route_name: Option<String> = match feature.properties.as_ref().unwrap().get("RouteName") {
        Some(a) => match a {
            serde_json::value::Value::String(x) => Some(x.clone()),
            _ => None,
        },
        _ => None,
    };

    let train_num: Option<String> = match feature.properties.as_ref().unwrap().get("TrainNum") {
        Some(a) => match a {
            serde_json::value::Value::String(x) => Some(x.clone()),
            _ => None,
        },
        _ => None,
    };

    let trip_id: Option<String> = match route_name.as_deref() {
        Some("San Joaquins") => train_num.clone(),
        _ => match trip_name {
            Some(x) => {
                let hashmap_results = trip_name_to_id_hashmap.get(&x);

                match hashmap_results {
                    Some(hashmap_results) => {
                        match hashmap_results.len() {
                            1 => Some(hashmap_results[0].clone()),
                            _ => {
                                let possible_results = hashmap_results
                                    .iter()
                                    .filter(|trip_id_candidate| {
                                        let trip =
                                            gtfs.trips.get(trip_id_candidate.as_str()).unwrap();

                                        let calendar = gtfs.calendar.get(&trip.service_id).unwrap();

                                        match origin_weekday {
                                            Weekday::Mon => calendar.monday,
                                            Weekday::Tue => calendar.tuesday,
                                            Weekday::Wed => calendar.wednesday,
                                            Weekday::Thu => calendar.thursday,
                                            Weekday::Fri => calendar.friday,
                                            Weekday::Sat => calendar.saturday,
                                            Weekday::Sun => calendar.sunday,
                                        }
                                        //Seven days a week
                                        //Every hour, every minute, every second
                                        //You know night after night
                                        //I'll be lovin' you right, seven days a week
                                    })
                                    .collect::<Vec<&String>>();

                                match possible_results.len() {
                                    0 => None,
                                    _ => Some(possible_results[0].clone()),
                                }
                            }
                        }
                    }
                    None => None,
                }
            }
            None => None,
        },
    };

    let id = match &train_num {
        Some(train_num) => Some(format!("{}-{}", starting_yyyy_mm_dd_in_new_york, train_num)),
        None => None,
    };

    let route_id: Option<String> = match route_name {
        Some(route_name) => match route_name.as_str() {
            "San Joaquins" => Some("SJ2".to_string()),
            _ => long_name_to_route_id_hashmap
                .get(&route_name.clone())
                .cloned(),
        },
        None => None,
    };

    let bearing: Option<f32> = get_bearing(feature);

    let trip_desc = gtfs_realtime::TripDescriptor {
        trip_id: trip_id.clone(),
        route_id: route_id.clone(),
        direction_id: None,
        start_time: None,
        start_date: Some(starting_yyyy_mm_dd_in_new_york.clone()),
        modified_trip: None,
        schedule_relationship: None,
    };

    let informed_entity = gtfs_realtime::EntitySelector {
        agency_id: None,
        route_id: route_id.clone(),
        trip: Some(trip_desc.clone()),
        route_type: None,
        stop_id: None,
        direction_id: None,
    };

    let alert = match &train_num {
        Some(train_num) => match asm_lookup_table {
            Some(asm_lookup_table) => {
                match asm_lookup_table.get(&(origin_local_time.date_naive(), train_num.clone())) {
                    Some(alerts) => asm_alert_to_gtfs_rt(informed_entity, alerts),
                    None => None,
                }
            }
            None => None,
        },
        None => None,
    };

    FeedEntity {
        alert: alert,
        id: id.unwrap(),
        is_deleted: Some(false),
        trip_modifications: None,
        stop: None,
        shape: None,
        trip_update: Some(gtfs_realtime::TripUpdate {
            vehicle: None,
            trip: trip_desc.clone(),
            timestamp,
            delay: None,
            stop_time_update: arrivals,
            trip_properties: None,
        }),
        vehicle: Some(gtfs_realtime::VehiclePosition {
            stop_id: None,
            current_status: None,
            timestamp,
            congestion_level: None,
            occupancy_status: None,
            occupancy_percentage: None,
            multi_carriage_details: vec![],
            current_stop_sequence: None,
            vehicle: None,
            trip: Some(trip_desc.clone()),
            position: Some(gtfs_realtime::Position {
                speed,
                odometer: None,
                bearing,
                latitude: point[1] as f32,
                longitude: point[0] as f32,
            }),
        }),
    }
}

pub fn make_gtfs_header() -> gtfs_realtime::FeedHeader {
    gtfs_realtime::FeedHeader {
        gtfs_realtime_version: String::from("2.0"),
        incrementality: None,
        timestamp: Some(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        ),
    }
}

fn tz_char_to_tz(tz: char) -> Option<chrono_tz::Tz> {
    match tz {
        'E' => Some(chrono_tz::America::New_York),
        'M' => Some(chrono_tz::America::Denver),
        'P' => Some(chrono_tz::America::Los_Angeles),
        'C' => Some(chrono_tz::America::Chicago),
        'A' => Some(chrono_tz::America::Phoenix),
        //NEVER SUPPOSED TO HAPPEN
        _ => Some(chrono_tz::America::New_York),
    }
}

//for arrivals and departures, does not parse PM or AM.
fn time_and_tz_to_unix(timestamp_text: &String, tz: char) -> Option<i64> {
    //  println!("{}, {}", timestamp_text, tz);
    // tz: String like "P", "C", "M", or "E"
    //time: "12/11/2023 17:36:00"
    let naive_dt = NaiveDateTime::parse_from_str(timestamp_text, "%m/%d/%Y %H:%M:%S").unwrap();

    let local_time_representation = tz_char_to_tz(tz)
        .unwrap()
        .from_local_datetime(&naive_dt)
        .latest();

    match local_time_representation {
        None => None,
        Some(local_time_representation) => {
            local_time_representation.timestamp().try_into().unwrap()
        }
    }
}

//for origin departure conversion to local time representation
pub fn origin_departure(timestamp_text: &str, tz: char) -> chrono::DateTime<chrono_tz::Tz> {
    let naive_dt = NaiveDateTime::parse_from_str(timestamp_text, "%m/%d/%Y %l:%M:%S %p").unwrap();

    tz_char_to_tz(tz)
        .unwrap()
        .from_local_datetime(&naive_dt)
        .latest()
        .unwrap()
}

//time is formatted 11/18/2023 4:58:09 PM
pub fn process_timestamp_text(timestamp_text: &str) -> Option<i64> {
    let naive_dt = NaiveDateTime::parse_from_str(timestamp_text, "%m/%d/%Y %l:%M:%S %p");

    if naive_dt.is_err() {
        println!("Error parsing timestamp: {}", timestamp_text);
        return None;
    }

    let naive_dt = naive_dt.unwrap();

    let eastern_time = chrono_tz::America::New_York
        .from_local_datetime(&naive_dt)
        .latest()
        .unwrap();

    eastern_time.timestamp().try_into().unwrap()
}

pub async fn fetch_amtrak_gtfs_rt(
    gtfs: &Gtfs,
    client: &reqwest::Client,
) -> Result<GtfsAmtrakResults, Box<dyn std::error::Error + Sync + Send>> {
    let joined_res = fetch_amtrak_gtfs_rt_joined(gtfs, client).await;

    let mut vehicles: Vec<gtfs_realtime::FeedEntity> = vec![];
    let mut trips: Vec<gtfs_realtime::FeedEntity> = vec![];
    let mut alerts: Vec<FeedEntity> = vec![];

    match joined_res {
        Ok(joined_res) => {
            for feed_entity in joined_res.unified_feed.entity {
                vehicles.push(feed_entity.clone());
                trips.push(feed_entity.clone());

                if feed_entity.alert.is_some() {
                    alerts.push(feed_entity.clone());
                }
            }

            Ok(GtfsAmtrakResults {
                trip_updates: FeedMessage {
                    entity: trips,
                    header: joined_res.unified_feed.header.clone(),
                },
                vehicle_positions: FeedMessage {
                    entity: vehicles,
                    header: joined_res.unified_feed.header.clone(),
                },
                alerts: FeedMessage {
                    entity: alerts,
                    header: joined_res.unified_feed.header.clone(),
                },
            })
        }
        Err(x) => Err(x),
    }
}

pub async fn fetch_amtrak_gtfs_rt_joined(
    gtfs: &Gtfs,
    client: &reqwest::Client,
) -> Result<GtfsAmtrakResultsJoined, Box<dyn std::error::Error + Sync + Send>> {
    let raw_data = client
        .get("https://maps.amtrak.com/services/MapDataService/trains/getTrainsData")
        .send()
        .await;

    let raw_asm_data = client
        .get("https://asm-backend.transitdocs.com/map")
        .send()
        .await;

    match raw_data {
        Ok(raw_data) => {
            //println!("Raw data successfully downloaded");

            let decrypted_string = amtk::decrypt(raw_data.text().await.unwrap().as_str())?;

            let geojson: geojson::GeoJson = decrypted_string.parse::<geojson::GeoJson>()?;
            let features_collection: FeatureCollection = FeatureCollection::try_from(geojson)?;

            let lookup_table: Option<HashMap<(NaiveDate, String), Vec<asm::AsmAlert>>> =
                match raw_asm_data {
                    Ok(raw_asm_data) => {
                        let asm_root = raw_asm_data.text().await?;

                        let asm_root_json = serde_json::from_str::<asm::AsmRoot>(&asm_root);

                        match asm_root_json {
                            Ok(asm_root) => Some(asm::make_lookup_table_from_asm_root(asm_root)),
                            Err(_) => None,
                        }
                    }
                    Err(_) => None,
                };

            Ok(GtfsAmtrakResultsJoined {
                unified_feed: FeedMessage {
                    entity: features_collection
                        .features
                        .iter()
                        .map(|feature: &geojson::Feature| {
                            feature_to_gtfs_unified(&gtfs, feature, lookup_table.as_ref())
                        })
                        .collect::<Vec<FeedEntity>>(),
                    header: make_gtfs_header(),
                },
            })
        }
        Err(err) => Err(Box::new(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_amtrak() {
        let client = reqwest::ClientBuilder::new()
            .deflate(true)
            .gzip(true)
            .brotli(true)
            .build()
            .unwrap();

        println!("download and process amtrak gtfs file");

        let gtfs = Gtfs::from_url_async("https://content.amtrak.com/content/gtfs/GTFS.zip")
            .await
            .unwrap();

        println!("amtrak download finished");

        let amtrak_results = fetch_amtrak_gtfs_rt_joined(&gtfs, &client).await;

        assert!(amtrak_results.is_ok());

        for entity in amtrak_results.as_ref().unwrap().unified_feed.entity.iter() {
            //println!("{:?}", entity.trip_update);
        }

        let raw_data = client
            .get("https://maps.amtrak.com/services/MapDataService/trains/getTrainsData")
            .send()
            .await
            .unwrap();

        let decrypted_string = amtk::decrypt(raw_data.text().await.unwrap().as_str()).unwrap();

        let geojson: geojson::GeoJson = decrypted_string.parse::<geojson::GeoJson>().unwrap();
        let features_collection: FeatureCollection = FeatureCollection::try_from(geojson).unwrap();

        assert_eq!(
            features_collection.features.len(),
            amtrak_results.as_ref().unwrap().unified_feed.entity.len()
        );

        // println!("{:?}", amtrak_results.unwrap());
    }

    #[test]
    fn read_last_stop() {
        let test_str = r#"{"code":"LAX","tz":"P","bus":false,"scharr":"10/11/2024 16:57:00","schcmnt":"","autoarr":false,"autodep":false}"#;

        let amtrak_arrival = serde_json::from_str::<AmtrakArrivalJson>(test_str);

        assert!(amtrak_arrival.is_ok());

        let amtrak_arrival = amtrak_arrival.unwrap();

        assert_eq!(amtrak_arrival.code, "LAX");
    }
}
