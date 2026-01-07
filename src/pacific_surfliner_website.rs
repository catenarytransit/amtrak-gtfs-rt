use gtfs_realtime::FeedEntity;
use gtfs_structures::Gtfs;
use scraper::{Html, Selector};
use std::time::SystemTime;

pub async fn fetch_pacific_surfliner_advisories(
    client: &reqwest::Client,
    gtfs: &Gtfs,
) -> Result<Vec<FeedEntity>, Box<dyn std::error::Error + Sync + Send>> {
    let url = "https://www.pacificsurfliner.com/plan-your-trip/alerts/travel-advisories/";
    let resp = client.get(url).send().await?;
    let text = resp.text().await?;

    // Find Pacific Surfliner route ID
    let route_id = gtfs
        .routes
        .values()
        .find(|r| r.long_name.as_deref() == Some("Pacific Surfliner"))
        .map(|r| r.id.clone());

    if route_id.is_none() {
        return Ok(vec![]);
    }

    Ok(parse_pacific_surfliner_advisories(&text, route_id))
}

pub fn parse_pacific_surfliner_advisories(text: &str, route_id: Option<String>) -> Vec<FeedEntity> {
    let document = Html::parse_document(text);
    let mut alerts = Vec::new();

    let h4_selector = Selector::parse("h4").unwrap();
    // let strong_selector = Selector::parse("strong").unwrap(); // No longer used for simple contains check
    // let orange_selector = Selector::parse(".u-textColor--orange").unwrap();

    for h4 in document.select(&h4_selector) {
        let mut current_node = h4.next_sibling();

        let header_text_all = h4
            .text()
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let is_strict_section = header_text_all.contains("Service Updates")
            || header_text_all.contains("Station Notices");

        while let Some(node) = current_node {
            if let Some(element) = scraper::ElementRef::wrap(node) {
                if element.value().name() == "h4" {
                    break; // Next category
                }

                let check_starts_with_strong = || {
                    for child in element.children() {
                        if let Some(child_el) = scraper::ElementRef::wrap(child) {
                            if child_el.value().name() == "strong" {
                                return true;
                            }
                            // If we hit another element before strong, we assume it's not a strong-start title
                            // unless that element is just a wrapper? assuming direct strong for now as per observation.
                            return false;
                        } else if let Some(child_text) = child.value().as_text() {
                            if !child_text.trim().is_empty() {
                                return false;
                            }
                        }
                    }
                    false
                };

                // Check if this element initiates an alert (Title)
                let is_strong = check_starts_with_strong();
                let is_orange = element
                    .value()
                    .classes()
                    .any(|c| c == "u-textColor--orange");

                // Strict rules: only orange text counts as title in specified sections
                let is_title = if is_strict_section {
                    is_orange
                } else {
                    is_strong || is_orange
                };

                if is_title {
                    let title_text = element
                        .text()
                        .collect::<String>()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");

                    if !title_text.is_empty() {
                        let mut description = String::new();

                        // Consume siblings until next title or header
                        let mut content_sibling = element.next_sibling();
                        while let Some(sib_node) = content_sibling {
                            if let Some(sib) = scraper::ElementRef::wrap(sib_node) {
                                if sib.value().name() == "h4" {
                                    break;
                                }

                                // Check if this is a subheader (Southbound/Northbound)
                                // Use consistent title check logic for current section

                                let check_sib_starts_with_strong = || {
                                    for child in sib.children() {
                                        if let Some(child_el) = scraper::ElementRef::wrap(child) {
                                            if child_el.value().name() == "strong" {
                                                return true;
                                            }
                                            return false;
                                        } else if let Some(child_text) = child.value().as_text() {
                                            if !child_text.trim().is_empty() {
                                                return false;
                                            }
                                        }
                                    }
                                    false
                                };

                                let sib_is_strong = check_sib_starts_with_strong();
                                let sib_is_orange =
                                    sib.value().classes().any(|c| c == "u-textColor--orange");
                                let sib_is_title_candidate = if is_strict_section {
                                    sib_is_orange
                                } else {
                                    sib_is_strong || sib_is_orange
                                };

                                if sib_is_title_candidate {
                                    let header_text = sib
                                        .text()
                                        .collect::<String>()
                                        .split_whitespace()
                                        .collect::<Vec<_>>()
                                        .join(" ");

                                    let lower_text = header_text.to_lowercase();
                                    // always merge Southbound/Northbound, regardless of strictly being a title or not?
                                    // Actually, if it MATCHES title criteria, we check if it's South/North.
                                    // If yes, merge. If no, break (new alert).

                                    if lower_text.contains("southbound")
                                        || lower_text.contains("northbound")
                                    {
                                        description.push_str("\n### ");
                                        description.push_str(&header_text);
                                        description.push_str("\n\n");
                                    } else {
                                        break; // It's a new alert
                                    }
                                } else {
                                    let text = sib
                                        .text()
                                        .collect::<String>()
                                        .split_whitespace()
                                        .collect::<Vec<_>>()
                                        .join(" ");

                                    if !text.is_empty() {
                                        if sib.value().name() == "ul" {
                                            // format list items
                                            for li in sib.select(&Selector::parse("li").unwrap()) {
                                                let li_text = li
                                                    .text()
                                                    .collect::<String>()
                                                    .split_whitespace()
                                                    .collect::<Vec<_>>()
                                                    .join(" ");
                                                description.push_str(&format!("- {}\n", li_text));
                                            }
                                        } else {
                                            description.push_str(&text);
                                            description.push_str("\n\n");
                                        }
                                    }
                                }
                            }

                            content_sibling = sib_node.next_sibling();
                        }

                        // Create alert entity
                        use std::collections::hash_map::DefaultHasher;
                        use std::hash::{Hash, Hasher};
                        let mut hasher = DefaultHasher::new();
                        title_text.hash(&mut hasher);
                        let id_hash = hasher.finish();

                        let id = format!("PAC_SURF_{}", id_hash);

                        let ent = FeedEntity {
                            id,
                            is_deleted: Some(false),
                            trip_update: None,
                            vehicle: None,
                            stop: None,
                            shape: None,
                            trip_modifications: None,
                            alert: Some(gtfs_realtime::Alert {
                                active_period: vec![],
                                informed_entity: vec![gtfs_realtime::EntitySelector {
                                    agency_id: None,
                                    route_id: route_id.clone(),
                                    route_type: None,
                                    trip: None,
                                    stop_id: None,
                                    direction_id: None,
                                }],
                                cause: Some(1),  // Unknown cause
                                effect: Some(8), // Unknown effect
                                url: None,
                                header_text: Some(gtfs_realtime::TranslatedString {
                                    translation: vec![
                                        gtfs_realtime::translated_string::Translation {
                                            text: title_text,
                                            language: Some("en".to_string()),
                                        },
                                    ],
                                }),
                                description_text: Some(gtfs_realtime::TranslatedString {
                                    translation: vec![
                                        gtfs_realtime::translated_string::Translation {
                                            text: description.trim().to_string(),
                                            language: Some("en".to_string()),
                                        },
                                    ],
                                }),
                                tts_header_text: None,
                                tts_description_text: None,
                                severity_level: None,
                                image: None,
                                image_alternative_text: None,
                                cause_detail: None,
                                effect_detail: None,
                            }),
                        };
                        alerts.push(ent);
                        current_node = content_sibling;
                        continue;
                    }
                }
            }

            current_node = node.next_sibling();
        }
    }

    alerts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strict_grouping_station_notices() {
        let html = r#"
        <div class="ContentWidth ContentArea">
            <div>
                <h4><span class="u-textColor--darkBlue" style="text-decoration: underline;">Station Notices</span></h4>
                
                <p class="u-textColor--orange"><strong>IRVINE TRAIN STATION ELEVATOR MAINTENANCE</strong></p>
                <p><em>Updated December 16, 2025</em></p>
                <p>The City of Irvine will perform upcoming maintenance...</p>
                <p><strong>1/7/26 – (All Day)</strong>&nbsp;</p>
                <p>&nbsp;</p>
                
                <p class="u-textColor--orange"><strong>PARKING LOT CLOSED AT GUADALUPE STATION</strong></p>
                <p><em>Updated December 3, 2025</em></p>
                <p>Guadalupe Station Parking Lot...</p>
                
                <p class="u-textColor--orange"><strong>TEMPORARY TICKET WINDOW CLOSURES</strong></p>
                <p>Pacific Surfliner trains continue to serve all stations...</p>
                <ul>
                    <li><strong>San Juan Capistrano</strong>: Nearest staffed station is Anaheim</li>
                    <li><strong>Santa Ana</strong>: Nearest staffed station is Anaheim</li>
                </ul>
                <p><strong>Solana Beach</strong>:</p>
                <ul>
                    <li>Nearest staffed station is Santa Fe Depot...</li>
                </ul>
            </div>
        </div>
        "#;

        let alerts = parse_pacific_surfliner_advisories(html, Some("route_id".to_string()));

        // Expected alerts:
        // 1. IRVINE TRAIN STATION ELEVATOR MAINTENANCE
        // 2. PARKING LOT CLOSED AT GUADALUPE STATION
        // 3. TEMPORARY TICKET WINDOW CLOSURES

        assert_eq!(alerts.len(), 3);

        // Verify specifics
        assert_eq!(
            alerts[0]
                .alert
                .as_ref()
                .unwrap()
                .header_text
                .as_ref()
                .unwrap()
                .translation[0]
                .text,
            "IRVINE TRAIN STATION ELEVATOR MAINTENANCE"
        );
        // Verify that the bold "1/7/26 – (All Day)" was merged into description, not seemingly a new alert
        let desc0 = &alerts[0]
            .alert
            .as_ref()
            .unwrap()
            .description_text
            .as_ref()
            .unwrap()
            .translation[0]
            .text;
        assert!(desc0.contains("1/7/26 – (All Day)"));

        assert_eq!(
            alerts[2]
                .alert
                .as_ref()
                .unwrap()
                .header_text
                .as_ref()
                .unwrap()
                .translation[0]
                .text,
            "TEMPORARY TICKET WINDOW CLOSURES"
        );
        let desc2 = &alerts[2]
            .alert
            .as_ref()
            .unwrap()
            .description_text
            .as_ref()
            .unwrap()
            .translation[0]
            .text;
        // Verify nested bold items are part of description
        assert!(desc2.contains("San Juan Capistrano"));
        assert!(desc2.contains("Solana Beach"));
    }

    #[test]
    fn test_normal_grouping_track_closures() {
        let html = r#"
        <div class="ContentWidth ContentArea">
            <div>
                <h4><span class="u-textColor--darkBlue" style="text-decoration: underline;">Track Closures</span></h4>
                <p><strong>Temporary Track Closure January 6</strong></p>
                <p>Due to weather-related track damage...</p>
            </div>
        </div>
        "#;

        let alerts = parse_pacific_surfliner_advisories(html, Some("route_id".to_string()));

        // Should catch the bold title even if not orange
        assert_eq!(alerts.len(), 1);
        assert_eq!(
            alerts[0]
                .alert
                .as_ref()
                .unwrap()
                .header_text
                .as_ref()
                .unwrap()
                .translation[0]
                .text,
            "Temporary Track Closure January 6"
        );
    }

    #[test]
    fn test_merge_non_start_strong() {
        let html = r#"
        <div class="ContentWidth ContentArea">
            <div>
                <h4><span class="u-textColor--darkBlue" style="text-decoration: underline;">Track Closures</span></h4>
                <p><strong>Temporary Track Closure</strong></p>
                <p>Description text.</p>
                <p>The <strong>bus connections</strong> will be as follows:</p>
                <p>More description.</p>
            </div>
        </div>
        "#;

        let alerts = parse_pacific_surfliner_advisories(html, Some("route_id".to_string()));

        // Should be 1 alert. "The bus connections..." should be merged.
        assert_eq!(alerts.len(), 1);
        assert_eq!(
            alerts[0]
                .alert
                .as_ref()
                .unwrap()
                .header_text
                .as_ref()
                .unwrap()
                .translation[0]
                .text,
            "Temporary Track Closure"
        );

        let desc = &alerts[0]
            .alert
            .as_ref()
            .unwrap()
            .description_text
            .as_ref()
            .unwrap()
            .translation[0]
            .text;
        assert!(desc.contains("The bus connections will be as follows"));
    }
}
