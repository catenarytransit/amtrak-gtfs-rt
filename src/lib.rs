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
//!    let gtfs = Gtfs::from_url_async("https://github.com/catenarytransit/amtrak-schedule-fixer/releases/download/latest/amtrak-gtfs-noshapes-fixed.zip")
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
mod timezone;
use geojson::FeatureCollection;
use gtfs_realtime::FeedEntity;
use gtfs_realtime::FeedMessage;
use gtfs_structures::Gtfs;
use jiff::{
    Timestamp, ToSpan,
    civil::{Date, DateTime, Weekday},
};
use std::collections::HashMap;
use std::time::SystemTime;
pub mod asm;

pub const DEFAULT_PROXIES: &[&str] = &[
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
    "http://198.23.236.47:1111",
    "http://198.23.189.151:8118",
    "http://162.255.110.107:8080",
];

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
    // "schcmnt":"Canceled"
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

/// Match a train to the GTFS schedule, including zero-padded train numbers
/// and date-specific service exceptions. Never invent a trip identifier.
fn match_scheduled_trip(
    gtfs: &Gtfs,
    route_id: Option<&str>,
    train_number: &str,
    service_date: Date,
) -> Option<String> {
    fn normalized(number: &str) -> &str {
        let trimmed = number.trim().trim_start_matches('0');
        if trimmed.is_empty() { "0" } else { trimmed }
    }

    fn runs_on(gtfs: &Gtfs, service_id: &str, day: Date) -> bool {
        if let Some(exceptions) = gtfs.calendar_dates.get(service_id) {
            if let Some(exception) = exceptions
                .iter()
                .rev()
                .find(|entry| entry.date.to_string() == day.to_string())
            {
                return exception.exception_type == gtfs_structures::Exception::Added;
            }
        }
        let Some(calendar) = gtfs.calendar.get(service_id) else {
            return false;
        };
        let Ok(first) = calendar.start_date.to_string().parse::<Date>() else {
            return false;
        };
        let Ok(last) = calendar.end_date.to_string().parse::<Date>() else {
            return false;
        };
        if day < first || day > last {
            return false;
        }
        match day.weekday() {
            Weekday::Monday => calendar.monday,
            Weekday::Tuesday => calendar.tuesday,
            Weekday::Wednesday => calendar.wednesday,
            Weekday::Thursday => calendar.thursday,
            Weekday::Friday => calendar.friday,
            Weekday::Saturday => calendar.saturday,
            Weekday::Sunday => calendar.sunday,
        }
    }

    let number = normalized(train_number);
    let mut candidates: Vec<_> = gtfs
        .trips
        .values()
        .filter(|trip| {
            route_id.is_none_or(|route| trip.route_id == route)
                && (trip
                    .trip_short_name
                    .as_deref()
                    .is_some_and(|short| normalized(short) == number)
                    || normalized(&trip.id) == number)
        })
        .collect();

    // Multiple scheduled variants require an exact Eastern service-date match.
    candidates.retain(|trip| runs_on(gtfs, &trip.service_id, service_date));
    if candidates.len() == 1 {
        Some(candidates[0].id.clone())
    } else {
        None
    }
}


/* A GTFS service day begins at midnight in the agency timezone, but its
 * stop_times can exceed 24 hours. The civil date of OrigSchDep in Eastern
 * time is therefore NOT necessarily the GTFS service date. */
fn match_scheduled_departure(
    gtfs: &Gtfs,
    route_id: Option<&str>,
    train_number: &str,
    origin_stop_id: &str,
    departure: Timestamp,
) -> Option<(String, Date)> {
    let eastern = timezone::get("America/New_York");
    let eastern_day = eastern.to_datetime(departure).date();
    fn normalized(s: &str) -> &str {
        let s = s.trim().trim_start_matches('0');
        if s.is_empty() { "0" } else { s }
    }
    // Evaluate both the current Eastern service day (00:00-23:59 GTFS)
    // and earlier service days (24:00+ GTFS). Bound the backward search by
    // the actual overflow in candidate first-stop departure times, allowing
    // one extra day for DST transitions. Do not test future service days:
    // GTFS stop times are nonnegative offsets from service-day midnight.
    let candidates: Vec<_> = gtfs.trips.values().filter_map(|trip| {
        if !route_id.is_none_or(|route| trip.route_id == route) {
            return None;
        }
        if !trip.trip_short_name.as_deref().is_some_and(|s| normalized(s) == normalized(train_number))
            && normalized(&trip.id) != normalized(train_number)
        {
            return None;
        }
        // Some GTFS trips contain a leading non-revenue stop or a stop with
        // no departure. Match Amtrak's explicit OrigCode, not the first row.
        let origin_stop = trip.stop_times.iter()
            .find(|stop_time| stop_time.stop.id == origin_stop_id
                && stop_time.departure_time.is_some())?;
        Some((trip, origin_stop.departure_time?))
    }).collect();
    let max_origin_seconds = candidates.iter().map(|(_, seconds)| *seconds).max()?;
    // Test current date and previous dates that can reach the observed
    // departure. A safety day accommodates the 23/25-hour DST boundary.
    let days_to_check = i64::from(max_origin_seconds / 86_400) + 1;
    let mut matches: Vec<(i64, String, Date)> = Vec::new();
    for offset in 0..=days_to_check {
        let service_date = eastern_day - offset.days();
        let Ok(midnight) = service_date.at(0, 0, 0, 0).to_zoned_datetime(eastern.clone()) else { continue };
        let midnight_seconds = midnight.timestamp().as_second();
        for (trip, departure_seconds) in &candidates {
            if !runs_on_service_day(gtfs, &trip.service_id, service_date) { continue }
            let Some(scheduled_seconds) = midnight_seconds
                .checked_add(i64::from(*departure_seconds)) else { continue };
            let discrepancy = (scheduled_seconds - departure.as_second()).abs();
            // Only minute-scale timetable rounding is tolerated.
            if discrepancy <= 300 {
                matches.push((discrepancy, trip.id.clone(), service_date));
            }
        }
    }
    matches.sort_by_key(|candidate| candidate.0);
    if matches.is_empty() {
        // Show each active candidate's absolute departure and discrepancy,
        // so upstream GTFS calendar errors can be distinguished from
        // stop_time timezone offsets without guessing.
        eprintln!(
            "Amtrak schedule match failed: train={}, route={:?}, origin={}, eastern_departure={}, candidates_with_origin={}, checked_days={}",
            train_number, route_id, origin_stop_id, departure,
            candidates.len(), days_to_check + 1,
        );
        for offset in 0..=days_to_check {
            let day = eastern_day - offset.days();
            let Ok(midnight) = day.at(0, 0, 0, 0).to_zoned_datetime(eastern.clone()) else {
                continue;
            };
            for (trip, seconds) in &candidates {
                if !runs_on_service_day(gtfs, &trip.service_id, day) {
                    continue;
                }
                if let Some(instant) = midnight.timestamp().as_second().checked_add(i64::from(*seconds)) {
                    eprintln!(
                        "  active candidate trip={} route={} service_day={} origin_seconds={} scheduled_unix={} difference_seconds={}",
                        trip.id, trip.route_id, day, seconds, instant,
                        instant - departure.as_second(),
                    );
                }
            }
        }
    }
    let best = matches.first()?;
    if matches.get(1).is_some_and(|other| other.0 == best.0) {
        // Ambiguous schedules must not generate an arbitrary trip_id.
        return None;
    }
    Some((best.1.clone(), best.2))
}

