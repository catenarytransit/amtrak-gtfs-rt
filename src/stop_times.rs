use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootTripData {
    pub data: Vec<TripDataEntity>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TripDataEntity {
    pub id: String,
    pub travel_service: TravelService,
    pub status_summary: StatusSummary,
    pub stops: Vec<AmtrakStopTime>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TravelService {
    pub id: String,
    pub number: String,
    pub date: String,
    #[serde(rename = "type")]
    pub type_field: Type,
    pub name: Name,
    pub operator: Operator,
    pub origin: StationMetadata,
    pub destination: StationMetadata,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Type {
    pub code: String,
    pub description: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Name {
    pub code: String,
    pub description: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Operator {
    pub code: String,
    pub description: String,
    pub number: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationMetadata {
    pub code: String,
    pub name: String,
    pub facility: String,
    pub time_zone: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusSummary {
    pub display_message: Option<String>,
    pub location_info: Option<LocationInfo>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationInfo {
    pub last_known_location_code: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmtrakStopTime {
    pub id: String,
    pub stop_number: i64,
    pub station: Station,
    pub departure: Option<DepartureOrArrival>,
    pub arrival: Option<DepartureOrArrival>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Station {
    pub code: String,
    pub name: String,
    pub facility: Option<String>,
    pub time_zone: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepartureOrArrival {
    pub schedule: Schedule,
    pub status_info: StatusInfo,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub date_time: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusInfo {
    pub status: String,
    pub display_status: String,
    pub display_message: String,
    pub date_time_type: Option<String>,
    pub date_time: Option<String>,
    pub auto_calculated: Option<bool>,
    pub delay: Option<String>,
    pub as_of: String,
}

pub async fn query_all_trips_simultaniously(
    train_numbers: &Vec<(String, Date)>,
) -> HashMap<(String, Date), RootTripData> {
    let client = reqwest::Client::new();

    let mut futures = vec![];

    for (train_number, starting_date) in train_numbers {
        let future = get_stop_times(train_number, starting_date, &client);
        futures.push(future);
    }

    let results = futures::future::join_all(futures).await;

    let mut result_map = HashMap::new();

    for (i, result) in results.into_iter().enumerate() {
        let train_number = &train_numbers[i];
        match result {
            Ok(data) => {
                result_map.insert(train_number.clone(), data);
            }
            Err(e) => {
                eprintln!(
                    "Error fetching data for train number {:?}: {}",
                    train_number, e
                );
            }
        }
    }

    result_map
}

pub async fn get_stop_times(
    train_number: &str,
    starting_date: &Date,
    client: &reqwest::Client,
) -> Result<RootTripData, Box<dyn std::error::Error>> {
    let url = format!(
        "https://amtraktime.catenarymaps.org/amtrakstatus?trainnum={}&starting_date={}",
        train_number,
        starting_date.strftime("%Y-%m-%d")
    );

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("referer", "https://www.amtrak.com/".parse()?);
    headers.insert("Referer", "https://www.amtrak.com/".parse()?);
    // Session credentials must come from deployment configuration, never source.
    if let Ok(cookie) = std::env::var("AMTRAK_COOKIE") {
        headers.insert(reqwest::header::COOKIE, cookie.parse()?);
    }
    headers.insert("origin", "https://amtrak.inq.com".parse()?);
    headers.insert("Accept", "application/json, text/plain, */*".parse()?);

    let response = client.get(&url).headers(headers).send().await?;
    let response_text = response.text().await?;
    let deserialized = serde_json::from_str::<RootTripData>(&response_text);

    if let Err(deserialized) = &deserialized {
        eprintln!(
            "Error deserializing response for train number {}: {}",
            train_number, deserialized
        );
        //  eprintln!("Response text: {}", response_text);
    }

    let deserialized = deserialized?;

    //  println!("GOT INFO: {:?}", deserialized);

    Ok(deserialized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize() {
        let example_data = r#"{"data": [{"id": "059520240513","travelService": {"id": "059520240513","number": "595","date": "2024-05-13","type": {"code": "TRN","description": "Intercity Train"},"name": {"code": "PCFS","description": "Pacific Surfliner"},"operator": {"code": "AMTK","description": "Amtrak","number": "0595"},"origin": {"code": "SAN","name": "San Diego, CA","facility": "Santa Fe Depot","timeZone": "America/Los_Angeles"},"destination": {"code": "LAX","name": "Los Angeles, CA","facility": "Union Station","timeZone": "America/Los_Angeles"}},"statusSummary": {"displayMessage": "ONTIME AT SAN","locationInfo": {"lastKnownLocationCode": "SAN","latitude": 32.716169,"longitude": -117.169576}},"stops": [{"id": "059520240513SAN","stopNumber": 1,"station": {"code": "SAN","name": "San Diego, CA","facility": "Santa Fe Depot","timeZone": "America/Los_Angeles"},"departure": {"schedule": {"dateTime": "2024-05-13T21:01:00-07:00"},"statusInfo": {"status": "ON TIME","displayStatus": "On Time","displayMessage": "On Time","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T21:01:00-07:00","autoCalculated": false,"delay": "PT0S","asOf": "2024-05-13T12:49:00-07:00"}}},{"id": "059520240513OLT","stopNumber": 2,"station": {"code": "OLT","name": "San Diego, CA","facility": "Old Town Transportation Center","timeZone": "America/Los_Angeles"},"arrival": {"schedule": {"dateTime": "2024-05-13T21:09:00-07:00"},"statusInfo": {"status": "ON TIME","displayStatus": "On Time","displayMessage": "On Time","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T21:09:00-07:00","autoCalculated": true,"delay": "PT0S","asOf": "2024-05-13T00:49:00-07:00"}},"departure": {"schedule": {"dateTime": "2024-05-13T21:10:00-07:00"},"statusInfo": {"status": "ON TIME","displayStatus": "On Time","displayMessage": "On Time","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T21:10:00-07:00","autoCalculated": true,"delay": "PT0S","asOf": "2024-05-13T00:49:00-07:00"}}},{"id": "059520240513SOL","stopNumber": 3,"station": {"code": "SOL","name": "Solana Beach, CA","timeZone": "America/Los_Angeles"},"arrival": {"schedule": {"dateTime": "2024-05-13T21:39:00-07:00"},"statusInfo": {"status": "ON TIME","displayStatus": "On Time","displayMessage": "On Time","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T21:39:00-07:00","autoCalculated": true,"delay": "PT0S","asOf": "2024-05-13T00:49:00-07:00"}},"departure": {"schedule": {"dateTime": "2024-05-13T21:40:00-07:00"},"statusInfo": {"status": "DELAYED","displayStatus": "Now 09:41PM","displayMessage": "1 Minute Late","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T21:41:00-07:00","autoCalculated": true,"delay": "PT1M","asOf": "2024-05-13T00:49:00-07:00"}}},{"id": "059520240513OSD","stopNumber": 4,"station": {"code": "OSD","name": "Oceanside, CA","timeZone": "America/Los_Angeles"},"arrival": {"schedule": {"dateTime": "2024-05-13T21:55:00-07:00"},"statusInfo": {"status": "DELAYED","displayStatus": "Now 09:59PM","displayMessage": "4 Minutes Late","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T21:59:00-07:00","autoCalculated": true,"delay": "PT4M","asOf": "2024-05-13T00:49:00-07:00"}},"departure": {"schedule": {"dateTime": "2024-05-13T21:57:00-07:00"},"statusInfo": {"status": "DELAYED","displayStatus": "Now 10:01PM","displayMessage": "4 Minutes Late","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T22:01:00-07:00","autoCalculated": true,"delay": "PT4M","asOf": "2024-05-13T00:49:00-07:00"}}},{"id": "059520240513SNC","stopNumber": 5,"station": {"code": "SNC","name": "San Juan Capistrano, CA","timeZone": "America/Los_Angeles"},"arrival": {"schedule": {"dateTime": "2024-05-13T22:32:00-07:00"},"statusInfo": {"status": "ON TIME","displayStatus": "On Time","displayMessage": "On Time","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T22:32:00-07:00","autoCalculated": true,"delay": "PT0S","asOf": "2024-05-13T00:49:00-07:00"}},"departure": {"schedule": {"dateTime": "2024-05-13T22:34:00-07:00"},"statusInfo": {"status": "ON TIME","displayStatus": "On Time","displayMessage": "On Time","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T22:34:00-07:00","autoCalculated": true,"delay": "PT0S","asOf": "2024-05-13T00:49:00-07:00"}}},{"id": "059520240513IRV","stopNumber": 6,"station": {"code": "IRV","name": "Irvine, CA","timeZone": "America/Los_Angeles"},"arrival": {"schedule": {"dateTime": "2024-05-13T22:48:00-07:00"},"statusInfo": {"status": "ON TIME","displayStatus": "On Time","displayMessage": "On Time","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T22:48:00-07:00","autoCalculated": true,"delay": "PT0S","asOf": "2024-05-13T00:49:00-07:00"}},"departure": {"schedule": {"dateTime": "2024-05-13T22:49:00-07:00"},"statusInfo": {"status": "ON TIME","displayStatus": "On Time","displayMessage": "On Time","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T22:49:00-07:00","autoCalculated": true,"delay": "PT0S","asOf": "2024-05-13T00:49:00-07:00"}}},{"id": "059520240513SNA","stopNumber": 7,"station": {"code": "SNA","name": "Santa Ana, CA","timeZone": "America/Los_Angeles"},"arrival": {"schedule": {"dateTime": "2024-05-13T22:59:00-07:00"},"statusInfo": {"status": "ON TIME","displayStatus": "On Time","displayMessage": "On Time","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T22:59:00-07:00","autoCalculated": true,"delay": "PT0S","asOf": "2024-05-13T00:49:00-07:00"}},"departure": {"schedule": {"dateTime": "2024-05-13T23:01:00-07:00"},"statusInfo": {"status": "ON TIME","displayStatus": "On Time","displayMessage": "On Time","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T23:01:00-07:00","autoCalculated": true,"delay": "PT0S","asOf": "2024-05-13T00:49:00-07:00"}}},{"id": "059520240513ANA","stopNumber": 8,"station": {"code": "ANA","name": "Anaheim, CA","timeZone": "America/Los_Angeles"},"arrival": {"schedule": {"dateTime": "2024-05-13T23:08:00-07:00"},"statusInfo": {"status": "ON TIME","displayStatus": "On Time","displayMessage": "On Time","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T23:08:00-07:00","autoCalculated": true,"delay": "PT0S","asOf": "2024-05-13T00:49:00-07:00"}},"departure": {"schedule": {"dateTime": "2024-05-13T23:10:00-07:00"},"statusInfo": {"status": "ON TIME","displayStatus": "On Time","displayMessage": "On Time","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T23:10:00-07:00","autoCalculated": true,"delay": "PT0S","asOf": "2024-05-13T00:49:00-07:00"}}},{"id": "059520240513FUL","stopNumber": 9,"station": {"code": "FUL","name": "Fullerton, CA","timeZone": "America/Los_Angeles"},"arrival": {"schedule": {"dateTime": "2024-05-13T23:17:00-07:00"},"statusInfo": {"status": "ON TIME","displayStatus": "On Time","displayMessage": "On Time","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T23:17:00-07:00","autoCalculated": true,"delay": "PT0S","asOf": "2024-05-13T00:49:00-07:00"}},"departure": {"schedule": {"dateTime": "2024-05-13T23:18:00-07:00"},"statusInfo": {"status": "ON TIME","displayStatus": "On Time","displayMessage": "On Time","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T23:18:00-07:00","autoCalculated": true,"delay": "PT0S","asOf": "2024-05-13T00:49:00-07:00"}}},{"id": "059520240513LAX","stopNumber": 10,"station": {"code": "LAX","name": "Los Angeles, CA","facility": "Union Station","timeZone": "America/Los_Angeles"},"arrival": {"schedule": {"dateTime": "2024-05-13T23:57:00-07:00"},"statusInfo": {"status": "EARLY","displayStatus": "Now 11:49PM","displayMessage": "8 Minutes Early","dateTimeType": "ESTIMATE","dateTime": "2024-05-13T23:49:00-07:00","autoCalculated": true,"delay": "PT-8M","asOf": "2024-05-13T00:49:00-07:00"}}}]}]}"#;

        let deserialized: RootTripData = serde_json::from_str(example_data).unwrap();

        assert_eq!(deserialized.data.len(), 1);

        let trip_data = &deserialized.data[0];

        assert_eq!(trip_data.id, "059520240513");
    }
}
