use std::collections::BTreeMap;

use chrono::NaiveDate;
use mindcanary_storage::{DailyBrowserFeatures, DailyCheckInFeatures, DailyOsFeatures};

#[derive(Debug, Clone, PartialEq)]
pub struct DailyFeatureSnapshot {
    pub local_date: String,
    pub open_tab_count_mean: Option<f64>,
    pub open_tab_count_max: Option<f64>,
    pub tab_switch_count: Option<f64>,
    pub active_seconds: Option<f64>,
    pub idle_seconds: Option<f64>,
    pub aggregate_bucket_count: u64,
    pub os_active_seconds: Option<f64>,
    pub os_idle_seconds: Option<f64>,
    pub os_aggregate_bucket_count: u64,
    pub sleep_minutes: Option<f64>,
    pub mood: Option<f64>,
    pub energy: Option<f64>,
    pub irritability: Option<f64>,
    pub concentration: Option<f64>,
    pub impulsivity: Option<f64>,
    pub check_in_count: u64,
}

impl DailyFeatureSnapshot {
    fn new(local_date: String) -> Self {
        Self {
            local_date,
            open_tab_count_mean: None,
            open_tab_count_max: None,
            tab_switch_count: None,
            active_seconds: None,
            idle_seconds: None,
            aggregate_bucket_count: 0,
            os_active_seconds: None,
            os_idle_seconds: None,
            os_aggregate_bucket_count: 0,
            sleep_minutes: None,
            mood: None,
            energy: None,
            irritability: None,
            concentration: None,
            impulsivity: None,
            check_in_count: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaselineConfig {
    pub min_baseline_days: usize,
    pub relative_change_threshold: f64,
    pub min_sustained_days: usize,
    pub max_baseline_relative_mad: f64,
}

pub const LAUNCH_BASELINE_CONFIG_VERSION: &str = "local-v1-alpha-2026-06-20-windowed-pooled-rates";
pub const LAUNCH_BASELINE_CONFIG: BaselineConfig = BaselineConfig {
    min_baseline_days: 3,
    relative_change_threshold: 0.25,
    min_sustained_days: 2,
    max_baseline_relative_mad: 0.5,
};

impl Default for BaselineConfig {
    fn default() -> Self {
        LAUNCH_BASELINE_CONFIG
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsightDimension {
    BrowserTabs,
    TabSwitching,
    ActiveTime,
    ComputerActiveTime,
    Sleep,
    Energy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeDirection {
    Higher,
    Lower,
}

impl ChangeDirection {
    const fn as_word(self) -> &'static str {
        match self {
            Self::Higher => "higher",
            Self::Lower => "lower",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insight {
    pub local_date: String,
    pub dimension: InsightDimension,
    pub direction: ChangeDirection,
    pub summary: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadinessStatus {
    ChangeDescribed,
    WithinBaseline,
    NeedsSustainedChange,
    MissingCurrent,
    InsufficientBaseline,
    ZeroBaseline,
    UnstableBaseline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimensionReadiness {
    pub dimension: InsightDimension,
    pub status: ReadinessStatus,
    pub comparable_day_count: usize,
    pub minimum_day_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsightAnalysis {
    pub insights: Vec<Insight>,
    pub readiness: Vec<DimensionReadiness>,
}

#[derive(Debug, Clone, Copy)]
struct MetricDefinition {
    dimension: InsightDimension,
    label: &'static str,
    unit: &'static str,
    value: fn(&DailyFeatureSnapshot) -> Option<f64>,
    coverage_label: &'static str,
    coverage: fn(&DailyFeatureSnapshot) -> u64,
}

#[derive(Debug, Clone, PartialEq)]
struct MetricChange {
    direction: ChangeDirection,
    current_value: f64,
    baseline_median: f64,
    relative_change: f64,
    baseline_day_count: usize,
    baseline_dates: Vec<String>,
    current_dates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct BaselineObservation {
    local_date: String,
    value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BaselineEvaluation {
    Insufficient,
    Zero,
    Unstable,
    Ready { median: f64 },
}

const METRICS: [MetricDefinition; 6] = [
    MetricDefinition {
        dimension: InsightDimension::BrowserTabs,
        label: "Open-tab average",
        unit: "tabs",
        value: open_tab_count_mean,
        coverage_label: "browser periods",
        coverage: browser_bucket_count,
    },
    MetricDefinition {
        dimension: InsightDimension::TabSwitching,
        label: "Tab switching rate",
        unit: "switches/hour",
        value: tab_switches_per_recorded_hour,
        coverage_label: "browser periods",
        coverage: browser_bucket_count,
    },
    MetricDefinition {
        dimension: InsightDimension::ActiveTime,
        label: "Active browser share",
        unit: "%",
        value: browser_active_percent,
        coverage_label: "browser periods",
        coverage: browser_bucket_count,
    },
    MetricDefinition {
        dimension: InsightDimension::ComputerActiveTime,
        label: "Computer active share",
        unit: "%",
        value: os_active_percent,
        coverage_label: "OS periods",
        coverage: os_bucket_count,
    },
    MetricDefinition {
        dimension: InsightDimension::Sleep,
        label: "Sleep duration",
        unit: "minutes",
        value: sleep_minutes,
        coverage_label: "check-ins",
        coverage: check_in_count,
    },
    MetricDefinition {
        dimension: InsightDimension::Energy,
        label: "Energy check-in",
        unit: "",
        value: energy,
        coverage_label: "check-ins",
        coverage: check_in_count,
    },
];

pub fn combine_daily_features(
    browser: &[DailyBrowserFeatures],
    os: &[DailyOsFeatures],
    check_ins: &[DailyCheckInFeatures],
) -> Vec<DailyFeatureSnapshot> {
    let mut snapshots = BTreeMap::<String, DailyFeatureSnapshot>::new();

    for features in browser {
        let snapshot = snapshots
            .entry(features.local_date.clone())
            .or_insert_with(|| DailyFeatureSnapshot::new(features.local_date.clone()));
        snapshot.open_tab_count_mean = features.open_tab_count_mean;
        snapshot.open_tab_count_max = features.open_tab_count_max;
        snapshot.tab_switch_count = features.tab_switch_count;
        snapshot.active_seconds = features.active_seconds;
        snapshot.idle_seconds = features.idle_seconds;
        snapshot.aggregate_bucket_count = features.aggregate_bucket_count;
    }

    for features in os {
        let snapshot = snapshots
            .entry(features.local_date.clone())
            .or_insert_with(|| DailyFeatureSnapshot::new(features.local_date.clone()));
        snapshot.os_active_seconds = features.active_seconds;
        snapshot.os_idle_seconds = features.idle_seconds;
        snapshot.os_aggregate_bucket_count = features.aggregate_bucket_count;
    }

    for features in check_ins {
        let snapshot = snapshots
            .entry(features.local_date.clone())
            .or_insert_with(|| DailyFeatureSnapshot::new(features.local_date.clone()));
        snapshot.sleep_minutes = features.sleep_minutes;
        snapshot.mood = features.mood;
        snapshot.energy = features.energy;
        snapshot.irritability = features.irritability;
        snapshot.concentration = features.concentration;
        snapshot.impulsivity = features.impulsivity;
        snapshot.check_in_count = features.check_in_count;
    }

    snapshots.into_values().collect()
}

pub fn generate_insights(
    snapshots: &[DailyFeatureSnapshot],
    config: BaselineConfig,
) -> Vec<Insight> {
    analyze_insights(snapshots, config).insights
}

pub fn analyze_insights(
    snapshots: &[DailyFeatureSnapshot],
    config: BaselineConfig,
) -> InsightAnalysis {
    if config.min_baseline_days == 0
        || !config.relative_change_threshold.is_finite()
        || config.relative_change_threshold <= 0.0
        || config.min_sustained_days < 2
        || !config.max_baseline_relative_mad.is_finite()
        || config.max_baseline_relative_mad <= 0.0
    {
        return InsightAnalysis {
            insights: Vec::new(),
            readiness: Vec::new(),
        };
    }

    let first_window_end = config.min_baseline_days + config.min_sustained_days - 1;
    let insights = snapshots
        .len()
        .checked_sub(1)
        .filter(|index| *index >= first_window_end)
        .map(|index| {
            METRICS
                .into_iter()
                .filter_map(|metric| insight_for_metric(snapshots, index, metric, config))
                .collect()
        })
        .unwrap_or_default();

    let readiness = latest_readiness(snapshots, config);

    InsightAnalysis {
        insights,
        readiness,
    }
}

fn insight_for_metric(
    snapshots: &[DailyFeatureSnapshot],
    index: usize,
    metric: MetricDefinition,
    config: BaselineConfig,
) -> Option<Insight> {
    let current = snapshots.get(index)?;
    let window = current_window(snapshots, index, config.min_sustained_days)?;
    let change = metric_change_for_window(snapshots, index, metric, config)?;

    Some(Insight {
        local_date: current.local_date.clone(),
        dimension: metric.dimension,
        direction: change.direction,
        summary: format!(
            "Across a {}-day window, {} was {} than your prior personal baseline.",
            change.current_dates.len(),
            metric.label,
            change.direction.as_word()
        ),
        evidence: vec![
            Evidence {
                label: "current window median".to_owned(),
                value: format_measure(change.current_value, metric.unit),
            },
            Evidence {
                label: "current window".to_owned(),
                value: format!(
                    "{} days: {}",
                    change.current_dates.len(),
                    change.current_dates.join(", ")
                ),
            },
            Evidence {
                label: "baseline median".to_owned(),
                value: format_measure(change.baseline_median, metric.unit),
            },
            Evidence {
                label: "change".to_owned(),
                value: format!("{:+.0}%", change.relative_change * 100.0),
            },
            Evidence {
                label: "baseline days".to_owned(),
                value: format!("{} prior days", change.baseline_day_count),
            },
            Evidence {
                label: "prior dates".to_owned(),
                value: change.baseline_dates.join(", "),
            },
            Evidence {
                label: "current coverage".to_owned(),
                value: format_coverage(
                    window
                        .iter()
                        .map(|snapshot| (metric.coverage)(snapshot))
                        .sum(),
                    metric.coverage_label,
                ),
            },
        ],
    })
}

fn latest_readiness(
    snapshots: &[DailyFeatureSnapshot],
    config: BaselineConfig,
) -> Vec<DimensionReadiness> {
    if snapshots.is_empty() {
        return METRICS
            .iter()
            .map(|metric| DimensionReadiness {
                dimension: metric.dimension,
                status: ReadinessStatus::MissingCurrent,
                comparable_day_count: 0,
                minimum_day_count: config.min_baseline_days,
            })
            .collect();
    }

    let latest_index = snapshots.len() - 1;
    METRICS
        .iter()
        .map(|metric| readiness_for_metric(snapshots, latest_index, *metric, config))
        .collect()
}

fn readiness_for_metric(
    snapshots: &[DailyFeatureSnapshot],
    index: usize,
    metric: MetricDefinition,
    config: BaselineConfig,
) -> DimensionReadiness {
    let Some(window) = current_window(snapshots, index, config.min_sustained_days) else {
        return DimensionReadiness {
            dimension: metric.dimension,
            status: ReadinessStatus::MissingCurrent,
            comparable_day_count: 0,
            minimum_day_count: config.min_baseline_days,
        };
    };
    let window_start = index + 1 - window.len();
    let baseline_values = prior_metric_observations(&snapshots[..window_start], metric)
        .into_iter()
        .map(|observation| observation.value)
        .collect::<Vec<_>>();
    let comparable_day_count = baseline_values.len();
    let current_values = window
        .iter()
        .filter_map(|snapshot| (metric.value)(snapshot).filter(|value| value.is_finite()))
        .collect::<Vec<_>>();
    let status = if current_values.len() != window.len() {
        ReadinessStatus::MissingCurrent
    } else if comparable_day_count < config.min_baseline_days {
        ReadinessStatus::InsufficientBaseline
    } else {
        match evaluate_baseline(&baseline_values, config) {
            BaselineEvaluation::Insufficient => ReadinessStatus::InsufficientBaseline,
            BaselineEvaluation::Zero => ReadinessStatus::ZeroBaseline,
            BaselineEvaluation::Unstable => ReadinessStatus::UnstableBaseline,
            BaselineEvaluation::Ready {
                median: baseline_median,
            } => window_change_direction(
                &current_values,
                baseline_median,
                config.relative_change_threshold,
            )
            .map_or(ReadinessStatus::WithinBaseline, |_| {
                ReadinessStatus::ChangeDescribed
            }),
        }
    };

    DimensionReadiness {
        dimension: metric.dimension,
        status,
        comparable_day_count,
        minimum_day_count: config.min_baseline_days,
    }
}

fn metric_change_for_window(
    snapshots: &[DailyFeatureSnapshot],
    index: usize,
    metric: MetricDefinition,
    config: BaselineConfig,
) -> Option<MetricChange> {
    let window = current_window(snapshots, index, config.min_sustained_days)?;
    let window_start = index + 1 - window.len();
    let baseline_observations = prior_metric_observations(&snapshots[..window_start], metric);
    if baseline_observations.len() < config.min_baseline_days {
        return None;
    }

    let current_values = window
        .iter()
        .map(|snapshot| (metric.value)(snapshot).filter(|value| value.is_finite()))
        .collect::<Option<Vec<_>>>()?;
    let current_value = median(current_values.clone())?;
    let baseline_day_count = baseline_observations.len();
    let baseline_dates = baseline_observations
        .iter()
        .map(|observation| observation.local_date.clone())
        .collect::<Vec<_>>();
    let baseline_values = baseline_observations
        .into_iter()
        .map(|observation| observation.value)
        .collect::<Vec<_>>();
    let BaselineEvaluation::Ready {
        median: baseline_median,
    } = evaluate_baseline(&baseline_values, config)
    else {
        return None;
    };
    let direction = window_change_direction(
        &current_values,
        baseline_median,
        config.relative_change_threshold,
    )?;
    let relative_change = (current_value - baseline_median) / baseline_median;

    Some(MetricChange {
        direction,
        current_value,
        baseline_median,
        relative_change,
        baseline_day_count,
        baseline_dates,
        current_dates: window
            .iter()
            .map(|snapshot| snapshot.local_date.clone())
            .collect(),
    })
}

fn current_window(
    snapshots: &[DailyFeatureSnapshot],
    index: usize,
    window_days: usize,
) -> Option<&[DailyFeatureSnapshot]> {
    if window_days < 2 || index + 1 < window_days {
        return None;
    }
    let window = &snapshots[index + 1 - window_days..=index];
    window
        .windows(2)
        .all(|pair| local_date(&pair[0]).and_then(|date| date.succ_opt()) == local_date(&pair[1]))
        .then_some(window)
}

fn window_change_direction(
    values: &[f64],
    baseline_median: f64,
    threshold: f64,
) -> Option<ChangeDirection> {
    let mut direction = None;
    for value in values {
        let relative_change = (value - baseline_median) / baseline_median;
        if relative_change.abs() < threshold {
            return None;
        }
        let candidate = if relative_change.is_sign_positive() {
            ChangeDirection::Higher
        } else {
            ChangeDirection::Lower
        };
        if direction.is_some_and(|current| current != candidate) {
            return None;
        }
        direction = Some(candidate);
    }
    direction
}

fn prior_metric_observations(
    snapshots: &[DailyFeatureSnapshot],
    metric: MetricDefinition,
) -> Vec<BaselineObservation> {
    snapshots
        .iter()
        .filter_map(|snapshot| {
            (metric.value)(snapshot)
                .filter(|value| value.is_finite())
                .map(|value| BaselineObservation {
                    local_date: snapshot.local_date.clone(),
                    value,
                })
        })
        .collect()
}

fn evaluate_baseline(values: &[f64], config: BaselineConfig) -> BaselineEvaluation {
    if values.len() < config.min_baseline_days {
        return BaselineEvaluation::Insufficient;
    }

    let Some(baseline_median) = median(values.to_vec()) else {
        return BaselineEvaluation::Insufficient;
    };
    if baseline_median.abs() < f64::EPSILON {
        return BaselineEvaluation::Zero;
    }

    let Some(relative_mad) = relative_median_absolute_deviation(values, baseline_median) else {
        return BaselineEvaluation::Insufficient;
    };
    if relative_mad > config.max_baseline_relative_mad {
        return BaselineEvaluation::Unstable;
    }

    BaselineEvaluation::Ready {
        median: baseline_median,
    }
}

fn relative_median_absolute_deviation(values: &[f64], baseline_median: f64) -> Option<f64> {
    let deviations = values
        .iter()
        .map(|value| (value - baseline_median).abs())
        .collect::<Vec<_>>();
    median(deviations).map(|mad| mad / baseline_median.abs())
}

fn local_date(snapshot: &DailyFeatureSnapshot) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&snapshot.local_date, "%Y-%m-%d").ok()
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    values.retain(|value| value.is_finite());
    if values.is_empty() {
        return None;
    }

    values.sort_by(f64::total_cmp);
    let midpoint = values.len() / 2;
    if values.len() % 2 == 0 {
        Some((values[midpoint - 1] + values[midpoint]) / 2.0)
    } else {
        Some(values[midpoint])
    }
}

fn format_measure(value: f64, unit: &str) -> String {
    let value = format_number(value);
    match unit {
        "" => value,
        "%" => format!("{value}%"),
        _ => format!("{value} {unit}"),
    }
}

fn format_number(value: f64) -> String {
    if (value.fract()).abs() < 0.05 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

fn format_coverage(count: u64, label: &str) -> String {
    format!("{count} {label}")
}

fn open_tab_count_mean(snapshot: &DailyFeatureSnapshot) -> Option<f64> {
    snapshot.open_tab_count_mean
}

fn tab_switches_per_recorded_hour(snapshot: &DailyFeatureSnapshot) -> Option<f64> {
    let recorded_hours = recorded_hours(snapshot.aggregate_bucket_count)?;
    snapshot
        .tab_switch_count
        .map(|switches| switches / recorded_hours)
}

fn browser_active_percent(snapshot: &DailyFeatureSnapshot) -> Option<f64> {
    active_percent(snapshot.active_seconds, snapshot.aggregate_bucket_count)
}

fn os_active_percent(snapshot: &DailyFeatureSnapshot) -> Option<f64> {
    active_percent(
        snapshot.os_active_seconds,
        snapshot.os_aggregate_bucket_count,
    )
}

fn active_percent(active_seconds: Option<f64>, period_count: u64) -> Option<f64> {
    let recorded_hours = recorded_hours(period_count)?;
    active_seconds.map(|seconds| seconds / (recorded_hours * 3_600.0) * 100.0)
}

fn recorded_hours(period_count: u64) -> Option<f64> {
    if period_count == 0 {
        return None;
    }
    let period_count = u32::try_from(period_count).ok()?;
    Some(f64::from(period_count) / 4.0)
}

fn sleep_minutes(snapshot: &DailyFeatureSnapshot) -> Option<f64> {
    snapshot.sleep_minutes
}

fn energy(snapshot: &DailyFeatureSnapshot) -> Option<f64> {
    snapshot.energy
}

const fn browser_bucket_count(snapshot: &DailyFeatureSnapshot) -> u64 {
    snapshot.aggregate_bucket_count
}

const fn os_bucket_count(snapshot: &DailyFeatureSnapshot) -> u64 {
    snapshot.os_aggregate_bucket_count
}

const fn check_in_count(snapshot: &DailyFeatureSnapshot) -> u64 {
    snapshot.check_in_count
}

#[cfg(test)]
mod tests {
    use mindcanary_protocol::{IngestDisposition, ProtocolRequest};
    use mindcanary_storage::{DatabaseKey, EncryptedStore};
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn combines_browser_and_check_in_days() {
        let snapshots = combine_daily_features(
            &[DailyBrowserFeatures {
                local_date: "2026-01-05".to_owned(),
                open_tab_count_mean: Some(9.5),
                open_tab_count_max: Some(11.0),
                tab_switch_count: Some(22.0),
                retained_across_day_count: Some(5.0),
                continuous_scrolling_seconds: None,
                active_seconds: Some(1_920.0),
                idle_seconds: Some(1_200.0),
                aggregate_bucket_count: 4,
            }],
            &[DailyOsFeatures {
                local_date: "2026-01-05".to_owned(),
                active_seconds: Some(3_600.0),
                idle_seconds: Some(900.0),
                lock_count: Some(1.0),
                unlock_count: Some(1.0),
                suspend_count: None,
                resume_count: None,
                aggregate_bucket_count: 4,
            }],
            &[DailyCheckInFeatures {
                local_date: "2026-01-05".to_owned(),
                sleep_minutes: Some(420.0),
                mood: Some(4.0),
                energy: Some(3.0),
                irritability: Some(2.0),
                concentration: Some(4.0),
                impulsivity: Some(2.0),
                check_in_count: 1,
                context_tags: Vec::new(),
            }],
        );

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].local_date, "2026-01-05");
        assert_eq!(snapshots[0].open_tab_count_mean, Some(9.5));
        assert_eq!(snapshots[0].os_active_seconds, Some(3_600.0));
        assert_eq!(snapshots[0].energy, Some(3.0));
    }

    #[test]
    fn detects_changes_from_personal_baseline() {
        let snapshots = vec![
            snapshot("2026-01-05", Some(9.0), Some(24.0), Some(3.0)),
            snapshot("2026-01-06", Some(10.0), Some(26.0), Some(4.0)),
            snapshot("2026-01-07", Some(9.5), Some(25.0), Some(3.0)),
            snapshot("2026-01-08", Some(24.0), Some(70.0), Some(6.0)),
            snapshot("2026-01-09", Some(25.0), Some(72.0), Some(6.0)),
        ];

        let insights = generate_insights(&snapshots, BaselineConfig::default());

        assert!(
            insights
                .iter()
                .any(|insight| insight.dimension == InsightDimension::BrowserTabs)
        );
        assert!(
            insights
                .iter()
                .any(|insight| insight.dimension == InsightDimension::Energy)
        );
        assert!(insights.iter().all(|insight| {
            insight
                .evidence
                .iter()
                .any(|evidence| evidence.label == "baseline days")
        }));
        assert!(insights.iter().all(|insight| {
            insight
                .evidence
                .iter()
                .any(|evidence| evidence.label == "prior dates")
        }));
        assert!(insights.iter().all(|insight| {
            insight
                .evidence
                .iter()
                .any(|evidence| evidence.label == "current window")
        }));
        assert!(insights.iter().all(|insight| {
            insight
                .evidence
                .iter()
                .any(|evidence| evidence.label == "current coverage")
        }));
    }

    #[test]
    fn only_describes_the_latest_complete_window() {
        let snapshots = vec![
            snapshot("2026-01-05", Some(10.0), None, None),
            snapshot("2026-01-06", Some(10.0), None, None),
            snapshot("2026-01-07", Some(10.0), None, None),
            snapshot("2026-01-08", Some(30.0), None, None),
            snapshot("2026-01-09", Some(30.0), None, None),
            snapshot("2026-01-10", Some(10.0), None, None),
        ];

        let analysis = analyze_insights(&snapshots, BaselineConfig::default());

        assert!(
            analysis.insights.is_empty(),
            "old changed windows should not remain visible as current changes"
        );
        let browser_tabs = analysis
            .readiness
            .iter()
            .find(|readiness| readiness.dimension == InsightDimension::BrowserTabs)
            .unwrap();
        assert_eq!(browser_tabs.status, ReadinessStatus::WithinBaseline);
    }

    #[test]
    fn does_not_describe_an_isolated_one_day_spike() {
        let snapshots = vec![
            snapshot("2026-01-05", Some(10.0), None, None),
            snapshot("2026-01-06", Some(10.0), None, None),
            snapshot("2026-01-07", Some(10.0), None, None),
            snapshot("2026-01-08", Some(28.0), None, None),
            snapshot("2026-01-09", Some(10.0), None, None),
        ];

        let analysis = analyze_insights(&snapshots, BaselineConfig::default());

        assert!(analysis.insights.is_empty());
        let browser_tabs = analysis
            .readiness
            .iter()
            .find(|readiness| readiness.dimension == InsightDimension::BrowserTabs)
            .unwrap();
        assert_eq!(browser_tabs.status, ReadinessStatus::WithinBaseline);
    }

    #[test]
    fn normalizes_cumulative_browser_signals_by_recorded_coverage() {
        let mut snapshots = vec![
            snapshot("2026-01-05", None, Some(240.0), None),
            snapshot("2026-01-06", None, Some(240.0), None),
            snapshot("2026-01-07", None, Some(240.0), None),
            snapshot("2026-01-08", None, Some(240.0), None),
            snapshot("2026-01-09", None, Some(50.0), None),
        ];
        for snapshot in &mut snapshots[..4] {
            snapshot.aggregate_bucket_count = 96;
        }
        snapshots[4].aggregate_bucket_count = 20;

        let analysis = analyze_insights(&snapshots, BaselineConfig::default());

        assert!(analysis.insights.is_empty());
        let tab_switching = analysis
            .readiness
            .iter()
            .find(|readiness| readiness.dimension == InsightDimension::TabSwitching)
            .unwrap();
        assert_eq!(tab_switching.status, ReadinessStatus::WithinBaseline);
    }

    #[test]
    fn keeps_a_mixed_window_out_of_descriptions() {
        let snapshots = vec![
            snapshot("2026-01-05", Some(10.0), None, None),
            snapshot("2026-01-06", Some(10.0), None, None),
            snapshot("2026-01-07", Some(10.0), None, None),
            snapshot("2026-01-08", Some(10.0), None, None),
            snapshot("2026-01-09", Some(28.0), None, None),
        ];

        let analysis = analyze_insights(&snapshots, BaselineConfig::default());

        assert!(analysis.insights.is_empty());
        let browser_tabs = analysis
            .readiness
            .iter()
            .find(|readiness| readiness.dimension == InsightDimension::BrowserTabs)
            .unwrap();
        assert_eq!(browser_tabs.status, ReadinessStatus::WithinBaseline);
    }

    #[test]
    fn uses_all_prior_days_by_default() {
        let config = BaselineConfig {
            min_baseline_days: 3,
            relative_change_threshold: 0.25,
            min_sustained_days: 2,
            max_baseline_relative_mad: 0.5,
        };
        let snapshots = vec![
            snapshot("2026-01-05", Some(10.0), None, None),
            snapshot("2026-01-06", Some(11.0), None, None),
            snapshot("2026-01-07", Some(10.0), None, None),
            snapshot("2026-01-10", Some(60.0), None, None),
            snapshot("2026-01-11", Some(70.0), None, None),
        ];

        let insights = generate_insights(&snapshots, config);
        let insight = insights
            .iter()
            .find(|insight| insight.dimension == InsightDimension::BrowserTabs)
            .unwrap();

        assert_eq!(insight.local_date, "2026-01-11");
        assert!(
            insight
                .evidence
                .iter()
                .any(|evidence| evidence.label == "baseline days"
                    && evidence.value == "3 prior days")
        );
        assert!(
            insight
                .evidence
                .iter()
                .any(|evidence| evidence.label == "prior dates"
                    && evidence.value == "2026-01-05, 2026-01-06, 2026-01-07")
        );
    }

    #[test]
    fn explains_latest_day_abstention_per_dimension() {
        let insufficient = analyze_insights(
            &[
                snapshot("2026-01-05", Some(9.0), None, None),
                snapshot("2026-01-06", Some(10.0), None, None),
            ],
            BaselineConfig::default(),
        );
        let browser_tabs = insufficient
            .readiness
            .iter()
            .find(|readiness| readiness.dimension == InsightDimension::BrowserTabs)
            .unwrap();
        assert_eq!(browser_tabs.status, ReadinessStatus::InsufficientBaseline);
        assert_eq!(browser_tabs.comparable_day_count, 0);
        assert_eq!(browser_tabs.minimum_day_count, 3);

        let stable = analyze_insights(
            &[
                snapshot("2026-01-05", Some(10.0), None, None),
                snapshot("2026-01-06", Some(10.0), None, None),
                snapshot("2026-01-07", Some(10.0), None, None),
                snapshot("2026-01-08", Some(11.0), None, None),
                snapshot("2026-01-09", Some(11.0), None, None),
            ],
            BaselineConfig::default(),
        );
        let browser_tabs = stable
            .readiness
            .iter()
            .find(|readiness| readiness.dimension == InsightDimension::BrowserTabs)
            .unwrap();
        assert_eq!(browser_tabs.status, ReadinessStatus::WithinBaseline);
        assert_eq!(browser_tabs.comparable_day_count, 3);

        let energy = stable
            .readiness
            .iter()
            .find(|readiness| readiness.dimension == InsightDimension::Energy)
            .unwrap();
        assert_eq!(energy.status, ReadinessStatus::MissingCurrent);
    }

    #[test]
    fn abstains_when_the_comparable_baseline_is_too_variable() {
        let snapshots = vec![
            snapshot("2026-01-05", Some(8.0), None, None),
            snapshot("2026-01-06", Some(28.0), None, None),
            snapshot("2026-01-07", Some(48.0), None, None),
            snapshot("2026-01-08", Some(70.0), None, None),
            snapshot("2026-01-09", Some(72.0), None, None),
        ];

        let analysis = analyze_insights(&snapshots, BaselineConfig::default());

        assert!(analysis.insights.is_empty());
        let browser_tabs = analysis
            .readiness
            .iter()
            .find(|readiness| readiness.dimension == InsightDimension::BrowserTabs)
            .unwrap();
        assert_eq!(browser_tabs.status, ReadinessStatus::UnstableBaseline);
        assert_eq!(browser_tabs.comparable_day_count, 3);
    }

    #[test]
    fn launch_rule_meets_the_synthetic_false_nudge_budget() {
        assert_eq!(
            LAUNCH_BASELINE_CONFIG_VERSION,
            "local-v1-alpha-2026-06-20-windowed-pooled-rates"
        );

        let quiet_cases = [
            (
                "stable routine with ordinary noise",
                vec![
                    snapshot("2026-01-05", Some(10.0), Some(24.0), Some(4.0)),
                    snapshot("2026-01-06", Some(10.5), Some(26.0), Some(4.0)),
                    snapshot("2026-01-07", Some(9.8), Some(25.0), Some(4.0)),
                    snapshot("2026-01-08", Some(10.1), Some(27.0), Some(5.0)),
                    snapshot("2026-01-09", Some(10.2), Some(24.0), Some(4.0)),
                ],
            ),
            (
                "one isolated spike",
                vec![
                    snapshot("2026-01-05", Some(10.0), None, None),
                    snapshot("2026-01-06", Some(10.0), None, None),
                    snapshot("2026-01-07", Some(10.0), None, None),
                    snapshot("2026-01-08", Some(28.0), None, None),
                    snapshot("2026-01-09", Some(10.0), None, None),
                ],
            ),
            (
                "zero-heavy count baseline",
                vec![
                    snapshot("2026-01-05", Some(0.0), None, None),
                    snapshot("2026-01-06", Some(0.0), None, None),
                    snapshot("2026-01-07", Some(0.0), None, None),
                    snapshot("2026-01-08", Some(4.0), None, None),
                    snapshot("2026-01-09", Some(4.0), None, None),
                ],
            ),
            (
                "unstable baseline",
                vec![
                    snapshot("2026-01-05", Some(8.0), None, None),
                    snapshot("2026-01-06", Some(28.0), None, None),
                    snapshot("2026-01-07", Some(48.0), None, None),
                    snapshot("2026-01-08", Some(70.0), None, None),
                    snapshot("2026-01-09", Some(72.0), None, None),
                ],
            ),
        ];

        for (name, snapshots) in quiet_cases {
            let analysis = analyze_insights(&snapshots, LAUNCH_BASELINE_CONFIG);
            assert!(
                analysis.insights.is_empty(),
                "{name} produced synthetic false nudges: {:?}",
                analysis.insights
            );
        }

        let schedule_shift = vec![
            snapshot("2026-01-05", Some(10.0), None, None),
            snapshot("2026-01-06", Some(11.0), None, None),
            snapshot("2026-01-07", Some(10.0), None, None),
            snapshot("2026-01-10", Some(60.0), None, None),
            snapshot("2026-01-11", Some(70.0), None, None),
        ];
        let schedule_analysis = analyze_insights(&schedule_shift, LAUNCH_BASELINE_CONFIG);
        assert_eq!(schedule_analysis.insights.len(), 1);
        assert_eq!(
            schedule_analysis.insights[0].dimension,
            InsightDimension::BrowserTabs
        );
        assert!(
            schedule_analysis.insights[0]
                .evidence
                .iter()
                .any(|evidence| evidence.label == "prior dates")
        );

        let sustained_shift = vec![
            snapshot("2026-01-05", Some(9.0), Some(24.0), Some(3.0)),
            snapshot("2026-01-06", Some(10.0), Some(26.0), Some(4.0)),
            snapshot("2026-01-07", Some(9.5), Some(25.0), Some(3.0)),
            snapshot("2026-01-08", Some(24.0), Some(70.0), Some(6.0)),
            snapshot("2026-01-09", Some(25.0), Some(72.0), Some(6.0)),
        ];
        let analysis = analyze_insights(&sustained_shift, LAUNCH_BASELINE_CONFIG);
        assert!(
            analysis
                .insights
                .iter()
                .any(|insight| insight.dimension == InsightDimension::BrowserTabs)
        );
        assert!(
            analysis
                .insights
                .iter()
                .any(|insight| insight.dimension == InsightDimension::Energy)
        );
        assert_neutral_language(&analysis.insights);
    }

    #[test]
    fn synthetic_fixtures_produce_local_personal_rhythm_insights() {
        let temp = TempDir::new().unwrap();
        let key = DatabaseKey::from_bytes([42_u8; 32]);
        let mut store = EncryptedStore::open(temp.path().join("mindcanary.db"), &key).unwrap();

        let mut requests = mindcanary_test_support::synthetic_browser_requests();
        requests.extend(mindcanary_test_support::synthetic_check_in_requests());
        for request in requests {
            ingest_request(&mut store, request);
        }

        let browser = store.daily_browser_features().unwrap();
        let os = store.daily_os_features().unwrap();
        let check_ins = store.daily_check_in_features().unwrap();
        let snapshots = combine_daily_features(&browser, &os, &check_ins);
        let insights = generate_insights(&snapshots, BaselineConfig::default());

        assert_eq!(browser.len(), 5);
        assert_eq!(check_ins.len(), 5);
        assert!(
            insights
                .iter()
                .any(|insight| insight.dimension == InsightDimension::BrowserTabs)
        );
        assert!(
            insights
                .iter()
                .any(|insight| insight.dimension == InsightDimension::Energy)
        );
        assert_neutral_language(&insights);
    }

    fn snapshot(
        local_date: &str,
        open_tab_count_mean: Option<f64>,
        tab_switch_count: Option<f64>,
        energy: Option<f64>,
    ) -> DailyFeatureSnapshot {
        DailyFeatureSnapshot {
            local_date: local_date.to_owned(),
            open_tab_count_mean,
            open_tab_count_max: None,
            tab_switch_count,
            active_seconds: None,
            idle_seconds: None,
            aggregate_bucket_count: 4,
            os_active_seconds: None,
            os_idle_seconds: None,
            os_aggregate_bucket_count: 0,
            sleep_minutes: None,
            mood: None,
            energy,
            irritability: None,
            concentration: None,
            impulsivity: None,
            check_in_count: 1,
        }
    }

    fn ingest_request(store: &mut EncryptedStore, request: ProtocolRequest) {
        match request {
            ProtocolRequest::Health { .. }
            | ProtocolRequest::GetSourceStatus { .. }
            | ProtocolRequest::GetDailyRhythmInsights { .. }
            | ProtocolRequest::GetDailyTimeline { .. }
            | ProtocolRequest::PrepareDeleteLatestCheckIn { .. }
            | ProtocolRequest::DeleteLatestCheckIn { .. }
            | ProtocolRequest::GetCollectionSettings { .. }
            | ProtocolRequest::GetPlatformCapabilities { .. }
            | ProtocolRequest::SetSignalCollection { .. }
            | ProtocolRequest::PrepareDeleteSignalRecords { .. }
            | ProtocolRequest::DeleteSignalRecords { .. }
            | ProtocolRequest::GetLocalDataSummary { .. }
            | ProtocolRequest::PrepareExportLocalRecords { .. }
            | ProtocolRequest::ExportLocalRecords { .. }
            | ProtocolRequest::PrepareCreateLocalBackup { .. }
            | ProtocolRequest::CreateLocalBackup { .. }
            | ProtocolRequest::VerifyLocalBackup { .. }
            | ProtocolRequest::RestoreLocalBackup { .. }
            | ProtocolRequest::PrepareClearLocalRecords { .. }
            | ProtocolRequest::ClearLocalRecords { .. } => {
                unreachable!("fixture request should ingest data");
            }
            ProtocolRequest::SaveAnnotation { .. }
            | ProtocolRequest::PrepareDeleteAnnotation { .. }
            | ProtocolRequest::DeleteAnnotation { .. } => {
                unreachable!("analytics fixtures do not consume annotations");
            }
            ProtocolRequest::IngestAggregate { batch, .. } => {
                assert_eq!(store.ingest(&batch).unwrap(), IngestDisposition::Stored);
            }
            ProtocolRequest::SubmitCheckIn { check_in, .. } => {
                assert_eq!(
                    store.submit_check_in(&check_in).unwrap(),
                    IngestDisposition::Stored
                );
            }
        }
    }

    fn assert_neutral_language(insights: &[Insight]) {
        const BLOCKED_TERMS: [&str; 8] = [
            "mania",
            "manic",
            "depression",
            "depressive",
            "psychosis",
            "diagnosis",
            "warning",
            "risk",
        ];

        for insight in insights {
            let mut text = insight.summary.to_lowercase();
            for evidence in &insight.evidence {
                text.push(' ');
                text.push_str(&evidence.label.to_lowercase());
                text.push(' ');
                text.push_str(&evidence.value.to_lowercase());
            }

            for blocked in BLOCKED_TERMS {
                assert!(
                    !text.contains(blocked),
                    "insight used blocked term {blocked:?}: {text}"
                );
            }
        }
    }
}