fn runs_on_service_day(gtfs: &Gtfs, service_id: &str, day: Date) -> bool {
    if let Some(exceptions) = gtfs.calendar_dates.get(service_id) {
        if let Some(exception) = exceptions
            .iter()
            .rev()
            .find(|entry| entry.date.to_string() == day.to_string())
        {
            return exception.exception_type == gtfs_structures::Exception::Added;
        }
    }
    let Some(calendar) = gtfs.calendar.get(service_id) else { return false };
    let Ok(first) = calendar.start_date.to_string().parse::<Date>() else { return false };
    let Ok(last) = calendar.end_date.to_string().parse::<Date>() else { return false };
    if day < first || day > last { return false }
    match day.weekday() {
        Weekday::Monday => calendar.monday,
        Weekday::Tuesday => calendar.tuesday,
        Weekday::Wednesday => calendar.wednesday,
        Weekday::Thursday => calendar.thursday,
        Weekday::Friday => calendar.friday,
        Weekday::Saturday => calendar.saturday,
        Weekday::Sunday => calendar.sunday,
    }
}

/// Some Amtrak realtime Northeast Regional services are published under a
/// separate GTFS route (Hartford Line or Valley Flyer). Resolve those only
/// when train number, origin, departure instant and active calendar agree.
fn match_northeast_regional_branch_departure(
    gtfs: &Gtfs,
    train_number: &str,
    origin_stop_id: &str,
    departure: Timestamp,
) -> Option<(String, Date, String)> {
    let mut matches = gtfs.routes.values()
        .filter(|route| matches_northeast_regional_branch_name(route.long_name.as_deref()))
        .filter_map(|route| {
            match_scheduled_departure(
                gtfs, Some(&route.id), train_number, origin_stop_id, departure,
            ).map(|(trip_id, day)| (trip_id, day, route.id.clone()))
        });
    let result = matches.next()?;
    // Do not select an arbitrary trip if both branches match.
    if matches.next().is_some() { return None; }
    Some(result)
}

fn matches_northeast_regional_branch_name(long_name: Option<&str>) -> bool {
    long_name.is_some_and(|name| {
        name.eq_ignore_ascii_case("Amtrak Hartford Line")
            || name.eq_ignore_ascii_case("Hartford Line")
            || name.eq_ignore_ascii_case("Amtrak Valley Flyer")
            || name.eq_ignore_ascii_case("Valley Flyer")
    })
}

