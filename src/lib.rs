use chrono::Datelike;
use chrono::NaiveDate;
use chrono::TimeZone;
use chrono::Weekday;
use geojson::FeatureCollection;
use gtfs_structures::Gtfs;
use std::collections::HashMap;
use std::time::SystemTime;

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
    tz: String,
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

    for i in 0 as i32..100 as i32 {
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

fn option_i32_to_i64(x: Option<i32>) -> Option<i64> {
    match x {
        Some(x) => Some(x as i64),
        None => None,
    }
}

pub fn feature_to_gtfs_unified(gtfs: &Gtfs, feature: &geojson::Feature) -> gtfs_rt::FeedEntity {
    let geometry = feature.geometry.as_ref().unwrap();
    let point: Option<geojson::PointType> = match geometry.value.clone() {
        geojson::Value::Point(x) => Some(x),
        _ => None,
    };

    let long_name_to_route_id_hashmap: HashMap<String, String> = HashMap::from_iter(
        gtfs.routes
            .iter()
            .map(|(_string, route)| (route.long_name.clone(), route.id.clone())),
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

    let arrivals: Vec<gtfs_rt::trip_update::StopTimeUpdate> =
        feature_to_amtrak_arrival_structs(feature)
            .iter()
            .map(|feature| gtfs_rt::trip_update::StopTimeUpdate {
                stop_sequence: None,
                stop_id: Some(feature.code.clone()),
                arrival: match &feature.estarr {
                    Some(estarr) => Some(gtfs_rt::trip_update::StopTimeEvent {
                        delay: None,
                        time: option_i32_to_i64(time_and_tz_to_unix(&estarr, &feature.tz)),
                        uncertainty: None,
                    }),
                    None => None,
                },
                departure: match &feature.estdep {
                    Some(estdep) => Some(gtfs_rt::trip_update::StopTimeEvent {
                        delay: None,
                        time: option_i32_to_i64(time_and_tz_to_unix(&estdep, &feature.tz)),
                        uncertainty: None,
                    }),
                    None => None,
                },
                departure_occupancy_status: None,
                schedule_relationship: None,
                stop_time_properties: None,
            })
            .collect::<Vec<gtfs_rt::trip_update::StopTimeUpdate>>();

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
            serde_json::value::Value::String(x) => Some(x.clone()),
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

    let origin_local_time = origin_departure(&origin_time_string, &origin_tz);

    let origin_weekday = origin_local_time.weekday();

    let trip_id: Option<String> = match trip_name {
        Some(x) => {
            let hashmapresults = trip_name_to_id_hashmap.get(&x);

            match hashmapresults {
                Some(hashmapresults) => {
                    match hashmapresults.len() {
                        1 => Some(hashmapresults[0].clone()),
                        _ => {
                            let possible_results = hashmapresults
                                .iter()
                                .filter(|trip_id_candidate| {
                                    let trip = gtfs.trips.get(trip_id_candidate.clone()).unwrap();

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
        trip_id: trip_id,
        route_id: route_id,
        direction_id: None,
        start_time: None,
        start_date: None,
        schedule_relationship: None,
    };

    gtfs_rt::FeedEntity {
        alert: None,
        id: id.unwrap(),
        is_deleted: Some(false),
        shape: None,
        trip_update: Some(gtfs_rt::TripUpdate {
            vehicle: None,
            trip: trip_desc.clone(),
            timestamp: timestamp,
            delay: None,
            stop_time_update: arrivals,
            trip_properties: None,
        }),
        vehicle: Some(gtfs_rt::VehiclePosition {
            stop_id: None,
            current_status: None,
            timestamp: timestamp,
            congestion_level: None,
            occupancy_status: None,
            occupancy_percentage: None,
            multi_carriage_details: vec![],
            current_stop_sequence: None,
            vehicle: None,
            trip: Some(trip_desc.clone()),
            position: Some(gtfs_rt::Position {
                speed: speed,
                odometer: None,
                bearing: bearing,
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

fn convert_12_to_24_hour(hour: u8, pm: bool) -> u8 {
    match pm {
        false => match hour {
            12 => 0,
            _ => hour + 12,
        },
        true => match hour {
            12 => 12,
            _ => hour + 12
        },
    }
}

fn tz_str_to_tz(tz: &str) -> Option<chrono_tz::Tz> {
    match tz {
        "E" => Some(chrono_tz::America::New_York),
        "M" => Some(chrono_tz::America::Denver),
        "P" => Some(chrono_tz::America::Los_Angeles),
        "C" => Some(chrono_tz::America::Chicago),
        "A" => Some(chrono_tz::America::Phoenix),
        _ => None,
    }
}

//for arrivals and departures, does not parse PM or AM.
fn time_and_tz_to_unix(timestamp_text: &String, tz: &String) -> Option<i32> {
    // tz: String like "P", "C", "M", or "E"
    //time: "12/11/2023 17:36:00"
    let parts = timestamp_text.split(" ").collect::<Vec<&str>>();

    let date_parts = parts[0]
        .split("/")
        .map(|x| x.parse::<i32>().unwrap())
        .collect::<Vec<i32>>();

    let time_parts = parts[1]
        .split(":")
        .map(|x| x.parse::<u8>().unwrap())
        .collect::<Vec<u8>>();

    let native_dt = NaiveDate::from_ymd_opt(
        date_parts[2],
        date_parts[0].try_into().unwrap(),
        date_parts[1].try_into().unwrap(),
    )
    .unwrap()
    .and_hms_opt(
        time_parts[0].into(),
        time_parts[1].into(),
        time_parts[2].into(),
    )
    .unwrap();

    let local_time_representation = tz_str_to_tz(tz)
        .unwrap()
        .from_local_datetime(&native_dt)
        .unwrap();

    Some(local_time_representation.timestamp().try_into().unwrap())
}

//for origin departure conversion to local time representation
pub fn origin_departure(timestamp_text: &str, tz: &str) -> chrono::DateTime<chrono_tz::Tz> {
    let parts = timestamp_text.split(" ").collect::<Vec<&str>>();

    let date_parts = parts[0]
        .split("/")
        .map(|x| x.parse::<i32>().unwrap())
        .collect::<Vec<i32>>();

    let time_parts = parts[1]
        .split(":")
        .map(|x| x.parse::<u8>().unwrap())
        .collect::<Vec<u8>>();

    let is_pm = parts[2] == "PM";

    let native_d = NaiveDate::from_ymd_opt(
        date_parts[2],
        date_parts[0].try_into().unwrap(),
        date_parts[1].try_into().unwrap(),
    )
    .unwrap();

    //println!("{}, {:?}, {}",convert_12_to_24_hour(time_parts[0], is_pm), time_parts, parts[2]);
    
    let native_dt = native_d.and_hms_opt(
        convert_12_to_24_hour(time_parts[0], is_pm).into(),
        time_parts[1].into(),
        time_parts[2].into(),
    )
    .unwrap();

    let local_time_representation = tz_str_to_tz(tz)
        .unwrap()
        .from_local_datetime(&native_dt)
        .unwrap();

    local_time_representation
}

//time is formatted 11/18/2023 4:58:09 PM
pub fn process_timestamp_text(timestamp_text: &str) -> u64 {
    let parts = timestamp_text.split(" ").collect::<Vec<&str>>();

    let date_parts = parts[0]
        .split("/")
        .map(|x| x.parse::<i32>().unwrap())
        .collect::<Vec<i32>>();

    let time_parts = parts[1]
        .split(":")
        .map(|x| x.parse::<u8>().unwrap())
        .collect::<Vec<u8>>();

    let is_pm = parts[2] == "PM";

    let native_dt = NaiveDate::from_ymd_opt(
        date_parts[2],
        date_parts[0].try_into().unwrap(),
        date_parts[1].try_into().unwrap(),
    )
    .unwrap()
    .and_hms_opt(
        convert_12_to_24_hour(time_parts[0], is_pm).into(),
        time_parts[1].into(),
        time_parts[2].into(),
    )
    .unwrap();

    let newyorktime = chrono_tz::America::New_York
        .from_local_datetime(&native_dt)
        .unwrap();

    newyorktime.timestamp().try_into().unwrap()
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
            for feedentity in joined_res.unified_feed.entity {
                vehicles.push(feedentity.clone());
                trips.push(feedentity.clone());
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

            let decrypted_string = amtk::decrypt(raw_data.text().await.unwrap().as_str()).unwrap();

            let geojson: geojson::GeoJson = decrypted_string.parse::<geojson::GeoJson>().unwrap();
            let featurescollection: FeatureCollection =
                FeatureCollection::try_from(geojson).unwrap();

            //println!("Successfully decrypted");
            //println!("{}", decrypted_string);
            Ok(GtfsAmtrakResultsJoined {
                unified_feed: gtfs_rt::FeedMessage {
                    entity: featurescollection
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
        let client = reqwest::Client::new();

        println!("download and process amtrak gtfs file");

        let gtfs = Gtfs::from_url_async("https://content.amtrak.com/content/gtfs/GTFS.zip")
            .await
            .unwrap();

        println!("amtrak download finished");

        let amtrak_results = fetch_amtrak_gtfs_rt_joined(&gtfs, &client).await;

        assert!(amtrak_results.is_ok());

        println!("{:#?}", amtrak_results.unwrap());
    }
}
