use std::time::{Duration, SystemTime};
use std::time::UNIX_EPOCH;
use geojson::FeatureCollection;
use geojson::GeoJson;
use chrono_tz::Tz;
use chrono_tz::UTC;
use chrono::{NaiveDate, NaiveTime, NaiveDateTime};
use chrono::DateTime;
use chrono::TimeZone;

pub struct GtfsAmtrakResults {
    pub vehicle_positions: gtfs_rt::FeedMessage,
    pub trip_updates: gtfs_rt::FeedMessage
}

pub fn make_gtfs_header() -> gtfs_rt::FeedHeader {
    gtfs_rt::FeedHeader {
        gtfs_realtime_version: String::from("2.0"),
        incrementality: None,
        timestamp: Some(SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs()),
    }
}

fn convert_12_to_24_hour(hour: u8, pm: bool) -> u8 {
    match pm {
        false => {
            match hour {
                12 => 0,
                _ => hour + 12
            }
        },
        true => hour + 12
    }
}

//time is formatted 11/18/2023 4:58:09 PM
pub fn process_timestamp_text(timestamp_text: &str) -> u64 {
    let parts = timestamp_text.split(" ").collect::<Vec<&str>>();

    let date_parts = parts[0].split("/").map(|x| x.parse::<i32>().unwrap()).collect::<Vec<i32>>();

    let time_parts = parts[1].split(":").map(|x| x.parse::<u8>().unwrap()).collect::<Vec<u8>>();

    let is_pm = parts[2] == "PM";

    let native_dt = NaiveDate::from_ymd_opt(date_parts[2], date_parts[0].try_into().unwrap(), date_parts[1].try_into().unwrap()).unwrap()
    .and_hms_opt(convert_12_to_24_hour(time_parts[0], is_pm).into(), time_parts[1].into(), time_parts[2].into()).unwrap();

    let newyorktime = chrono_tz::America::New_York.from_local_datetime(&native_dt).unwrap();

    newyorktime.timestamp().try_into().unwrap()
}

pub async fn fetch_amtrak_gtfs_rt(client: &reqwest::Client) -> Result<GtfsAmtrakResults,Box<dyn std::error::Error>> {
        //println!("fetching");
        
        let raw_data = client.get("https://maps.amtrak.com/services/MapDataService/trains/getTrainsData").send().await;
        if raw_data.is_err() {
            println!("Raw data err");
            return Err(Box::new(raw_data.unwrap_err()));
        }
                //println!("Raw data successfully downloaded");

                match amtk::decrypt(raw_data.unwrap().text().await.unwrap().as_str()) {
                    Ok(decrypted_string) => {

                        
                    let geojson: geojson::GeoJson = decrypted_string.parse::<geojson::GeoJson>().unwrap();

                    let featurescollection: FeatureCollection = FeatureCollection::try_from(geojson).unwrap();
                    
                        //println!("Successfully decrypted");
                        //println!("{}", decrypted_string);
                        Ok(GtfsAmtrakResults {
                            vehicle_positions: gtfs_rt::FeedMessage {
                                entity: featurescollection.features.iter().map(|feature| {
                                    let geometry = feature.geometry.as_ref().unwrap();
                                    let point: Option<geojson::PointType> = match geometry.value.clone() {
                                        geojson::Value::Point(x) => Some(x),
                                        _ => None
                                    };

                                    let point = point.unwrap();

                                    let speed: Option<f32> = match feature.properties.as_ref().unwrap().get("Velocity") {
                                        Some(speed_text) => 
                                            match speed_text {
                                                serde_json::value::Value::String(x) => Some(x.as_str().parse::<f32>().unwrap() * 0.2777777),
                                                _ => None
                                            }
                                        ,
                                        _ => None
                                    };

                                    //unix time seconds
                                    let timestamp: Option<u64> = match feature.properties.as_ref().unwrap().get("updated_at") {
                                        Some(timestamp_text) => 
                                            match timestamp_text {
                                                serde_json::value::Value::String(timestamp_text) => Some(process_timestamp_text(timestamp_text)),
                                                _ => None
                                            }
                                        ,
                                        _ => None
                                    };

                                    let trip_id: Option<String> = match feature.properties.as_ref().unwrap().get("TrainNum") {
                                        Some(a) => 
                                            match a {
                                                serde_json::value::Value::String(x) => Some(x.clone()),
                                                _ => None
                                            }
                                        ,
                                        _ => None
                                    };

                                    let route_id: Option<String> = match feature.properties.as_ref().unwrap().get("RouteName") {
                                        Some(a) => 
                                            match a {
                                                serde_json::value::Value::String(x) => Some(x.clone()),
                                                _ => None
                                            }
                                        ,
                                        _ => None
                                    };

                                    let id: Option<String> = match feature.properties.as_ref().unwrap().get("TrainNum") {
                                        Some(a) => 
                                            match a {
                                                serde_json::value::Value::String(x) => Some(x.clone()),
                                                _ => None
                                            }
                                        ,
                                        _ => None
                                    };

                                    let bearing: Option<f32> = match feature.properties.as_ref().unwrap().get("Heading") {
                                        Some(bearing_text) => 
                                            match bearing_text {
                                                serde_json::value::Value::String(x) => match x.as_str() {
                                                    "N" => Some(0.01),
                                                    "NE" => Some(45.0),
                                                    "E" => Some(90.0),
                                                    "SE" => Some(135.0),
                                                    "S" => Some(180.0),
                                                    "SW" => Some(225.0),
                                                    "W" => Some(270.0),
                                                    "NW" => Some(315.0),
                                                    _ => None
                                                },
                                                _ => None
                                            }
                                        ,
                                        _ => None
                                    };

                                    gtfs_rt::FeedEntity {
                                        alert: None,
                                        id: id.unwrap(),
                                        is_deleted: Some(false),
                                        shape: None,
                                        trip_update: None,
                                        vehicle: Some(
                                            gtfs_rt::VehiclePosition {
                                                stop_id: None,
                                                current_status: None,
                                                timestamp: timestamp,
                                                congestion_level: None,
                                                occupancy_status: None,
                                                occupancy_percentage: None,
                                                multi_carriage_details: vec![],
                                                current_stop_sequence: None,
                                                vehicle: None,
                                                trip: Some(gtfs_rt::TripDescriptor {
                                                    trip_id: trip_id,
                                                    route_id: route_id,
                                                    direction_id: None,
                                                    start_time: None,
                                                    start_date: None,
                                                    schedule_relationship: None
                                                }),
                                                position: Some(
                                                    gtfs_rt::Position {
                                                        speed: speed,
                                                        odometer: None,
                                                        bearing: bearing,
                                                        latitude: point[1] as f32,
                                                        longitude: point[0] as f32
                                                    }
                                                )
                                            }
                                        )
                                    }
                                }).collect::<Vec<gtfs_rt::FeedEntity>>(),
                                header: make_gtfs_header()
                            },
                            trip_updates: gtfs_rt::FeedMessage {
                                entity: vec![],
                                header: make_gtfs_header()
                            },
                        })
                    },
                    Err(err) => {
                        Err(Box::new(err))
                    }
                }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_amtrak() {
        println!("running test");
        let client = reqwest::Client::new();

        let amtrak_results = fetch_amtrak_gtfs_rt(&client).await;

        assert!(amtrak_results.is_ok());
    }
}