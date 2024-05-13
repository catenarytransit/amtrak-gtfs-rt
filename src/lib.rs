//! # amtrak-gtfs-rt
//!Decrypts Amtrak's GTFS-RT
//! 
//!This software package decrypts the Amtrak track-a-train json data and performs lookups of trip information in the GTFS schedule to match each vehicle with it's route_id and trip_id.
//!Pull requests are welcome!
//!
//!A valid Amtrak GTFS structure must be passed into the function to work.
//!
//!Here's an example of some working code! 
//!Note that `prost` version `0.11` should be used, as `gtfs-rt` does not use `0.12` yet.
//!```rust 
//!extern crate amtrak_gtfs_rt;
//!
//!use prost::Message;
//!use gtfs_structures::Gtfs;

//!#[tokio::main]
//!async fn main() {
//!    let gtfs = Gtfs::from_url_async("https://content.amtrak.com/content/gtfs/GTFS.zip")
//!    .await
//!    .unwrap();
//!
//!    let client = reqwest::Client::new();
//!    let amtrak_gtfs_rt = amtrak_gtfs_rt::fetch_amtrak_gtfs_rt(&gtfs, &client).await.unwrap();
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
//! Thus, we've included a function `filter_capital_corridor()` which takes in any `gtfs_rt::FeedMessage` and removes CC vehicles and trips.


use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, TimeZone, Weekday};
use geojson::FeatureCollection;
use gtfs_structures::Gtfs;
use std::collections::HashMap;
use std::time::SystemTime;
pub mod stop_times;
use crate::stop_times::RootTripData;