fn feature_to_gtfs_unified(
    gtfs: &Gtfs,
    feature: &geojson::Feature,
    asm_lookup_table: Option<&HashMap<(Date, String), Vec<asm::AsmAlert>>>,
) -> FeedEntity {
    let geometry = feature.geometry.as_ref().unwrap();
    let point: Option<geojson::PointType> = match geometry.value.clone() {
        geojson::GeometryValue::Point {
            coordinates: coords,
        } => Some(coords),
        _ => None,
    };

    let long_name_to_route_id_hashmap: HashMap<String, String> = HashMap::from_iter(
        gtfs.routes
            .iter()
            .filter(|(_, route)| route.long_name.is_some())
            .map(|(_string, route)| (route.long_name.as_ref().unwrap().clone(), route.id.clone())),
    );

    let point = point.unwrap();

    let speed: Option<f32> = get_speed(feature);

    //unix time seconds
    let timestamp: Option<u64> = match feature.properties.as_ref().unwrap().get("updated_at") {
        Some(timestamp_text) => match timestamp_text {
            serde_json::value::Value::String(timestamp_text) => {
                process_timestamp_text(timestamp_text).map(|x| x as u64)
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
                    scheduled_time: None,
                }),
                None => match &feature.estarr {
                    Some(estarr) => Some(gtfs_realtime::trip_update::StopTimeEvent {
                        delay: None,
                        time: time_and_tz_to_unix(estarr, feature.tz),
                        uncertainty: None,
                        scheduled_time: None,
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
                                                                            scheduled_time: None,
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
                    scheduled_time: None,
                }),
                None => feature.estdep.as_ref().map(|estdep| gtfs_realtime::trip_update::StopTimeEvent {
                    delay: None,
                    time: time_and_tz_to_unix(estdep, feature.tz),
                    uncertainty: None,
                    scheduled_time: None,
                })},
            departure_occupancy_status: None,
            schedule_relationship: match feature.schcmnt.as_str() {
                "Canceled" => Some(1),
                _ => None
            },
            stop_time_properties: None,
        })
        .collect::<Vec<gtfs_realtime::trip_update::StopTimeUpdate>>();

    let origin_timestamp = origin_departure(&origin_time_string, origin_tz);

    let mut starting_service_date_new_york = timezone::get("America/New_York")
        .to_datetime(origin_timestamp)
        .date();

    // Check if the computed date is off by ~24 hours by comparing scheduled vs realtime
    // for the first stop with both values available
    let date_offset = detect_date_offset_from_delay(&features_list);
    if date_offset != 0 {
        starting_service_date_new_york =
            starting_service_date_new_york + (date_offset as i64).days();
    }

    // ASM alert dates and service weekdays use the train's origin-local date.
    let origin_local_time = tz_char_to_tz(origin_tz).to_datetime(origin_timestamp);

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

    let mut route_id: Option<String> = match route_name.as_deref() {
        Some("Gold Runner") => Some("GR".to_string()),
        Some(name) => long_name_to_route_id_hashmap.get(name).cloned(),
        None => None,
    };

    // Match the actual scheduled origin departure in absolute time, including
    // GTFS stop_times over 24 hours and calendar_dates exceptions.
    let origin_stop_id = feature.properties.as_ref()
        .and_then(|properties| properties.get("OrigCode"))
        .and_then(|value| value.as_str());
    let mut scheduled_match = trip_name.as_deref().zip(origin_stop_id).and_then(|(number, stop)| {
        match_scheduled_departure(gtfs, route_id.as_deref(), number, stop, origin_timestamp)
    });
    // Amtrak route labels do not always match the published GTFS route names
    // (e.g. "Keystone" vs. "Keystone Service", Hartford Line and Valley Flyer
    // vs. "Northeast Regional"). After the route-specific lookup fails, use
    // train short name, origin and active departure across all GTFS routes.
    // The matcher rejects equally plausible trips rather than guessing.
    if scheduled_match.is_none() && route_name.as_deref() != Some("Gold Runner") {
        if let Some((number, stop)) = trip_name.as_deref().zip(origin_stop_id) {
            if let Some((id, day)) =
                match_scheduled_departure(gtfs, None, number, stop, origin_timestamp)
            {
                if let Some(trip) = gtfs.trips.get(&id) {
                    route_id = Some(trip.route_id.clone());
                    scheduled_match = Some((id, day));
                }
            }
        }
    }
    // Limit the original named fallback to Northeast Regional when a unique
    // across-all-routes match could not be established.
    if scheduled_match.is_none() && route_name.as_deref() == Some("Northeast Regional") {
        if let Some((number, stop)) = trip_name.as_deref().zip(origin_stop_id) {
            if let Some((trip_id, day, branch_route_id)) =
                match_northeast_regional_branch_departure(gtfs, number, stop, origin_timestamp)
            {
                route_id = Some(branch_route_id);
                scheduled_match = Some((trip_id, day));
            }
        }
    }
    if let Some((_, day)) = &scheduled_match {
        starting_service_date_new_york = *day;
    }
    let starting_yyyy_mm_dd_in_new_york = starting_service_date_new_york
        .strftime("%Y%m%d").to_string();
    let trip_id = scheduled_match.map(|(id, _)| id);
    if trip_id.is_none() && route_id.as_deref() != Some("GR") {
        if let Some(route_id) = &route_id {

            let route_name = gtfs.routes.get(route_id.as_str()).and_then(|route| route.long_name.as_ref());

            eprintln!(
                "Unmatched scheduled train {:?}, route {:?}, Eastern service date {}",
                trip_name,
                route_name,
                starting_service_date_new_york
            );

            println!("Possible trips for this train:");
            for trip in gtfs.trips.values().filter(|trip| {
                trip.route_id == *route_id
                    && trip.trip_short_name.as_deref().is_some_and(|s| s == trip_name.as_ref().unwrap())
            }) {
                println!(
                    "  trip_id: {}, service_id: {}, trip_short_name: {:?}",
                    trip.id, trip.service_id, trip.trip_short_name
                );
                //print service date
                if let Some(calendar) = gtfs.calendar.get(&trip.service_id) {
                    println!(
                        "    service dates: {} to {}, weekdays: {}{}{}{}{}{}{}",
                        calendar.start_date, calendar.end_date,
                        if calendar.monday { "M" } else { "-" },
                        if calendar.tuesday { "T" } else { "-" },
                        if calendar.wednesday { "W" } else { "-" },
                        if calendar.thursday { "R" } else { "-" },
                        if calendar.friday { "F" } else { "-" },
                        if calendar.saturday { "S" } else { "-" },
                        if calendar.sunday { "U" } else { "-" },
                    );
                }
            }
        }
    }

    let id = match &train_num {
        Some(train_num) => Some(format!("{}-{}", starting_yyyy_mm_dd_in_new_york, train_num)),
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
                match asm_lookup_table.get(&(origin_local_time.date(), train_num.clone())) {
                    Some(alerts) => asm_alert_to_gtfs_rt(informed_entity, alerts),
                    None => None,
                }
            }
            None => None,
        },
        None => None,
    };

    FeedEntity {
        alert,
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
        feed_version: None,
    }
}

