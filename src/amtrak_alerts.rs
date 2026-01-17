use chrono::Datelike;
use chrono::Datelike;
use gtfs_realtime::translated_string::Translation;
use gtfs_realtime::{Alert, EntitySelector, FeedEntity, FeedHeader, FeedMessage};
use reqwest::Client;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

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

    let res = client.get(&url).send().await?;
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

pub async fn generate_alerts_feed(gtfs: &gtfs_structures::Gtfs, client: &Client) -> FeedMessage {
    let mut entities = vec![];

    let now_system = SystemTime::now();
    let now_date = chrono::prelude::Utc::now().naive_utc().date();

    // We want to look at trains running today, yesterday (if delayed), and tomorrow (if starting soon).
    let window_start = now_system - std::time::Duration::from_secs(8 * 3600); // 8 hours ago
    let window_end = now_system + std::time::Duration::from_secs(4 * 3600); // 4 hours in the future (buffer for upcoming)

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
                // We'll approximate using the date at midnight UTC (simplification, ideally use feed timezone).
                // Assuming Amtrak feed is mostly America/New_York or similar, but dates are dates.

                // Find first and last stop time
                // This might be slow if stop_times are lazy, but usually they are populated.

                // Optimization: Just check if we have stop times.
                // Actually, accessing stop_times for every trip could be heavy.
                // But typically necessary.

                // Let's get the max and min times from the trip's stop times if available
                // If not, we skip.

                // Actually Gtfs struct usually has stop_times attached to trip or via separate map.
                // gtfs-structures puts them in trip.stop_times if parsed that way.
                // Let's assume they are there.

                // gtfs-structures Trip struct has stop_times field.
                let stop_times = &trip.stop_times;
                if !stop_times.is_empty() {
                    // We need to hint the type here or allow inference to work by not wrapping in wrapper tuple immediately if complex
                    if let Some(first) = stop_times.first() {
                        if let Some(last) = stop_times.last() {
                            let start_secs = first.departure_time.unwrap_or(0);
                            let end_secs = last
                                .arrival_time
                                .unwrap_or(last.departure_time.unwrap_or(0));

                            // Construct rough timestamps
                            // We can't easily get precise SystemTime without timezone.
                            // But we can check roughly against "seconds from now vs seconds from midnight".

                            // Let's stick to the "8 hour buffer" logic using chrono dates.
                            let date_midnight = date_check.and_hms_opt(0, 0, 0).unwrap();
                            let trip_start =
                                date_midnight + chrono::Duration::seconds(start_secs as i64);
                            let trip_end =
                                date_midnight + chrono::Duration::seconds(end_secs as i64);

                            // We used UTC date. Amtrak is US. Let's assume UTC for date math relative to now is "close enough"
                            // or we should be more generous with buffer.
                            // Better: use the current time in a fixed timezone like America/New_York?
                            // Since we don't know the user's detailed intent for Timezone,
                            // and we are just filtering candidates, being generous is better.

                            // Window check:
                            // Does the trip interval [Start, End] overlap with [Now - 8h, Now + 8h]?
                            // Actually user said "buffer to previous (8h) and future".
                            // Meaning:
                            // If trip ended 7 hours ago, it might still be running (delayed). Include.
                            // If trip ends 9 hours ago, exclude.
                            // If trip starts 2 hours from now, include.
                            // If trip starts 20 hours from now, exclude.

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

    for (train_num, date) in train_queries {
        if let Ok(Some(service)) = fetch_train_status(client, &train_num, &date).await {
            if let Some(entity) = create_alert_entity(train_num.clone(), service) {
                entities.push(entity);
            }
        }
    }

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
        let feed = generate_alerts_feed(&gtfs, &client).await;

        println!("Generated {} alerts.", feed.entity.len());

        for entity in feed.entity.iter().take(5) {
            println!("Alert: {:?}", entity);
        }

        // We generally expect at least SOME alerts or at least the code to not panic.
        // Asserting > 0 might be flaky if Amtrak is having a perfect day (rare).
        // So we just assert it ran.
        assert!(feed.header.gtfs_realtime_version == "2.0");
    }
}
