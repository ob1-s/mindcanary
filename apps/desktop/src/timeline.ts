import {
  DEFAULT_DAILY_TIMELINE_LIMIT,
  MAX_DAILY_TIMELINE_LIMIT,
  PROTOCOL_VERSION,
  type AnnotationRecord,
  type DailyBrowserTimeline,
  type DailyCheckInTimeline,
  type DailyOsTimeline,
  type DailyTimelineDay,
  type DailyTimelineSummary,
  type ProtocolRequest,
  type ProtocolResponse,
} from "@mindcanary/protocol";

import { CONTEXT_TAG_LABELS } from "./check-in";

export type DailyTimelineDashboardModel =
  | ReadyDailyTimelineDashboardModel
  | EmptyDailyTimelineDashboardModel
  | UnavailableDailyTimelineDashboardModel;

export interface ReadyDailyTimelineDashboardModel {
  state: "ready";
  generatedAt: string;
  coverageText: string;
  isTruncated: boolean;
  days: TimelineDayModel[];
}

export interface EmptyDailyTimelineDashboardModel {
  state: "empty";
  generatedAt: string;
  emptyTitle: string;
  emptyBody: string;
  days: [];
}

export interface UnavailableDailyTimelineDashboardModel {
  state: "unavailable";
  message: string;
  days: [];
}

export interface TimelineDayModel {
  localDate: string;
  dateLabel: string;
  coverageLabel: string;
  browser?: BrowserTimelineDayModel;
  os?: OsTimelineDayModel;
  checkIn?: CheckInTimelineDayModel;
  annotations: AnnotationTimelineModel[];
}

export type LatestLocalRecordModel =
  | {
      state: "ready";
      dateLabel: string;
      coverageLabel: string;
      entries: LatestLocalRecordEntry[];
    }
  | { state: "empty" }
  | { state: "unavailable" };

export interface LatestLocalRecordEntry {
  label: string;
  summary: string;
}

export interface AnnotationTimelineModel {
  annotationId: string;
  text: string;
  windowLabel: string;
  contextLabels: string[];
  record: AnnotationRecord;
}

export interface BrowserTimelineDayModel {
  openTabs: number | null;
  openTabsMaximum: number | null;
  tabSwitches: number | null;
  retainedAcrossDays: number | null;
  continuousScrollingMinutes: number | null;
  activeMinutes: number | null;
  recordedPeriodCount: number;
  summary: string;
}

export interface OsTimelineDayModel {
  activeMinutes: number | null;
  idleMinutes: number | null;
  lockCount: number | null;
  unlockCount: number | null;
  suspendCount: number | null;
  resumeCount: number | null;
  recordedPeriodCount: number;
  summary: string;
}

export interface CheckInTimelineDayModel {
  sleepMinutes: number | null;
  mood: number | null;
  energy: number | null;
  irritability: number | null;
  concentration: number | null;
  impulsivity: number | null;
  checkInCount: number;
  contextLabels: string[];
  summary: string;
}

export interface PriorCheckInReference {
  median: number;
  dayCount: number;
}

export type PriorCheckInReferences = Partial<
  Record<
    "mood" | "energy" | "irritability" | "concentration" | "impulsivity",
    PriorCheckInReference
  >
>;

export const BROWSER_TIMELINE_SOURCE =
  "Browser aggregate · 15-minute periods · raw periods retained for 90 days by default";
export const OS_TIMELINE_SOURCE =
  "OS aggregate · 15-minute periods · no app-specific content details";
export const CHECK_IN_TIMELINE_SOURCE =
  "Manual check-in · retained locally until you clear it";

export function toPriorCheckInReferences(
  model: DailyTimelineDashboardModel | undefined,
  beforeLocalDate: string,
): PriorCheckInReferences {
  if (model?.state !== "ready") return {};

  const references: PriorCheckInReferences = {};
  const fields = [
    "mood",
    "energy",
    "irritability",
    "concentration",
    "impulsivity",
  ] as const;
  for (const field of fields) {
    const values = model.days
      .filter((day) => day.localDate < beforeLocalDate)
      .map((day) => day.checkIn?.[field])
      .filter(
        (value): value is number => value != null && Number.isFinite(value),
      );
    const value = median(values);
    if (value !== null) {
      references[field] = { median: value, dayCount: values.length };
    }
  }
  return references;
}

export function createDailyTimelineRequest(
  limit = DEFAULT_DAILY_TIMELINE_LIMIT,
): ProtocolRequest {
  if (
    !Number.isInteger(limit) ||
    limit < 1 ||
    limit > MAX_DAILY_TIMELINE_LIMIT
  ) {
    throw new RangeError(
      `Daily timeline limit must be between 1 and ${MAX_DAILY_TIMELINE_LIMIT}.`,
    );
  }

  return {
    type: "get_daily_timeline",
    protocol_version: PROTOCOL_VERSION,
    limit,
  };
}

