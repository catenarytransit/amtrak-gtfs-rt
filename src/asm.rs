use std::collections::HashMap;

use gtfs_realtime::Alert as GtfsRtAlert;
use serde::Deserialize;
use serde::Serialize;

// New ASM schema
pub type Welcome = Vec<WelcomeElement>;

#[derive(Serialize, Deserialize)]
pub struct WelcomeElement {
    train_id: String,
    railroad: Railroad,
    origin_date: String,
    number: i64,
    all_numbers: Vec<i64>,
    name: String,
    origin: String,
    destination: String,
    partial_train: bool,
    last_updated: i64,
    current_timezone: String,
    threshold: i64,
    disruption: bool,
    total_miles: i64,
    location: Option<Location>,
    stops: Vec<Stop>,
    alerts: Option<Vec<Alert>>,
}

#[derive(Serialize, Deserialize)]
pub struct Alert {
    record_time: i64,
    text: String,
}

#[derive(Serialize, Deserialize)]
pub struct Location {
    latitude: f64,
    longitude: f64,
    heading: Option<i64>,
    speed: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Railroad {
    Amtrak,
    #[serde(rename = "VIA_RAIL")]
    ViaRail,
}

#[derive(Serialize, Deserialize)]
pub struct Stop {
    code: String,
    miles: i64,
    sched_depart: Option<i64>,
    depart: Option<Arrive>,
    canceled: bool,
    sched_arrive: Option<i64>,
    arrive: Option<Arrive>,
}

#[derive(Serialize, Deserialize)]
pub struct Arrive {
    variance: i64,
    times_compared: TimesCompared,
    #[serde(rename = "type")]
    arrive_type: Type,
}

#[derive(Serialize, Deserialize)]
pub enum Type {
    #[serde(rename = "ACTUAL")]
    Actual,
    #[serde(rename = "ESTIMATED")]
    Estimated,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TimesCompared {
    Departure,
    #[serde(rename = "ENROUTE_ARRIVAL")]
    EnrouteArrival,
    #[serde(rename = "MISMATCH_ARRIVAL")]
    MismatchArrival,
    #[serde(rename = "TERMINAL_ARRIVAL")]
    TerminalArrival,
}

// Backwards-compatible aliases for existing code
pub type AsmRoot = Welcome;
pub type AsmAlert = Alert;

pub fn make_lookup_table_from_asm_root(
    asm_root: AsmRoot,
) -> HashMap<(chrono::NaiveDate, String), Vec<AsmAlert>> {
    let mut lookup_table = HashMap::new();
    for train in asm_root {
        let date = chrono::NaiveDate::parse_from_str(&train.origin_date, "%Y-%m-%d");

        if let Ok(date) = date {
            let train_num = train.number.to_string();

            if let Some(alerts) = train.alerts {
                for alert in alerts {
                    lookup_table
                        .entry((date, train_num.clone()))
                        .or_insert(Vec::new())
                        .push(alert);
                }
            }
        }
    }

    //  println!("LOOKUP TABLE: {:#?}", lookup_table);

    lookup_table
}

pub fn asm_alert_to_gtfs_rt(
    informed_entity: gtfs_realtime::EntitySelector,
    asm_alerts: &Vec<AsmAlert>,
) -> Option<GtfsRtAlert> {
    if asm_alerts.is_empty() {
        return None;
    }

    let mut alert = gtfs_realtime::Alert::default();
    let mut description = gtfs_realtime::TranslatedString::default();

    let text = asm_alerts
        .iter()
        .map(|alert| alert.text.clone())
        .collect::<Vec<String>>()
        .join("\n\n");

    description
        .translation
        .push(gtfs_realtime::translated_string::Translation {
            text: text.clone(),
            language: Some("en".to_string()),
        });

    Some(GtfsRtAlert {
        description_text: Some(description),
        informed_entity: vec![informed_entity],
        ..Default::default()
    })
}
