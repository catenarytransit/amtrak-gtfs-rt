use std::time::{Duration, SystemTime};
use std::time::UNIX_EPOCH;
use geojson::FeatureCollection;
use geojson::GeoJson;
use chrono_tz::Tz;
use chrono_tz::UTC;

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

pub async fn fetch_amtrak_gtfs_rt(client: &reqwest::Client) -> Result<GtfsAmtrakResults,Box<dyn std::error::Error>> {
        //println!("fetching");

        let tz: Tz = "America/Los_Angeles".parse().unwrap();
        
        let raw_data = client.get("https://maps.amtrak.com/services/MapDataService/trains/getTrainsData").send().await;

        match raw_data {
            Ok(raw_data) => {

                //println!("Raw data successfully downloaded");

                match amtk::decrypt(raw_data.text().await.unwrap().as_str()) {
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
                                    let timestamp: Option<u64> = match feature.properties.as_ref().unwrap().get("LastValTS") {
                                        Some(timestamp_text) => 
                                            match timestamp_text {
                                                serde_json::value::Value::String(x) => Some(SystemTime::now()
                                                .duration_since(SystemTime::UNIX_EPOCH)
                                                .unwrap()
                                                .as_secs()),
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
            },
            Err(err) => {
                println!("Raw data err");

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