fn tz_char_to_tz(tz: char) -> jiff::tz::TimeZone {
    timezone::get(match tz {
        'M' => "America/Denver",
        'P' => "America/Los_Angeles",
        'C' => "America/Chicago",
        'A' => "America/Phoenix",
        _ => "America/New_York",
    })
}

// Select the later instant during a fold, and reject nonexistent local times.
fn time_and_tz_to_unix(timestamp_text: &str, tz: char) -> Option<i64> {
    let dt = DateTime::strptime("%m/%d/%Y %H:%M:%S", timestamp_text).ok()?;
    timezone::latest_timestamp(&tz_char_to_tz(tz), dt).map(|ts| ts.as_second())
}

/// Detects if the starting date is off by approximately 24 hours by analyzing delays.
/// Looks at the first stop with both scheduled and realtime times available.
/// Returns: -1 if date should be shifted back 1 day, +1 if forward 1 day, 0 if no adjustment needed.
fn detect_date_offset_from_delay(features_list: &[AmtrakArrivalJson]) -> i32 {
    const TWENTY_TWO_HOURS_SECS: i64 = 22 * 3600;
    const TWENTY_SIX_HOURS_SECS: i64 = 26 * 3600;

    for feature in features_list {
        // Get scheduled time (prefer arrival, fall back to departure)
        let scheduled_time = feature
            .scharr
            .as_ref()
            .and_then(|t| time_and_tz_to_unix(t, feature.tz))
            .or_else(|| {
                feature
                    .schdep
                    .as_ref()
                    .and_then(|t| time_and_tz_to_unix(t, feature.tz))
            });

        // Get realtime/actual time (prefer actual, fall back to estimated)
        let realtime_time = feature
            .postarr
            .as_ref()
            .and_then(|t| time_and_tz_to_unix(t, feature.tz))
            .or_else(|| {
                feature
                    .postdep
                    .as_ref()
                    .and_then(|t| time_and_tz_to_unix(t, feature.tz))
            })
            .or_else(|| {
                feature
                    .estarr
                    .as_ref()
                    .and_then(|t| time_and_tz_to_unix(t, feature.tz))
            })
            .or_else(|| {
                feature
                    .estdep
                    .as_ref()
                    .and_then(|t| time_and_tz_to_unix(t, feature.tz))
            });

        if let (Some(sched), Some(rt)) = (scheduled_time, realtime_time) {
            let delay = rt - sched;

            // If delay is close to +24 hours, the start date should be shifted back 1 day
            if delay > TWENTY_TWO_HOURS_SECS && delay < TWENTY_SIX_HOURS_SECS {
                return -1;
            }
            // If delay is close to -24 hours, the start date should be shifted forward 1 day
            if delay < -TWENTY_TWO_HOURS_SECS && delay > -TWENTY_SIX_HOURS_SECS {
                return 1;
            }
            // Found a usable pair, delay is reasonable
            return 0;
        }
    }

    // No usable data found
    0
}

//for origin departure conversion to local time representation
pub fn origin_departure(timestamp_text: &str, tz: char) -> Timestamp {
    let dt = DateTime::strptime("%m/%d/%Y %l:%M:%S %p", timestamp_text).unwrap();
    let tz = tz_char_to_tz(tz);
    timezone::latest_timestamp(&tz, dt).expect("valid origin departure")
}

