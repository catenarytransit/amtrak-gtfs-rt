use std::collections::HashMap;

use gtfs_realtime::Alert;
use serde::Deserialize;
use serde::Serialize;

pub type AsmRoot = Vec<AsmTrain>;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsmTrain {
    #[serde(rename = "train_id")]
    pub train_id: String,
    pub railroad: String,
    #[serde(rename = "origin_date")]
    pub origin_date: String,
    pub number: i64,
    #[serde(rename = "all_numbers")]
    pub all_numbers: Vec<i64>,
    pub name: String,
    pub origin: String,
    pub destination: String,
    #[serde(rename = "partial_train")]
    pub partial_train: bool,
    #[serde(rename = "last_updated")]
    pub last_updated: i64,
    #[serde(rename = "current_timezone")]
    pub current_timezone: String,
    pub threshold: i64,
    pub disruption: bool,
    #[serde(rename = "total_miles")]
    pub total_miles: i64,
    pub location: Location,
    pub stops: Vec<Stop>,
    #[serde(default)]
    pub alerts: Vec<AsmAlert>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
    pub heading: Option<i64>,
    pub speed: f64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Stop {
    pub code: String,
    pub miles: i64,
    #[serde(rename = "sched_depart")]
    pub sched_depart: Option<i64>,
    pub depart: Option<AsmTimepoint>,
    pub canceled: Option<bool>,
    pub arrive: Option<AsmTimepoint>,
    #[serde(rename = "sched_arrive")]
    pub sched_arrive: Option<i64>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AsmTimepoint {
    pub variance: i64,
    #[serde(rename = "times_compared")]
    pub times_compared: String,
    #[serde(rename = "type")]
    pub type_field: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AsmAlert {
    #[serde(rename = "record_time")]
    pub record_time: i64,
    pub text: String,
}

pub fn make_lookup_table_from_asm_root(
    asm_root: AsmRoot,
) -> HashMap<(chrono::NaiveDate, String), Vec<AsmAlert>> {
    let mut lookup_table = HashMap::new();
    for train in asm_root {
        let date = chrono::NaiveDate::parse_from_str(&train.origin_date, "%Y-%m-%d");

        if let Ok(date) = date {
            let train_id = train.train_id;
            for alert in train.alerts {
                lookup_table
                    .entry((date, train_id.clone()))
                    .or_insert(Vec::new())
                    .push(alert);
            }
        }
    }
    lookup_table
}

pub fn asm_alert_to_gtfs_rt(
    informed_entity: gtfs_realtime::EntitySelector,
    asm_alerts: &Vec<AsmAlert>,
) -> Option<gtfs_realtime::Alert> {
    if asm_alerts.len() == 0 {
        return None;
    }

    let mut alert = gtfs_realtime::Alert::default();
    let mut header = gtfs_realtime::TranslatedString::default();
    let mut description = gtfs_realtime::TranslatedString::default();
    let mut url = gtfs_realtime::TranslatedString::default();

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

    Some(Alert {
        description_text: Some(description),
        informed_entity: vec![informed_entity],
        ..Default::default()
    })
}