export function toDailyTimelineDashboardModel(
  response: ProtocolResponse,
): DailyTimelineDashboardModel {
  if (response.type === "error") {
    return {
      state: "unavailable",
      message:
        "Daily history is unavailable right now. Your local records remain on this device.",
      days: [],
    };
  }

  if (response.type !== "daily_timeline") {
    return {
      state: "unavailable",
      message: "The local service returned an unexpected response.",
      days: [],
    };
  }

  if (response.days.length === 0) {
    return {
      state: "empty",
      generatedAt: response.generated_at,
      emptyTitle: "No daily history yet",
      emptyBody:
        "A check-in, private annotation, or enabled local aggregate will start this view.",
      days: [],
    };
  }

  return {
    state: "ready",
    generatedAt: response.generated_at,
    coverageText: formatCoverage(response.summary),
    isTruncated: response.summary.days_truncated,
    days: response.days.map(toTimelineDay),
  };
}

export function toLatestLocalRecordModel(
  model: DailyTimelineDashboardModel | undefined,
): LatestLocalRecordModel {
  if (model === undefined || model.state === "empty") {
    return { state: "empty" };
  }
  if (model.state === "unavailable") {
    return { state: "unavailable" };
  }

  const day = [...model.days]
    .reverse()
    .find(
      (candidate) =>
        candidate.browser !== undefined ||
        candidate.os !== undefined ||
        candidate.checkIn !== undefined ||
        candidate.annotations.length > 0,
    );
  if (day === undefined) {
    return { state: "empty" };
  }

  const entries: LatestLocalRecordEntry[] = [];
  if (day.browser !== undefined) {
    entries.push({ label: "Browser", summary: day.browser.summary });
  }
  if (day.os !== undefined) {
    entries.push({ label: "Computer", summary: day.os.summary });
  }
  if (day.checkIn !== undefined) {
    entries.push({ label: "Check-in", summary: day.checkIn.summary });
  }
  if (day.annotations.length > 0) {
    entries.push({
      label: "Context",
      summary: pluralize(day.annotations.length, "private annotation"),
    });
  }

  return {
    state: "ready",
    dateLabel: day.dateLabel,
    coverageLabel: day.coverageLabel,
    entries,
  };
}

function toTimelineDay(day: DailyTimelineDay): TimelineDayModel {
  const browser = day.browser == null ? undefined : toBrowserDay(day.browser);
  const os = day.os == null ? undefined : toOsDay(day.os);
  const checkIn = day.check_in == null ? undefined : toCheckInDay(day.check_in);
  const annotations = day.annotations.map(toAnnotation);
  const coverageLabel = [
    browser === undefined ? undefined : "Browser",
    os === undefined ? undefined : "OS",
    checkIn === undefined ? undefined : "Check-in",
    annotations.length === 0 ? undefined : "Annotation",
  ]
    .filter((label): label is string => label !== undefined)
    .join(" + ");

  return {
    localDate: day.local_date,
    dateLabel: formatLocalDate(day.local_date),
    coverageLabel: coverageLabel.length === 0 ? "No record" : coverageLabel,
    browser,
    os,
    checkIn,
    annotations,
  };
}

function toAnnotation(record: AnnotationRecord): AnnotationTimelineModel {
  return {
    annotationId: record.annotation_id,
    text: record.text,
    windowLabel:
      record.start_minute == null || record.end_minute == null
        ? "Whole day"
        : `${formatMinute(record.start_minute)}-${formatMinute(record.end_minute)}`,
    contextLabels: record.context_tags.map((tag) => CONTEXT_TAG_LABELS[tag]),
    record,
  };
}

function toBrowserDay(day: DailyBrowserTimeline): BrowserTimelineDayModel {
  const openTabs = finiteOrNull(day.open_tab_count_mean);
  const openTabsMaximum = finiteOrNull(day.open_tab_count_max);
  const tabSwitches = finiteOrNull(day.tab_switch_count);
  const retainedAcrossDays = finiteOrNull(day.retained_across_day_count);
  const continuousScrollingMinutes =
    day.continuous_scrolling_seconds == null
      ? null
      : finiteOrNull(day.continuous_scrolling_seconds / 60);
  const activeMinutes =
    day.active_seconds == null ? null : finiteOrNull(day.active_seconds / 60);
  const parts = [
    openTabs === null
      ? undefined
      : `${formatNumber(openTabs)} average open tabs`,
    tabSwitches === null
      ? undefined
      : `${formatNumber(tabSwitches)} tab switches`,
    activeMinutes === null
      ? undefined
      : `${formatDuration(activeMinutes)} active`,
    continuousScrollingMinutes === null
      ? undefined
      : `${formatDuration(continuousScrollingMinutes)} continuous scrolling`,
  ].filter((part): part is string => part !== undefined);

  return {
    openTabs,
    openTabsMaximum,
    tabSwitches,
    retainedAcrossDays,
    continuousScrollingMinutes,
    activeMinutes,
    recordedPeriodCount: day.recorded_bucket_count,
    summary:
      parts.length === 0 ? "Browser aggregate recorded" : parts.join(" · "),
  };
}

