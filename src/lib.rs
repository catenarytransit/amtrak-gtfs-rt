use std::time::{Duration, SystemTime};
use std::time::UNIX_EPOCH;

pub struct GtfsAmtrakResults {
    vehicle_positions: gtfs_rt::FeedMessage,
    trip_updates: gtfs_rt::FeedMessage
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
        println!("fetching");
        
        let raw_data = client.get("https://maps.amtrak.com/services/MapDataService/trains/getTrainsData").send().await;

        match raw_data {
            Ok(raw_data) => {

                println!("Raw data successfully downloaded");

                match amtk::decrypt(raw_data.text().await.unwrap().as_str()) {
                    Ok(decrypted_string) => {
                        println!("Successfully decrypted");
                        println!("{}", decrypted_string);
                        Ok(GtfsAmtrakResults {
                            vehicle_positions: gtfs_rt::FeedMessage {
                                entity: vec![],
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