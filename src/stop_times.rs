use chrono::NaiveDate;
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
    train_numbers: &Vec<(String, NaiveDate)>,
) -> HashMap<(String, NaiveDate), RootTripData> {
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
    starting_date: &NaiveDate,
    client: &reqwest::Client,
) -> Result<RootTripData, Box<dyn std::error::Error>> {
    let url = format!(
        "https://amtraktime.catenarymaps.org/amtrakstatus?trainnum={}&starting_date={}",
        train_number,
        starting_date.format("%Y-%m-%d")
    );

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("referer", "https://www.amtrak.com/".parse()?);
    headers.insert("Referer", "https://www.amtrak.com/".parse()?);
    headers.insert("Cookie", "_pin_unauth=dWlkPVlqSXpNekUyWTJJdE1tTm1aaTAwWWpReExXSTBPR010TURobE5ETmpObVJtWTJSaQ; _gcl_au=1.1.1528459639.1710441762; _ga=GA1.2.974994175.1710441764; OptanonAlertBoxClosed=2024-03-14T18:42:47.755Z; at_check=true; AMCVS_2909B74F57B49A137F000101%40AdobeOrg=1; _gid=GA1.2.568845230.1715400190; ats-cid-AM-141529-sid=06389637; mdLogger=false; s_cc=true; NITAlme={%22Window.Position%22:{%22xPos%22:436%2C%22yPos%22:285}}; _abck=2255F8BDF256664125E83E3544315452~0~YAAQhPfVF9zFUW6PAQAAbCL4cAtC9Wzf14C6qaRsnKloyl0094tmODXq4v7MhnAwfh+8RCEhz2HyaiIR5kdKlYSs6dpQA+RSqe8iHFBGq4/sMtH+t8M4QT9d/kZdzuPgCHuvz14Zl3A7ff6BZqemxujprlnDzqcAxjLe0Ma3BMnbii9rFawv5A9O9imQ4x6jPmDZq8F09l0EDw62ihTjnDovcwuRiRM57XRRj2N3zcNbIEG70GRtDg8jXmZ1k/lTz5Fo7jpZRULDhXeZgEvvSZ2u3tU/RK5ydwccSMTbtIBD2Pr60DnXwcwAJKrI8YgbwN5rNwwCffMlyiMIKSqdOI59mpDZh09shelyU4xkL+9ouTyFyy/WsGAetqNy0ubNvDzhlmqlsvmSQjPJ9gNCoCv6XPJgnsx8~-1~-1~-1; ak_bmsc=ABEA7794C469BFA522D9ED78105E7275~000000000000000000000000000000~YAAQhPfVFxDGUW6PAQAAhS74cBcL0hze6VqpecqPmaHkhZ7gopAUKLx1sNfV0osUU+Irovsi1GJBV16JzlGu5Uw5AZlwpZEFsiUd1k8nEyzTgGHbz2a1AAjtY0KqEebPYq3pD/zyIRV9ZMKF66DjhcpQ7PC5lWTbNWpoY57M1NpH6+5AkKeCQJupybhHKJt/1d9cgCmKXr1KZix2+0y2D6BljRhclUg2i/DUu4aZx8posKosbPpexRV2WeMZKteorxghs37YuGeR5JWcWucP9yLvo+26AskLI7tVO8W35MnNkQU3v3AKK85STuGsEUoE8BdCSTQjeJG9M8mWnh8A9pVzGBK2JvTyyzFfhdZ7wKUta+Ap9PBba4GJJI4IDFbVxEDVfSu085TLWFX+D3jpOjHQSJLqE/R4Y8ZAv8C8fQYSzHx4U3QEoJm8zoAIXwEA0CLol4dxYRaswDmDieeP1NSlhmLKYrr0w8dM; AMCV_2909B74F57B49A137F000101%40AdobeOrg=179643557%7CMCMID%7C78937028747888993960383752020801057418%7CMCAAMLH-1716192605%7C9%7CMCAAMB-1716192605%7CRKhpRz8krg2tLO6pguXWp5olkAcUniQYPHaMWWgdJ3xzPWQmdj0y%7CMCOPTOUT-1715595005s%7CNONE%7CvVersion%7C5.5.0%7CMCIDTS%7C19857; kampyleUserSession=1715587966157; kampyleUserSessionsCount=72; bm_sz=E5A4BB697CB0905CD567E70875D6D6A9~YAAQ2NfOF/dJ02yPAQAAnGkEcRc13z61/4XrakJaBnt1LrulfmS1fSspVX89pzQ+q6T70tsSou/N4F1p7mJdmPYlYW+XXnfRf8wzBmfeW/AmLqU416vN31WoQOvQ2MEVKVo5WhfBuI7ELje6HlYLeIHxafpiO9tJkrdIYV3RjOGVyZrlDbDpiWerWhvB5Ic4WvC7xZgOX4TowbQ1KT1DL0/x4plrzwbwY4nf0tP/xw4yvY6EW3zF43VvCJCTBTOSyi+EqDmmd8sSZTmLr2WgNp0mndg9NSlhGhfM/WMWrbWioEACq35SDaMnK9snKr0+pYJPnUIprvfhQrovpRnIcJmR1DzxXlmFnBX27kA3uciKsnru1fjbwF6+MNulIO75mDQ0PGQoxvJNacTSqhY79mvIWxjbz/fCqYyfqdfvTPTorkPD/I/Y7tBaKMZZHhi1HvTOC3Mj9ZVXfzD1kJ1cMlToa0oI~3688005~3491384; mbox=PC#d3f1cbd47dfc4b20bed06dd0b61af0a9.35_0#1778832867|session#9ed2fa89cec742c48168df52beecf34b#1715589927; _rdt_uuid=1710441762340.9d0ffc4b-41d0-468b-95dd-1f4b56c8a0c9; _uetsid=611621500f4b11ef8c1f5d08064b5c53; _uetvid=4ba077e0170011eeb09563ba5684d7ce; _derived_epik=dj0yJnU9LU1HRVpnUVpuMkdrZ2NMZ2lZNkRwMnh6TG1BQzcyclkmbj00RmN4eGJvNlRkVEJjWlF6SG9oZXhnJm09ZiZ0PUFBQUFBR1pCeS1NJnJtPWYmcnQ9QUFBQUFHWkJ5LU0mc3A9Mg; OptanonConsent=isGpcEnabled=0&datestamp=Mon+May+13+2024+01%3A14%3A28+GMT-0700+(Pacific+Daylight+Time)&version=202311.1.0&isIABGlobal=false&hosts=&consentId=abaf7e8b-d120-4c52-af3b-c837b6e94c06&interactionCount=1&landingPath=NotLandingPage&groups=C0001%3A1%2CC0004%3A1%2CC0002%3A1%2CC0003%3A1&geolocation=US%3BCA&AwaitingReconsent=false&browserGpcFlag=0; _gat_gtag_UA_23218774_1=1; gpv_v9=Amtrak-Reservations-TrainStatus; s_sq=%5B%5BB%5D%5D; kampyleSessionPageCounter=5; bm_sv=1336275B6B9ADCE3E0278E6E21D49697~YAAQ2NfOFwtN02yPAQAADoEEcRdsyZyvVikah3MWMcW7BRpMRCbjOUv/c6sqK6bZ8kDFdsFlTUH3Q/4rTgS9mMG2eErJR5AgmZzXaxHOYurF8UXzR6uBdS1ILMNcmB6w2A19Sk87hc8LOxkNgP4A6emd0SbuZMKL21A3kY3/+qrtOS3PxhM/oRS4LS8FVMyd5s5NguMKT1koG0suIQQwngRNgx1uJhzdYeBIialdII5KUe89gRIeHUfzHI0KMbD/Fg==~1; s_getNewRepeat=1715588078197-Repeat; s_ptc=0.07%5E%5E0.00%5E%5E0.00%5E%5E0.00%5E%5E0.31%5E%5E0.01%5E%5E5.92%5E%5E0.02%5E%5E6.34; _abck=2255F8BDF256664125E83E3544315452~-1~YAAQl2rcF08Bx1aPAQAAnocacQujSiPs6ljfmFQTP/GyUICKZTtIp+Aj3KM5RJm9s7jtoYNhLc9v9DggQ1u+z0jKr7UbZtAuV7dlETVG66PPDGrNdITVJgr8xPWR3bSbfsVKMEM+5rPgrbTxfv0fWNuquEuiMj5eS/Oge0cOrCXsqsQE7lAVIp5FMDIgF/+IndSJyamv3NXe7KzQWsITCqN1Ku3ufZ2APyyFRMPcfPrxG6NFt2HMDzBXl1XQ/Ka/IslffNr4YDTh6Y/WhNGDfX4Fm0LzH+aqW4CdaqgVweTh03yiARZ8YXJJsd8+SmXdfmBztnQOl/3xeulso0dwHLVb8po/BhBi28SVjmEdKtM1sJBwOXOSG4YK/lFxDhtIvVEJuVVs00T5/sPoM7oeRzgfW4/bf9A2~0~-1~-1; ak_bmsc=ABEA7794C469BFA522D9ED78105E7275~000000000000000000000000000000~YAAQl2rcF1ABx1aPAQAAnocacRdBxJQvtgjSgbq52cxXULs8CeqFuncgrjGBD1u2h3VS3AdBPJulRhbmTMEj4aW7CB2FzqrmtzyZ/+H1YRyxndfbpItJugNBUOpFexnLSGC8aNr8H/8MCrUGI5sib/Tm15gc6IITpQ17kNQdbzOUTcPQL4ODPTo0wLjL9Hx9CWousW5awZsNIsYvmL3RW3m+4ImLtartCIWb6OedC2bHrrZEQmH+C0GLWsGdN5b+8xuYZ+AnFNoBY74n88BMy0wDh26ppmVB7aayxkmb0a2z1fJkIVJwdIAGVan53TuEHscW0jwEPrAclLlnUVZNHCvqlNNmsmzDXYFmH3BS/U7ErxC/JBcebJ6luO+vCw+rnYstF+gSB8+QHPMX98blQ7fvZDaTwbKdbonwBqjXJlWM6D4K8EmbnrhqCv9Y2AdPSV8Sme/c2oYbwnAle3c=; bm_sv=1336275B6B9ADCE3E0278E6E21D49697~YAAQl2rcF1EBx1aPAQAAnocacReCxi1hyMZHGWJlOrxi/1yYfxKzEG8d1bA+XdIJA4AVxiJXWp67zHYDdOb4NWInkLOtKHwWyNBjQfRrVSCHkm0zj3eNWZbLZge3Wv+63m54ldj7eXYq5WhmkLCHpcvpdqFuHx+Z08DrjNCzT9ql83O0RfHWHvqISb6Xuu6LN28PSyI8CMNILDF1DC9WWAccOD5pX2PMEbb73gl9YCr4pe22QoUFMMiFAxOxPdiIbQ==~1".parse()?);
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