function toOsDay(day: DailyOsTimeline): OsTimelineDayModel {
  const activeMinutes =
    day.active_seconds == null ? null : finiteOrNull(day.active_seconds / 60);
  const idleMinutes =
    day.idle_seconds == null ? null : finiteOrNull(day.idle_seconds / 60);
  const lockCount = finiteOrNull(day.lock_count);
  const unlockCount = finiteOrNull(day.unlock_count);
  const suspendCount = finiteOrNull(day.suspend_count);
  const resumeCount = finiteOrNull(day.resume_count);
  const parts = [
    activeMinutes === null
      ? undefined
      : `${formatDuration(activeMinutes)} computer active`,
    lockCount === null ? undefined : countLabel(lockCount, "lock"),
    suspendCount === null ? undefined : countLabel(suspendCount, "suspend"),
  ].filter((part): part is string => part !== undefined);

  return {
    activeMinutes,
    idleMinutes,
    lockCount,
    unlockCount,
    suspendCount,
    resumeCount,
    recordedPeriodCount: day.recorded_bucket_count,
    summary: parts.length === 0 ? "OS aggregate recorded" : parts.join(" · "),
  };
}

function toCheckInDay(day: DailyCheckInTimeline): CheckInTimelineDayModel {
  const sleepMinutes = finiteOrNull(day.sleep_minutes);
  const mood = finiteOrNull(day.mood);
  const energy = finiteOrNull(day.energy);
  const irritability = finiteOrNull(day.irritability);
  const concentration = finiteOrNull(day.concentration);
  const impulsivity = finiteOrNull(day.impulsivity);
  const parts = [
    day.check_in_count > 1
      ? `${day.check_in_count} check-ins summarized`
      : undefined,
    sleepMinutes === null ? undefined : `${formatDuration(sleepMinutes)} sleep`,
    energy === null ? undefined : `energy ${formatNumber(energy)}/7`,
    mood === null ? undefined : `mood ${formatNumber(mood)}/7`,
  ].filter((part): part is string => part !== undefined);

  return {
    sleepMinutes,
    mood,
    energy,
    irritability,
    concentration,
    impulsivity,
    checkInCount: day.check_in_count,
    contextLabels: day.context_tags.map((tag) => CONTEXT_TAG_LABELS[tag]),
    summary: parts.length === 0 ? "Check-in recorded" : parts.join(" · "),
  };
}

function formatCoverage(summary: DailyTimelineSummary): string {
  return [
    pluralize(summary.returned_day_count, "calendar day"),
    pluralize(summary.browser_day_count, "browser day"),
    pluralize(summary.os_day_count, "OS day"),
    pluralize(summary.check_in_day_count, "check-in day"),
    pluralize(summary.annotation_day_count, "annotated day"),
    pluralize(summary.missing_day_count, "explicit gap"),
  ].join(", ");
}

function formatMinute(value: number): string {
  const hour = Math.floor(value / 60);
  const minute = value % 60;
  return `${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
}

function formatLocalDate(localDate: string): string {
  return new Intl.DateTimeFormat("en", {
    weekday: "short",
    month: "short",
    day: "numeric",
    timeZone: "UTC",
  }).format(new Date(`${localDate}T12:00:00Z`));
}

function median(values: number[]): number | null {
  if (values.length === 0) return null;
  const sorted = [...values].sort((left, right) => left - right);
  const midpoint = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0
    ? ((sorted[midpoint - 1] ?? 0) + (sorted[midpoint] ?? 0)) / 2
    : (sorted[midpoint] ?? null);
}

function finiteOrNull(value: number | null | undefined): number | null {
  return value !== undefined && value !== null && Number.isFinite(value)
    ? value
    : null;
}

function formatDuration(minutes: number): string {
  if (minutes < 60) {
    return `${formatNumber(minutes)}m`;
  }
  const hours = minutes / 60;
  return `${formatNumber(hours)}h`;
}

function formatNumber(value: number): string {
  return Math.abs(value - Math.round(value)) < 0.05
    ? String(Math.round(value))
    : value.toFixed(1);
}

function pluralize(count: number, label: string): string {
  return `${count} ${label}${count === 1 ? "" : "s"}`;
}

function countLabel(count: number, label: string): string {
  return `${formatNumber(count)} ${label}${Math.abs(count - 1) < 0.05 ? "" : "s"}`;
}
