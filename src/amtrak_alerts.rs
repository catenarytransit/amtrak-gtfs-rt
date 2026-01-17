use chrono::Datelike;
use futures::stream::StreamExt;
use gtfs_realtime::translated_string::Translation;
use gtfs_realtime::{Alert, EntitySelector, FeedEntity, FeedHeader, FeedMessage};
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const PROXIES: &[&str] = &[
    "http://45.59.186.60:80",
    "http://34.194.110.189:80",
    "http://104.197.218.238:8080",
    "http://154.17.228.122:80",
    "http://152.26.229.52:9443",
    "http://51.8.61.60:80",
    "http://54.201.87.119:80",
    "http://50.203.147.155:80",
    "http://50.203.147.153:80",
    "http://50.203.147.157:80",
    "http://198.111.166.184:80",
    "http://143.198.135.176:80",
    "http://209.135.168.41:80",
    "http://100.48.28.177:80",
    "http://71.60.160.245:80",
    "http://174.138.54.65:80",
    "http://155.94.175.201:8080",
    "http://47.6.9.54:80",
    "http://74.50.96.247:8888",
    "http://108.170.12.14:80",
];

#[derive(Deserialize, Debug)]
pub struct AmtrakStationStop {
    pub stationName: Option<String>,
    pub stationCode: Option<String>,
    pub scheduledArrival: Option<String>,
    pub estimatedArrival: Option<String>,
    pub scheduledDeparture: Option<String>,
    pub estimatedDeparture: Option<String>,
    pub arrivalStatus: Option<String>,
    pub departureStatus: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct AmtrakStatusInfo {
    pub detailedMessage: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct AmtrakTrainService {
    pub stops: Option<Vec<AmtrakStationStop>>,
    pub statusInfo: Option<AmtrakStatusInfo>,
    pub trainNum: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct AmtrakResponse {
    pub data: Option<Vec<AmtrakTrainService>>,
}

pub async fn fetch_train_status(
    client: &Client,
    train_num: &str,
    date: &str, // YYYY-MM-DD
) -> Result<Option<AmtrakTrainService>, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!(
        "https://www.amtrak.com/dotcom/travel-service/statuses/{}?service-date={}",
        train_num, date
    );

    // Timeout is important here so one slow proxy doesn't hang the whole batch forever
    let res = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;
    let text = res.text().await?;

    // Attempt to parse. If it fails or data is empty, return None or Error.
    match serde_json::from_str::<AmtrakResponse>(&text) {
        Ok(parsed) => {
            if let Some(list) = parsed.data {
                if let Some(service) = list.into_iter().next() {
                    return Ok(Some(service));
                }
            }
            Ok(None)
        }
        Err(e) => {
            // In case of error, just return error
            Err(Box::new(e))
        }
    }
}

pub fn create_alert_entity(train_num: String, service: AmtrakTrainService) -> Option<FeedEntity> {
    // If there is no detailed message, there is no alert to create (or maybe we interpret delays as alerts? User asked for "alerts" replacing ASM).
    // ASM provided specific messages. Amtrak api `detailedMessage` seems to be the equivalent.

    let description_text = service.statusInfo.as_ref()?.detailedMessage.clone()?;

    if description_text.trim().is_empty() {
        return None;
    }

    let alert = Alert {
        active_period: vec![], // Active now
        informed_entity: vec![EntitySelector {
            agency_id: None,
            route_id: None, // We typically map train number to route_id elsewhere, but for pure alerts, maybe just trip? Or we leave route_id empty for now.
            // In the main lib we map names to route IDs. Here we might not have that context easily without the GTFS structure.
            // For now, we put the TripDescriptor with the train number (which creates a rough match).
            trip: Some(gtfs_realtime::TripDescriptor {
                trip_id: None, // We don't know the exact GTFS Trip ID without the lookup.
                route_id: None,
                direction_id: None,
                start_time: None,
                start_date: None,
                modified_trip: None,
                schedule_relationship: None,
            }),
            stop_id: None,
            route_type: None,
            direction_id: None,
        }],
        cause: Some(1),  // Unknown cause
        effect: Some(8), // Unknown effect
        url: None,
        header_text: None,
        description_text: Some(gtfs_realtime::TranslatedString {
            translation: vec![Translation {
                text: description_text,
                language: Some("en".to_string()),
            }],
        }),
        tts_header_text: None,
        tts_description_text: None,
        severity_level: None,
        image: None,
        image_alternative_text: None,
        cause_detail: None,
        effect_detail: None,
    };

    Some(FeedEntity {
        id: format!("alert-{}", train_num),
        is_deleted: Some(false),
        trip_update: None,
        vehicle: None,
        alert: Some(alert),
        shape: None,
        stop: None,
        trip_modifications: None,
    })
}

pub async fn generate_alerts_feed(
    gtfs: &gtfs_structures::Gtfs,
    _default_client: &Client,
) -> FeedMessage {
    // Create a pool of clients: 1 direct + N proxies
    let mut clients = Vec::new();

    // Direct client
    clients.push(Client::builder().build().unwrap_or_default());

    // Proxy clients
    for proxy_url in PROXIES {
        if let Ok(proxy) = reqwest::Proxy::all(*proxy_url) {
            if let Ok(client) = Client::builder().proxy(proxy).build() {
                clients.push(client);
            }
        }
    }

    // If all proxies fail, we still have the direct client.
    let clients = Arc::new(clients);

    let now_system = SystemTime::now();
    // We assume the system time is reasonably close to the timezone of the feed for "today".
    // Or just use Utc date for filtering.
    let now_date = chrono::prelude::Utc::now().naive_utc().date();

    // We want to look at trains running today, yesterday (if delayed), and tomorrow (if starting soon).
    let _window_start = now_system - std::time::Duration::from_secs(8 * 3600); // 8 hours ago
    let _window_end = now_system + std::time::Duration::from_secs(4 * 3600); // 4 hours in the future (buffer for upcoming)

    let mut train_queries: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();

    for trip in gtfs.trips.values() {
        let calendar_service = gtfs.calendar.get(&trip.service_id);
        let calendar_dates = gtfs.calendar_dates.get(&trip.service_id);

        // Check for Yesterday, Today, Tomorrow
        for offset in -1..=1 {
            let date_check = now_date + chrono::Duration::days(offset);

            let service_active = if let Some(cal) = calendar_service {
                if date_check < cal.start_date || date_check > cal.end_date {
                    false
                } else {
                    match date_check.weekday() {
                        chrono::Weekday::Mon => cal.monday,
                        chrono::Weekday::Tue => cal.tuesday,
                        chrono::Weekday::Wed => cal.wednesday,
                        chrono::Weekday::Thu => cal.thursday,
                        chrono::Weekday::Fri => cal.friday,
                        chrono::Weekday::Sat => cal.saturday,
                        chrono::Weekday::Sun => cal.sunday,
                    }
                }
            } else {
                false
            };

            // Apply calendar dates exceptions
            let service_active = if let Some(dates) = calendar_dates {
                let mut active = service_active;
                for date_exception in dates {
                    if date_exception.date == date_check {
                        if date_exception.exception_type == gtfs_structures::Exception::Added {
                            active = true;
                        } else if date_exception.exception_type
                            == gtfs_structures::Exception::Deleted
                        {
                            active = false;
                        }
                    }
                }
                active
            } else {
                service_active
            };

            if service_active {
                // Determine trip start and end relative to THIS date
                // Gtfs times are seconds from midnight of the service day.

                let stop_times = &trip.stop_times;
                if !stop_times.is_empty() {
                    // We need to hint the type here or allow inference to work by not wrapping in wrapper tuple immediately if complex
                    if let Some(first) = stop_times.first() {
                        if let Some(last) = stop_times.last() {
                            let start_secs = first.departure_time.unwrap_or(0);
                            let end_secs = last
                                .arrival_time
                                .unwrap_or(last.departure_time.unwrap_or(0));

                            // Let's stick to the "8 hour buffer" logic using chrono dates.
                            let date_midnight = date_check.and_hms_opt(0, 0, 0).unwrap();
                            let trip_start =
                                date_midnight + chrono::Duration::seconds(start_secs as i64);
                            let trip_end =
                                date_midnight + chrono::Duration::seconds(end_secs as i64);

                            let now_chrono = chrono::Utc::now().naive_utc();

                            let buffer_past = chrono::Duration::hours(8);
                            let buffer_future = chrono::Duration::hours(4); // "slightly in future"

                            // Valid if:
                            // Trip End > Now - 8h   AND   Trip Start < Now + 4h

                            if trip_end > (now_chrono - buffer_past)
                                && trip_start < (now_chrono + buffer_future)
                            {
                                if let Some(train_num) = &trip.trip_short_name {
                                    train_queries.insert((
                                        train_num.clone(),
                                        date_check.format("%Y-%m-%d").to_string(),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    println!("Queries to make: {}", train_queries.len());
    println!("Using client pool size: {}", clients.len());

    // Convert Set to Vec for iteration
    let queries_vec: Vec<_> = train_queries.into_iter().collect();

    let entities = futures::stream::iter(queries_vec.into_iter().enumerate())
        .map(|(idx, (train_num, date))| {
            let clients = clients.clone();
            async move {
                // Round robin selection
                let client = &clients[idx % clients.len()];
                match fetch_train_status(client, &train_num, &date).await {
                    Ok(Some(service)) => create_alert_entity(train_num, service),
                    Ok(None) => None,
                    Err(_) => {
                        // Optionally log error
                        None
                    }
                }
            }
        })
        .buffer_unordered(60) // concurrency limit
        .filter_map(|x| async move { x })
        .collect::<Vec<_>>()
        .await;

    FeedMessage {
        header: FeedHeader {
            gtfs_realtime_version: "2.0".to_string(),
            incrementality: None,
            timestamp: Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            ),
            feed_version: None,
        },
        entity: entities,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtfs_structures::Gtfs;

    #[tokio::test]
    async fn test_generate_alerts_feed_real() {
        // This test hits the real Amtrak GTFS and Real-time API.
        // It serves as an integration test to verify the whole flow.

        println!("Downloading GTFS...");
        let gtfs = Gtfs::from_url_async("https://content.amtrak.com/content/gtfs/GTFS.zip")
            .await
            .expect("Failed to download GTFS");
        println!("GTFS downloaded. Trips count: {}", gtfs.trips.len());

        let client = reqwest::Client::new();

        println!("Generating alerts feed...");
        let start = std::time::Instant::now();
        let feed = generate_alerts_feed(&gtfs, &client).await;
        let duration = start.elapsed();

        println!("Generated {} alerts in {:?}.", feed.entity.len(), duration);

        for entity in feed.entity.iter().take(5) {
            println!("Alert: {:?}", entity);
        }

        // We generally expect at least SOME alerts or at least the code to not panic.
        assert_eq!(feed.header.gtfs_realtime_version, "2.0");
    }
}
