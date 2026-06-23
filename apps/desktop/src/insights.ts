import {
  DEFAULT_DAILY_RHYTHM_INSIGHT_LIMIT,
  MAX_DAILY_RHYTHM_INSIGHT_LIMIT,
  PROTOCOL_VERSION,
  type DailyRhythmSummary,
  type ProtocolRequest,
  type ProtocolResponse,
  type RhythmChangeDirection,
  type RhythmDimensionReadiness,
  type RhythmEvidence,
  type RhythmInsight,
  type RhythmInsightDimension,
  type RhythmReadinessStatus,
} from "@mindcanary/protocol";

export type DailyRhythmDashboardModel =
  | ReadyDailyRhythmDashboardModel
  | EmptyDailyRhythmDashboardModel
  | UnavailableDailyRhythmDashboardModel;

export interface ReadyDailyRhythmDashboardModel {
  state: "ready";
  generatedAt: string;
  coverageText: string;
  isTruncated: boolean;
  cards: InsightCardModel[];
  readiness: ReadinessItemModel[];
}

export interface EmptyDailyRhythmDashboardModel {
  state: "empty";
  generatedAt: string;
  coverageText: string;
  emptyTitle: string;
  emptyBody: string;
  baselineProgressText?: string;
  cards: [];
  readiness: ReadinessItemModel[];
}

export interface UnavailableDailyRhythmDashboardModel {
  state: "unavailable";
  message: string;
  cards: [];
}

export interface InsightCardModel {
  localDate: string;
  dimensionLabel: string;
  changeLabel: string;
  summary: string;
  evidence: string[];
}

export interface ReadinessItemModel {
  dimensionLabel: string;
  statusLabel: string;
  detail: string;
  state: "described" | "stable" | "waiting" | "missing";
}

const DIMENSION_LABELS: Record<RhythmInsightDimension, string> = {
  browser_tabs: "Browser tabs",
  tab_switching: "Tab switching rate",
  active_time: "Active browser share",
  computer_active_time: "Computer active share",
  sleep: "Sleep",
  energy: "Energy check-in",
};

const DIRECTION_LABELS: Record<RhythmChangeDirection, string> = {
  higher: "Higher than baseline",
  lower: "Lower than baseline",
};

const READINESS_LABELS: Record<RhythmReadinessStatus, string> = {
  change_described: "Change noted",
  within_baseline: "Within your range",
  needs_sustained_change: "No clear shift yet",
  missing_current: "No recent value",
  insufficient_baseline: "Building history",
  zero_baseline: "Needs more variety",
  unstable_baseline: "History still varies",
};

export function createDailyRhythmInsightsRequest(
  limit = DEFAULT_DAILY_RHYTHM_INSIGHT_LIMIT,
): ProtocolRequest {
  if (
    !Number.isInteger(limit) ||
    limit < 1 ||
    limit > MAX_DAILY_RHYTHM_INSIGHT_LIMIT
  ) {
    throw new RangeError(
      `Daily rhythm insight limit must be between 1 and ${MAX_DAILY_RHYTHM_INSIGHT_LIMIT}.`,
    );
  }

  return {
    type: "get_daily_rhythm_insights",
    protocol_version: PROTOCOL_VERSION,
    limit,
  };
}

export function toDailyRhythmDashboardModel(
  response: ProtocolResponse,
): DailyRhythmDashboardModel {
  if (response.type === "error") {
    return {
      state: "unavailable",
      message:
        "Insights are unavailable right now. Your local data remains on this device.",
      cards: [],
    };
  }

  if (response.type !== "daily_rhythm_insights") {
    return {
      state: "unavailable",
      message: "The local service returned an unexpected response.",
      cards: [],
    };
  }

  const coverageText = formatCoverage(response.summary);
  const readiness = response.readiness.map(toReadinessItem);
  if (response.insights.length === 0) {
    const buildingBaseline = response.readiness.some(
      (item) => item.status === "insufficient_baseline",
    );
    return {
      state: "empty",
      generatedAt: response.generated_at,
      coverageText,
      emptyTitle: buildingBaseline
        ? "The canary is listening"
        : "No window change described",
      emptyBody: buildingBaseline
        ? "Your private log is already useful. Window comparisons begin after enough earlier days are available."
        : "Recorded windows were incomplete, within the current range, or too variable for a calm description.",
      baselineProgressText: buildingBaseline
        ? formatBaselineProgress(response.summary, response.readiness)
        : undefined,
      cards: [],
      readiness,
    };
  }

  return {
    state: "ready",
    generatedAt: response.generated_at,
    coverageText,
    isTruncated: response.summary.insights_truncated,
    cards: response.insights.map(toInsightCard),
    readiness,
  };
}

