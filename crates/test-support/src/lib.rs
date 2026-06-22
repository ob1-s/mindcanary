use chrono::{Duration, TimeZone, Utc};
use mindcanary_protocol::{
    AggregateBatch, CheckInRecord, ContextTag, Metric, ObservationPeriod, PROTOCOL_VERSION,
    ProtocolRequest, SignalId,
};
use uuid::Uuid;

pub fn synthetic_browser_requests() -> Vec<ProtocolRequest> {
    let source_instance_id = Uuid::from_u128(0x0197_0000_0000_7000_8000_0000_0000_0001);
    let start = Utc.with_ymd_and_hms(2026, 1, 5, 9, 0, 0).unwrap();

    (0_u32..20)
        .map(|sequence| {
            let day = sequence / 4;
            let bucket = sequence % 4;
            let period_start =
                start + Duration::days(i64::from(day)) + Duration::minutes(i64::from(bucket * 15));
            let tab_count = if day < 3 {
                8.0 + f64::from(bucket)
            } else {
                22.0 + f64::from((day - 3) * 2 + bucket)
            };
            let tab_switch_count = if day < 3 {
                4.0 + f64::from(bucket)
            } else {
                12.0 + f64::from((day - 3) * 3 + bucket * 2)
            };

            ProtocolRequest::IngestAggregate {
                protocol_version: PROTOCOL_VERSION,
                batch: AggregateBatch {
                    batch_id: Uuid::from_u128(
                        0x0197_0000_0000_7000_9000_0000_0000_0000 + u128::from(sequence),
                    ),
                    source_instance_id,
                    sequence: u64::from(sequence),
                    period: ObservationPeriod {
                        start: period_start,
                        end: period_start + Duration::minutes(15),
                        time_zone: "America/Sao_Paulo".to_owned(),
                    },
                    metrics: vec![
                        Metric {
                            signal: SignalId::BrowserOpenTabCountMean,
                            value: tab_count,
                        },
                        Metric {
                            signal: SignalId::BrowserOpenTabCountMax,
                            value: tab_count.ceil(),
                        },
                        Metric {
                            signal: SignalId::BrowserTabSwitchCount,
                            value: tab_switch_count,
                        },
                        Metric {
                            signal: SignalId::BrowserActiveSeconds,
                            value: if day < 3 { 480.0 } else { 780.0 },
                        },
                        Metric {
                            signal: SignalId::BrowserIdleSeconds,
                            value: if day < 3 { 300.0 } else { 90.0 },
                        },
                    ],
                },
            }
        })
        .collect()
}

pub fn synthetic_check_in_requests() -> Vec<ProtocolRequest> {
    let start = Utc.with_ymd_and_hms(2026, 1, 5, 21, 0, 0).unwrap();

    (0_u8..5)
        .map(|index| {
            let occurred_at = start + Duration::days(i64::from(index));
            ProtocolRequest::SubmitCheckIn {
                protocol_version: PROTOCOL_VERSION,
                check_in: CheckInRecord {
                    check_in_id: Uuid::from_u128(
                        0x0197_0000_0000_7000_a000_0000_0000_0000 + u128::from(index),
                    ),
                    occurred_at,
                    time_zone: "America/Sao_Paulo".to_owned(),
                    local_date: occurred_at
                        .with_timezone(&chrono_tz::America::Sao_Paulo)
                        .date_naive()
                        .to_string(),
                    sleep_minutes: Some(420_u16.saturating_sub(u16::from(index) * 15)),
                    perceived_sleep_need: Some(4),
                    mood: Some(4 + (index % 2)),
                    energy: Some(3 + index),
                    irritability: Some(2),
                    concentration: Some(4),
                    impulsivity: Some(2 + (index % 3)),
                    medication_taken: Some(true),
                    substance_use: Some(false),
                    context_tags: if index >= 3 {
                        vec![ContextTag::Deadline, ContextTag::NewsCycle]
                    } else {
                        vec![ContextTag::Exercise]
                    },
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_records_are_valid_and_contain_no_content_fields() {
        let mut requests = synthetic_browser_requests();
        requests.extend(synthetic_check_in_requests());
        let validation_time = Utc.with_ymd_and_hms(2026, 1, 6, 0, 0, 0).unwrap();

        for request in &requests {
            request
                .validate_at(validation_time + Duration::days(10))
                .unwrap();
            let json = serde_json::to_string(request).unwrap();
            assert!(!json.contains("\"url\""));
            assert!(!json.contains("\"title\""));
            assert!(!json.contains("\"text\""));
            assert!(!json.contains("\"note\""));
            assert!(!json.contains("\"diagnosis\""));
            assert!(!json.contains("mania"));
        }
    }
}