// Time is formatted 11/18/2023 4:58:09 PM.
pub fn process_timestamp_text(timestamp_text: &str) -> Option<i64> {
    let dt = DateTime::strptime("%m/%d/%Y %l:%M:%S %p", timestamp_text).ok()?;
    timezone::latest_timestamp(&timezone::get("America/New_York"), dt).map(|ts| ts.as_second())
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

            match fetch_pacific_surfliner_advisories(client, gtfs).await {
                Ok(mut surfliner_alerts) => {
                    alerts.append(&mut surfliner_alerts);
                }
                Err(e) => {
                    eprintln!("Error fetching Pacific Surfliner alerts: {}", e);
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

            let raw_data_text = raw_data.text().await?;

            let decrypted_string = amtk::decrypt(raw_data_text.as_str())?;

            let geojson: geojson::GeoJson = decrypted_string.parse::<geojson::GeoJson>()?;
            let features_collection: FeatureCollection = FeatureCollection::try_from(geojson)?;

            let lookup_table: Option<HashMap<(Date, String), Vec<asm::AsmAlert>>> =
                match raw_asm_data {
                    Ok(raw_asm_data) => {
                        let asm_root = raw_asm_data.text().await?;

                        println!("ASM data successfully downloaded");

                        let asm_root_json = serde_json::from_str::<asm::AsmRoot>(&asm_root);

                        match asm_root_json {
                            Ok(asm_root) => Some(asm::make_lookup_table_from_asm_root(asm_root)),
                            Err(e) => {
                                eprintln!(
                                    "Error parsing ASM data, proceeding without alerts, {:?}",
                                    e
                                );
                                None
                            }
                        }
                    }
                    Err(_) => {
                        eprintln!("Error fetching ASM data, proceeding without alerts");
                        None
                    }
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

pub mod pacific_surfliner_website;
use pacific_surfliner_website::fetch_pacific_surfliner_advisories;

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

        let gtfs = Gtfs::from_url_async("https://github.com/catenarytransit/amtrak-schedule-fixer/releases/download/latest/amtrak-gtfs-noshapes-fixed.zip")
            .await
            .unwrap();

        println!("amtrak download finished");

        let amtrak_results = fetch_amtrak_gtfs_rt_joined(&gtfs, &client).await;

        assert!(amtrak_results.is_ok());

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
        // Only Gold Runner uses train-number identity without a formal GTFS
        // schedule match; Hartford Line must match its scheduled trip.
        let without_trip: Vec<_> = amtrak_results
            .as_ref()
            .unwrap()
            .unified_feed
            .entity
            .iter()
            .filter(|entity| {
                entity.trip_update.as_ref().is_some_and(|update| {
                    update.trip.route_id.as_deref().is_some_and(|route| route != "GR")
                        && update.trip.trip_id.is_none()
                })
            })
            .map(|entity| entity.id.as_str())
            .collect();
        assert!(
            without_trip.is_empty(),
            "Trains with routes but no trip IDs: {without_trip:?}"
        );

        // println!("{:?}", amtrak_results.unwrap());
    }

    #[tokio::test]
    async fn test_alerts_feed_real() {
        let client = reqwest::ClientBuilder::new()
            .deflate(true)
            .gzip(true)
            .brotli(true)
            .build()
            .unwrap();

        let gtfs = Gtfs::from_url_async("https://github.com/catenarytransit/amtrak-schedule-fixer/releases/download/latest/amtrak-gtfs-noshapes-fixed.zip")
            .await
            .unwrap();

        let amtrak_results = fetch_amtrak_gtfs_rt(&gtfs, &client).await.unwrap();

        println!(
            "Trip updates: {} vehicles: {} alerts: {}",
            amtrak_results.trip_updates.entity.len(),
            amtrak_results.vehicle_positions.entity.len(),
            amtrak_results.alerts.entity.len()
        );

        // Ensure alerts feed is at least structurally valid
        for entity in &amtrak_results.alerts.entity {
            if let Some(alert) = &entity.alert {
                if let Some(desc) = &alert.description_text {
                    if !desc.translation.is_empty() {
                        println!("Alert description: {:?}", desc.translation[0].text);
                    }
                }
            }
        }
    }

    #[test]
    fn scheduled_trip_matching_assigns_every_known_route_trip() {
        // Reproduces the bug where a route was populated but the trip was missing.
        // Amtrak can report "595" while the GTFS short name is zero-padded.
        let day = jiff::civil::date(2026, 11, 16);
        let mut gtfs = Gtfs::default();
        gtfs.trips.insert(
            "scheduled-595".into(),
            gtfs_structures::Trip {
                id: "scheduled-595".into(),
                route_id: "PAC_SURF".into(),
                service_id: "special".into(),
                trip_short_name: Some("0595".into()),
                ..Default::default()
            },
        );
        gtfs.trips.insert(
            "other-route".into(),
            gtfs_structures::Trip {
                id: "other-route".into(),
                route_id: "OTHER".into(),
                service_id: "special".into(),
                trip_short_name: Some("595".into()),
                ..Default::default()
            },
        );
        gtfs.calendar_dates.insert(
            "special".into(),
            vec![gtfs_structures::CalendarDate {
                service_id: "special".into(),
                date: "2026-11-16".parse().unwrap(),
                exception_type: gtfs_structures::Exception::Added,
            }],
        );

        assert_eq!(
            match_scheduled_trip(&gtfs, Some("PAC_SURF"), "595", day),
            Some("scheduled-595".to_string())
        );
        assert_eq!(
            match_scheduled_trip(&gtfs, Some("OTHER"), "0595", day),
            Some("other-route".to_string())
        );
        // A missing schedule is not grounds for inventing a trip_id.
        assert_eq!(
            match_scheduled_trip(&gtfs, Some("PAC_SURF"), "999", day),
            None
        );
    }


    #[test]
    fn checks_both_current_and_previous_gtfs_service_days() {
        use std::sync::Arc;
        let mut gtfs = Gtfs::default();
        let lax = Arc::new(gtfs_structures::Stop {
            id: "LAX".into(),
            ..Default::default()
        });
        for (id, service, seconds) in [
            ("current", "monday", 3600u32),
            ("overflow", "sunday", 25 * 3600),
            ("wrong-time", "sunday", 26 * 3600),
        ] {
            gtfs.trips.insert(id.into(), gtfs_structures::Trip {
                id: id.into(),
                route_id: "SUNSET".into(),
                service_id: service.into(),
                trip_short_name: Some("2".into()),
                stop_times: vec![gtfs_structures::StopTime {
                    stop: lax.clone(),
                    departure_time: Some(seconds),
                    ..Default::default()
                }],
                ..Default::default()
            });
        }
        for (service, day) in [
            ("monday", "2026-11-16"),
            ("sunday", "2026-11-15"),
        ] {
            gtfs.calendar_dates.insert(service.into(), vec![gtfs_structures::CalendarDate {
                service_id: service.into(),
                date: day.parse().unwrap(),
                exception_type: gtfs_structures::Exception::Added,
            }]);
        }
        let origin = origin_departure("11/15/2026 10:00:00 PM", 'P');
        // Both the same-day 01:00 and previous-day 25:00 represent
        // this exact instant. It must NOT arbitrarily select a trip.
        assert_eq!(
            match_scheduled_departure(&gtfs, Some("SUNSET"), "2", "LAX", origin),
            None
        );
        gtfs.trips.remove("current");
        assert_eq!(
            match_scheduled_departure(&gtfs, Some("SUNSET"), "2", "LAX", origin),
            Some(("overflow".into(), jiff::civil::date(2026, 11, 15)))
        );
        gtfs.trips.remove("overflow");
        gtfs.trips.remove("wrong-time");
        // Now only the current-day trip exists: verify same-day matching.
        gtfs.trips.insert("current".into(), gtfs_structures::Trip {
            id: "current".into(),
            route_id: "SUNSET".into(),
            service_id: "monday".into(),
            trip_short_name: Some("2".into()),
            stop_times: vec![gtfs_structures::StopTime {
                stop: lax,
                departure_time: Some(3600),
                ..Default::default()
            }],
            ..Default::default()
        });
        assert_eq!(
            match_scheduled_departure(&gtfs, Some("SUNSET"), "2", "LAX", origin),
            Some(("current".into(), jiff::civil::date(2026, 11, 16)))
        );
    }

    #[test]
    fn northeast_regional_hartford_line_matches_by_train_short_name() {
        use std::sync::Arc;
        let mut gtfs = Gtfs::default();
        gtfs.routes.insert("HARTFORD".into(), gtfs_structures::Route {
            id: "HARTFORD".into(),
            long_name: Some("Amtrak Hartford Line".into()),
            ..Default::default()
        });
        gtfs.trips.insert("hartford-scheduled".into(), gtfs_structures::Trip {
            id: "hartford-scheduled".into(),
            route_id: "HARTFORD".into(),
            service_id: "hartford-day".into(),
            trip_short_name: Some("0475".into()),
            stop_times: vec![gtfs_structures::StopTime {
                stop: Arc::new(gtfs_structures::Stop {
                    id: "NHV".into(),
                    ..Default::default()
                }),
                departure_time: Some(12 * 3600),
                ..Default::default()
            }],
            ..Default::default()
        });
        gtfs.calendar_dates.insert("hartford-day".into(), vec![
            gtfs_structures::CalendarDate {
                service_id: "hartford-day".into(),
                date: "2026-11-16".parse().unwrap(),
                exception_type: gtfs_structures::Exception::Added,
            },
        ]);
        let departure = origin_departure("11/16/2026 12:00:00 PM", 'E');
        assert_eq!(
            match_northeast_regional_branch_departure(&gtfs, "475", "NHV", departure),
            Some((
                "hartford-scheduled".into(),
                jiff::civil::date(2026, 11, 16),
                "HARTFORD".into(),
            ))
        );
        assert!(match_northeast_regional_branch_departure(&gtfs, "476", "NHV", departure).is_none());
    }

    #[test]
    fn keystone_route_label_falls_back_to_gtfs_trip_short_name() {
        use std::sync::Arc;
        let mut gtfs = Gtfs::default();
        gtfs.routes.insert("KEYSTONE".into(), gtfs_structures::Route {
            id: "KEYSTONE".into(),
            long_name: Some("Keystone Service".into()),
            ..Default::default()
        });
        gtfs.trips.insert("keystone-trip".into(), gtfs_structures::Trip {
            id: "keystone-trip".into(),
            route_id: "KEYSTONE".into(),
            service_id: "keystone-monday".into(),
            trip_short_name: Some("0647".into()),
            stop_times: vec![gtfs_structures::StopTime {
                stop: Arc::new(gtfs_structures::Stop {
                    id: "NYP".into(),
                    ..Default::default()
                }),
                departure_time: Some(14 * 3600 + 11 * 60),
                ..Default::default()
            }],
            ..Default::default()
        });
        gtfs.calendar_dates.insert("keystone-monday".into(), vec![
            gtfs_structures::CalendarDate {
                service_id: "keystone-monday".into(),
                date: "2026-11-16".parse().unwrap(),
                exception_type: gtfs_structures::Exception::Added,
            },
        ]);
        let departure = origin_departure("11/16/2026 2:11:00 PM", 'E');
        let matched = match_scheduled_departure(&gtfs, None, "647", "NYP", departure);
        assert_eq!(
            matched,
            Some(("keystone-trip".into(), jiff::civil::date(2026, 11, 16)))
        );
        assert_eq!(gtfs.trips.get(&matched.unwrap().0).unwrap().route_id, "KEYSTONE");
        assert!(match_scheduled_departure(&gtfs, None, "647", "PHL", departure).is_none());
    }

    #[test]
    fn northeast_regional_valley_flyer_matches_by_train_short_name() {
        use std::sync::Arc;
        let mut gtfs = Gtfs::default();
        gtfs.routes.insert("VALLEY".into(), gtfs_structures::Route {
            id: "VALLEY".into(),
            long_name: Some("Amtrak Valley Flyer".into()),
            ..Default::default()
        });
        gtfs.trips.insert("valley-scheduled".into(), gtfs_structures::Trip {
            id: "valley-scheduled".into(),
            route_id: "VALLEY".into(),
            service_id: "valley-day".into(),
            trip_short_name: Some("0486".into()),
            stop_times: vec![gtfs_structures::StopTime {
                stop: Arc::new(gtfs_structures::Stop {
                    id: "NHV".into(),
                    ..Default::default()
                }),
                departure_time: Some(15 * 3600),
                ..Default::default()
            }],
            ..Default::default()
        });
        gtfs.calendar_dates.insert("valley-day".into(), vec![
            gtfs_structures::CalendarDate {
                service_id: "valley-day".into(),
                date: "2026-11-16".parse().unwrap(),
                exception_type: gtfs_structures::Exception::Added,
            },
        ]);
        let departure = origin_departure("11/16/2026 3:00:00 PM", 'E');
        assert_eq!(
            match_northeast_regional_branch_departure(&gtfs, "486", "NHV", departure),
            Some((
                "valley-scheduled".into(),
                jiff::civil::date(2026, 11, 16),
                "VALLEY".into(),
            ))
        );
        assert!(match_northeast_regional_branch_departure(&gtfs, "487", "NHV", departure).is_none());
        assert!(match_northeast_regional_branch_departure(&gtfs, "486", "NYP", departure).is_none());
    }

    #[test]
    fn match_origin_even_when_not_first_stop_time() {
        use std::sync::Arc;
        let mut gtfs = Gtfs::default();
        let stop = |code: &str, seconds| gtfs_structures::StopTime {
            stop: Arc::new(gtfs_structures::Stop {
                id: code.into(),
                ..Default::default()
            }),
            departure_time: seconds,
            ..Default::default()
        };
        gtfs.trips.insert("sunset".into(), gtfs_structures::Trip {
            id: "sunset".into(),
            route_id: "SUNSET".into(),
            service_id: "sunday".into(),
            trip_short_name: Some("2".into()),
            stop_times: vec![stop("NONREVENUE", None), stop("LAX", Some(25 * 3600))],
            ..Default::default()
        });
        gtfs.calendar_dates.insert("sunday".into(), vec![gtfs_structures::CalendarDate {
            service_id: "sunday".into(),
            date: "2026-11-15".parse().unwrap(),
            exception_type: gtfs_structures::Exception::Added,
        }]);
        let origin = origin_departure("11/15/2026 10:00:00 PM", 'P');
        assert_eq!(
            match_scheduled_departure(&gtfs, Some("SUNSET"), "2", "LAX", origin),
            Some(("sunset".into(), jiff::civil::date(2026, 11, 15)))
        );
    }

    #[test]
    fn sunset_limited_sunday_pacific_departure_uses_gtfs_service_day() {
        use std::sync::Arc;
        let mut gtfs = Gtfs::default();
        gtfs.trips.insert("sunset-2-sunday".into(), gtfs_structures::Trip {
            id: "sunset-2-sunday".into(),
            route_id: "SUNSET".into(),
            service_id: "sunday".into(),
            trip_short_name: Some("2".into()),
            stop_times: vec![gtfs_structures::StopTime {
                stop: Arc::new(gtfs_structures::Stop {
                    id: "LAX".into(),
                    ..Default::default()
                }),
                departure_time: Some(25 * 3600),
                ..Default::default()
            }],
            ..Default::default()
        });
        gtfs.calendar_dates.insert("sunday".into(), vec![gtfs_structures::CalendarDate {
            service_id: "sunday".into(),
            date: "2026-11-15".parse().unwrap(),
            exception_type: gtfs_structures::Exception::Added,
        }]);
        let origin = origin_departure("11/15/2026 10:00:00 PM", 'P');
        assert_eq!(
            match_scheduled_departure(&gtfs, Some("SUNSET"), "2", "LAX", origin),
            Some(("sunset-2-sunday".into(), jiff::civil::date(2026, 11, 15)))
        );
        assert!(match_scheduled_departure(&gtfs, Some("SUNSET"), "2", "NOL", origin).is_none());
    }

    #[test]
    fn late_los_angeles_departure_matches_next_eastern_service_day() {
        // GTFS schedule dates are Eastern-based, not the origin's local day.
        // 22:00 Sunday in Los Angeles is 01:00 Monday in New York.
        let origin = origin_departure("11/15/2026 10:00:00 PM", 'P');
        let pacific_date = tz_char_to_tz('P').to_datetime(origin).date();
        let eastern_date = timezone::get("America/New_York").to_datetime(origin).date();
        assert_eq!(pacific_date, jiff::civil::date(2026, 11, 15));
        assert_eq!(eastern_date, jiff::civil::date(2026, 11, 16));

        let mut gtfs = Gtfs::default();
        gtfs.trips.insert(
            "train-595-sun".into(),
            gtfs_structures::Trip {
                id: "train-595-sun".into(),
                route_id: "PAC_SURF".into(),
                service_id: "sunday".into(),
                trip_short_name: Some("0595".into()),
                ..Default::default()
            },
        );
        gtfs.trips.insert(
            "train-595-mon".into(),
            gtfs_structures::Trip {
                id: "train-595-mon".into(),
                route_id: "PAC_SURF".into(),
                service_id: "monday".into(),
                trip_short_name: Some("595".into()),
                ..Default::default()
            },
        );
        for (service_id, date) in [("sunday", "2026-11-15"), ("monday", "2026-11-16")] {
            gtfs.calendar_dates.insert(
                service_id.into(),
                vec![gtfs_structures::CalendarDate {
                    service_id: service_id.into(),
                    date: date.parse().unwrap(),
                    exception_type: gtfs_structures::Exception::Added,
                }],
            );
        }

        assert_eq!(
            match_scheduled_trip(&gtfs, Some("PAC_SURF"), "595", eastern_date),
            Some("train-595-mon".to_string()),
            "Must match the next day's Eastern-based GTFS service"
        );
        assert_eq!(
            match_scheduled_trip(&gtfs, Some("PAC_SURF"), "595", pacific_date),
            Some("train-595-sun".to_string()),
            "This is the wrong trip when local Pacific dates are used"
        );
    }

    #[test]
    fn removed_service_does_not_get_a_trip_id() {
        let day = jiff::civil::date(2026, 11, 16);
        let mut gtfs = Gtfs::default();
        gtfs.trips.insert(
            "removed".into(),
            gtfs_structures::Trip {
                id: "removed".into(),
                route_id: "R".into(),
                service_id: "canceled".into(),
                trip_short_name: Some("123".into()),
                ..Default::default()
            },
        );
        gtfs.calendar_dates.insert(
            "canceled".into(),
            vec![gtfs_structures::CalendarDate {
                service_id: "canceled".into(),
                date: "2026-11-16".parse().unwrap(),
                exception_type: gtfs_structures::Exception::Deleted,
            }],
        );
        assert!(match_scheduled_trip(&gtfs, Some("R"), "123", day).is_none());
    }

    #[test]
    fn read_last_stop() {
        let test_str = r#"{"code":"LAX","tz":"P","bus":false,"scharr":"10/11/2024 16:57:00","schcmnt":"","autoarr":false,"autodep":false}"#;

        let amtrak_arrival = serde_json::from_str::<AmtrakArrivalJson>(test_str);

        assert!(amtrak_arrival.is_ok());

        let amtrak_arrival = amtrak_arrival.unwrap();

        assert_eq!(amtrak_arrival.code, "LAX");
    }

    #[test]
    fn test_origin() {
        let origin_departure_calc = origin_departure("8/8/2025 9:51:00 AM", 'P');

        println!("{:#?}", origin_departure_calc);

        let starting_yyyy_mm_dd_in_new_york = timezone::get("America/New_York")
            .to_datetime(origin_departure_calc)
            .date()
            .strftime("%Y%m%d")
            .to_string();

        println!("{:#?}", starting_yyyy_mm_dd_in_new_york);
    }
    #[tokio::test]
    async fn test_surfliner_advisories() {
        let client = reqwest::Client::new();
        // create a dummy GTFS with Pacific Surfliner route

        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "PAC_SURF_ID".to_string(),
            gtfs_structures::Route {
                id: "PAC_SURF_ID".to_string(),
                long_name: Some("Pacific Surfliner".to_string()),
                short_name: None,
                desc: None,
                route_type: gtfs_structures::RouteType::Rail,
                url: None,
                ..Default::default()
            },
        );

        // Construct Gtfs using Default if available, otherwise minimal manual
        let gtfs = Gtfs {
            routes,
            ..Default::default()
        };

        // This test hits the network, so it might change over time.
        // We just check that it runs and doesn't crash.
        let alerts = fetch_pacific_surfliner_advisories(&client, &gtfs).await;

        match alerts {
            Ok(alerts) => {
                println!("Fetched {} alerts", alerts.len());
                for alert in alerts {
                    // Check if translation exists before accessing
                    if let Some(header) = alert.alert.as_ref().and_then(|a| a.header_text.as_ref())
                    {
                        if !header.translation.is_empty() {
                            println!("Alert: {:?}", header.translation[0].text);
                        }
                    }
                    if let Some(desc) = alert
                        .alert
                        .as_ref()
                        .and_then(|a| a.description_text.as_ref())
                    {
                        if !desc.translation.is_empty() {
                            println!("Description: {:?}", desc.translation[0].text);
                        }
                    }
                }
            }
            Err(e) => {
                println!("Error fetching alerts: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod jiff_regressions {
    use super::*;
    #[test]
    fn feed_times_keep_later_fold_and_reject_gap() {
        let expected = "2026-11-01T06:30:00Z"
            .parse::<jiff::Timestamp>()
            .unwrap()
            .as_second();
        assert_eq!(
            process_timestamp_text("11/01/2026 1:30:00 AM"),
            Some(expected)
        );
        assert_eq!(
            time_and_tz_to_unix("11/01/2026 01:30:00", 'E'),
            Some(expected)
        );
        assert!(process_timestamp_text("03/08/2026 2:30:00 AM").is_none());
        assert!(time_and_tz_to_unix("03/08/2026 02:30:00", 'E').is_none());
        assert!(process_timestamp_text("invalid").is_none());
    }
    #[test]
    fn origin_dates_and_arizona_keep_their_timezone_rules() {
        let origin = origin_departure("8/8/2025 9:51:00 AM", 'P');
        assert_eq!(
            timezone::get("America/New_York")
                .to_datetime(origin)
                .strftime("%Y%m%d")
                .to_string(),
            "20250808"
        );
        let mountain = time_and_tz_to_unix("07/01/2026 12:00:00", 'M').unwrap();
        let arizona = time_and_tz_to_unix("07/01/2026 12:00:00", 'A').unwrap();
        assert_eq!(arizona - mountain, 3600);
    }
}