function formatBaselineProgress(
  summary: DailyRhythmSummary,
  readiness: RhythmDimensionReadiness[],
): string {
  const baselineDays = readiness.reduce(
    (maximum, item) => Math.max(maximum, item.minimum_day_count),
    0,
  );
  const target = baselineDays + 2;
  const logged = Math.min(summary.daily_snapshot_count, target);
  return `${logged} of ${target} days logged. Gaps don't reset this.`;
}

function toInsightCard(insight: RhythmInsight): InsightCardModel {
  return {
    localDate: insight.local_date,
    dimensionLabel: DIMENSION_LABELS[insight.dimension],
    changeLabel: DIRECTION_LABELS[insight.direction],
    summary: insight.summary.replace(
      insight.local_date,
      formatLocalDate(insight.local_date),
    ),
    evidence: insight.evidence.map(formatEvidence),
  };
}

function formatEvidence(evidence: RhythmEvidence): string {
  let value = evidence.value;
  if (evidence.label === "prior dates") {
    value = evidence.value.split(", ").map(formatLocalDate).join(", ");
  } else if (evidence.label === "current window") {
    const [dayCount, dates] = evidence.value.split(": ", 2);
    value =
      dates === undefined
        ? evidence.value
        : `${dayCount}: ${dates.split(", ").map(formatLocalDate).join(", ")}`;
  }
  return `${evidence.label}: ${value}`;
}

function formatLocalDate(value: string): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (match === null) {
    return value;
  }
  const month = MONTH_LABELS[Number(match[2]) - 1];
  if (month === undefined) {
    return value;
  }
  return `${Number(match[3])} ${month} ${match[1]}`;
}

const MONTH_LABELS = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
] as const;

function toReadinessItem(
  readiness: RhythmDimensionReadiness,
): ReadinessItemModel {
  return {
    dimensionLabel: DIMENSION_LABELS[readiness.dimension],
    statusLabel: READINESS_LABELS[readiness.status],
    detail: readinessDetail(readiness),
    state: readinessState(readiness.status),
  };
}

function readinessDetail(readiness: RhythmDimensionReadiness): string {
  switch (readiness.status) {
    case "change_described":
      return `Based on ${readiness.comparable_day_count} earlier days.`;
    case "within_baseline":
      return `${readiness.comparable_day_count} earlier days available; recent values stayed within your usual range.`;
    case "needs_sustained_change":
      return `${readiness.comparable_day_count} earlier days available; no clear shift seen yet.`;
    case "missing_current":
      return "No recent data for this signal.";
    case "insufficient_baseline":
      return `${readiness.comparable_day_count} of ${readiness.minimum_day_count} days needed before comparisons can start.`;
    case "zero_baseline":
      return "Earlier values were all near zero, so a relative comparison wasn't meaningful.";
    case "unstable_baseline":
      return "Earlier values varied too much for a useful comparison.";
  }
}

function readinessState(
  status: RhythmReadinessStatus,
): ReadinessItemModel["state"] {
  switch (status) {
    case "change_described":
      return "described";
    case "within_baseline":
      return "stable";
    case "needs_sustained_change":
    case "insufficient_baseline":
    case "zero_baseline":
    case "unstable_baseline":
      return "waiting";
    case "missing_current":
      return "missing";
  }
}

function formatCoverage(summary: DailyRhythmSummary): string {
  return [
    pluralize(summary.daily_snapshot_count, "daily snapshot"),
    pluralize(summary.browser_day_count, "browser day"),
    pluralize(summary.os_day_count, "OS day"),
    pluralize(summary.check_in_day_count, "check-in day"),
  ].join(", ");
}

function pluralize(count: number, label: string): string {
  return `${count} ${label}${count === 1 ? "" : "s"}`;
}