//Written by Kyler Chin - Catenary Transit Initiatives.
pub fn filter_capital_corridor(input: gtfs_rt::FeedMessage) -> gtfs_rt::FeedMessage {
    let cc_route_id = "84";

    gtfs_rt::FeedMessage {
        entity: input
            .entity
            .into_iter()
            .filter(|item| {
                if item.vehicle.is_some() {
                    let vehicle = item.vehicle.as_ref().unwrap();
                    if vehicle.trip.is_some() {
                        let trip = vehicle.trip.as_ref().unwrap();
                        if trip.route_id.is_some() {
                            if trip.route_id.as_ref().unwrap().as_str() == cc_route_id {
                                return false;
                            }
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
            .collect::<Vec<gtfs_rt::FeedEntity>>(),
        header: input.header,
    }
}

#[derive(Clone, Debug)]
pub struct GtfsAmtrakResults {
    pub trip_updates: gtfs_rt::FeedMessage,
    pub vehicle_positions: gtfs_rt::FeedMessage,
}

#[derive(Clone, Debug)]
pub struct GtfsAmtrakResultsJoined {
    pub unified_feed: gtfs_rt::FeedMessage,
}

#[derive(serde::Deserialize)]
pub struct AmtrakArrivalJson {
    //{"code":"CTL",
    code: String,
    //"tz":"P",
    tz: char,
    //"bus":false,
    bus: bool,
    // "scharr":"12/11/2023 17:33:00",
    scharr: String,
    // "schdep":"12/11/2023 17:36:00",
    schdep: String,
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

        match feature.properties.as_ref().unwrap().get(key.as_str()) {
            Some(station_text) => match station_text {
                serde_json::value::Value::String(station_text) => {
                    let amtrak_arrival: Result<AmtrakArrivalJson, serde_json::Error> =
                        serde_json::from_str(&station_text);

                    if amtrak_arrival.is_ok() {
                        amtrak_arrival_jsons.push(amtrak_arrival.unwrap());
                    }
                }
                _ => {}
            },
            _ => {}
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

fn feature_to_gtfs_unified(gtfs: &Gtfs, feature: &geojson::Feature) -> gtfs_rt::FeedEntity {
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
            serde_json::value::Value::String(timestamp_text) => {
                Some(process_timestamp_text(&timestamp_text))
            }
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

    let arrivals: Vec<gtfs_rt::trip_update::StopTimeUpdate> =
    feature_to_amtrak_arrival_structs(feature)
        .iter()
        .map(|feature| gtfs_rt::trip_update::StopTimeUpdate {
            stop_sequence: None,
            stop_id: Some(feature.code.clone()),
            arrival: match &feature.estarr {
                Some(estarr) => Some(gtfs_rt::trip_update::StopTimeEvent {
                    delay: None,
                    time: Some(time_and_tz_to_unix(&estarr, feature.tz)),
                    uncertainty: None,
                }),
                None => None,
            },
            departure: match &feature.estdep {
                Some(estdep) => Some(gtfs_rt::trip_update::StopTimeEvent {
                    delay: None,
                    time: Some(time_and_tz_to_unix(&estdep, feature.tz)),
                    uncertainty: None,
                }),
                None => None,
            },
            departure_occupancy_status: None,
            schedule_relationship: None,
            stop_time_properties: None,
        })
        .collect::<Vec<gtfs_rt::trip_update::StopTimeUpdate>>();

    let origin_local_time = origin_departure(&origin_time_string, origin_tz);

    let starting_yyyy_mm_dd_in_new_york = origin_local_time.with_timezone(&chrono_tz::America::New_York).format("%Y%m%d").to_string();

    let origin_weekday = origin_local_time.weekday();

    let trip_id: Option<String> = match trip_name {
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
                                    let trip = gtfs.trips.get(trip_id_candidate.as_str()).unwrap();

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
    };

    let route_name: Option<String> = match feature.properties.as_ref().unwrap().get("RouteName") {
        Some(a) => match a {
            serde_json::value::Value::String(x) => Some(x.clone()),
            _ => None,
        },
        _ => None,
    };

    let id: Option<String> = match feature.properties.as_ref().unwrap().get("TrainNum") {
        Some(a) => match a {
            serde_json::value::Value::String(x) => Some(x.clone()),
            _ => None,
        },
        _ => None,
    };

    let route_id: Option<String> = match route_name {
        Some(route_name) => long_name_to_route_id_hashmap
            .get(&route_name.clone())
            .cloned(),
        None => None,
    };

    let bearing: Option<f32> = get_bearing(feature);

    let trip_desc = gtfs_rt::TripDescriptor {
        trip_id,
        route_id,
        direction_id: None,
        start_time: None,
        start_date: Some(starting_yyyy_mm_dd_in_new_york.clone()),
        modified_trip: None,
        schedule_relationship: None,
    };

    gtfs_rt::FeedEntity {
        alert: None,
        id: id.unwrap(),
        is_deleted: Some(false),
        trip_modifications: None,
        stop: None,
        shape: None,
        trip_update: Some(gtfs_rt::TripUpdate {
            vehicle: None,
            trip: trip_desc.clone(),
            timestamp,
            delay: None,
            stop_time_update: arrivals,
            trip_properties: None,
        }),
        vehicle: Some(gtfs_rt::VehiclePosition {
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
            position: Some(gtfs_rt::Position {
                speed,
                odometer: None,
                bearing,
                latitude: point[1] as f32,
                longitude: point[0] as f32,
            }),
        }),
    }
}

pub fn make_gtfs_header() -> gtfs_rt::FeedHeader {
    gtfs_rt::FeedHeader {
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
        _ => None,
    }
}

//for arrivals and departures, does not parse PM or AM.
fn time_and_tz_to_unix(timestamp_text: &String, tz: char) -> i64 {
    // tz: String like "P", "C", "M", or "E"
    //time: "12/11/2023 17:36:00"
    let naive_dt = NaiveDateTime::parse_from_str(timestamp_text, "%m/%d/%Y %H:%M:%S").unwrap();

    let local_time_representation = tz_char_to_tz(tz)
        .unwrap()
        .from_local_datetime(&naive_dt)
        .unwrap();

    local_time_representation.timestamp()
}

//for origin departure conversion to local time representation
pub fn origin_departure(timestamp_text: &str, tz: char) -> chrono::DateTime<chrono_tz::Tz> {
    let naive_dt = NaiveDateTime::parse_from_str(timestamp_text, "%m/%d/%Y %l:%M:%S %p").unwrap();

    let local_time_representation = tz_char_to_tz(tz)
        .unwrap()
        .from_local_datetime(&naive_dt)
        .unwrap();

    local_time_representation
}

//time is formatted 11/18/2023 4:58:09 PM
pub fn process_timestamp_text(timestamp_text: &str) -> u64 {
    let naive_dt = NaiveDateTime::parse_from_str(timestamp_text, "%m/%d/%Y %l:%M:%S %p").unwrap();

    let eastern_time = chrono_tz::America::New_York
        .from_local_datetime(&naive_dt)
        .unwrap();

    eastern_time.timestamp().try_into().unwrap()
}

pub async fn fetch_amtrak_gtfs_rt(
    gtfs: &Gtfs,
    client: &reqwest::Client,
) -> Result<GtfsAmtrakResults, Box<dyn std::error::Error>> {
    let joined_res = fetch_amtrak_gtfs_rt_joined(gtfs, client).await;

    let mut vehicles: Vec<gtfs_rt::FeedEntity> = vec![];
    let mut trips: Vec<gtfs_rt::FeedEntity> = vec![];

    match joined_res {
        Ok(joined_res) => {
            for feed_entity in joined_res.unified_feed.entity {
                vehicles.push(feed_entity.clone());
                trips.push(feed_entity.clone());
            }

            Ok(GtfsAmtrakResults {
                trip_updates: gtfs_rt::FeedMessage {
                    entity: trips,
                    header: joined_res.unified_feed.header.clone(),
                },
                vehicle_positions: gtfs_rt::FeedMessage {
                    entity: vehicles,
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
) -> Result<GtfsAmtrakResultsJoined, Box<dyn std::error::Error>> {
    let raw_data = client
        .get("https://maps.amtrak.com/services/MapDataService/trains/getTrainsData")
        .send()
        .await;

    match raw_data {
        Ok(raw_data) => {
            //println!("Raw data successfully downloaded");

            let decrypted_string = amtk::decrypt(raw_data.text().await.unwrap().as_str())?;

            let geojson: geojson::GeoJson = decrypted_string.parse::<geojson::GeoJson>()?;
            let features_collection: FeatureCollection =
                FeatureCollection::try_from(geojson)?;

            let list_of_train_ids = features_collection
                .features
                .iter()
                .map(|feature| {
                   { let train_num =  match feature.properties.as_ref().unwrap().get("TrainNum") {
                        Some(a) => match a {
                            serde_json::value::Value::String(x) => Some(x.clone()),
                            _ => None,
                        },
                        _ => None,
                    };
                    let starting_date = match feature.properties.as_ref().unwrap().get("OrigSchDep") {
                        Some(a) => match a {
                            serde_json::value::Value::String(x) => {
                                        let first_half = NaiveDate::parse_from_str(x.split(" ").nth(0).unwrap(), "%m/%d/%Y");

                                        match first_half {
                                            Ok(first_half) => {
                                                Some(first_half)
                                            },
                                            Err(_) => None,
                                        }
                            },
                            _ => None,
                        },
                        _ => None,
                    };

                    match (train_num, starting_date) {
                        (Some(train_num), Some(starting_date)) => Some((train_num, starting_date)),
                        _ => None
                    }
                }  
                }).flatten().collect::<Vec<(String, NaiveDate)>>();

            //query the stop times all simultaniously and put into hashmap
            //query_all_trips_simultaniously

           // let stop_times = stop_times::query_all_trips_simultaniously(&list_of_train_ids).await;

            //println!("Successfully decrypted");
            //println!("{}", decrypted_string);
            Ok(GtfsAmtrakResultsJoined {
                unified_feed: gtfs_rt::FeedMessage {
                    entity: features_collection
                        .features
                        .iter()
                        .map(|feature: &geojson::Feature| feature_to_gtfs_unified(&gtfs, feature))
                        .collect::<Vec<gtfs_rt::FeedEntity>>(),
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

        for entity in amtrak_results.unwrap().unified_feed.entity {
            println!("{:?}", entity.trip_update);
        }

       // println!("{:?}", amtrak_results.unwrap());
    }
}
