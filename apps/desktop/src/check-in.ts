import {
  CONTEXT_TAGS,
  MAX_CHECK_IN_SCALE,
  MAX_CONTEXT_TAGS,
  MAX_SLEEP_MINUTES,
  MIN_CHECK_IN_SCALE,
  PROTOCOL_VERSION,
  type CheckInRecord,
  type ContextTag,
  type ProtocolRequest,
} from "@mindcanary/protocol";

export interface CheckInDraft {
  sleepMinutes?: number;
  perceivedSleepNeed?: number;
  mood?: number;
  energy?: number;
  irritability?: number;
  concentration?: number;
  impulsivity?: number;
  medicationTaken?: boolean;
  substanceUse?: boolean;
  contextTags: ContextTag[];
}

export const CONTEXT_TAG_LABELS: Record<ContextTag, string> = {
  deadline: "Deadline",
  travel: "Travel",
  illness: "Illness",
  news_cycle: "News cycle",
  job_uncertainty: "Job uncertainty",
  social_conflict: "Social conflict",
  exercise: "Exercise",
  medication_change: "Medication change",
  substance_use: "Substance use",
  unusual_good_event: "Unusual good event",
  other: "Other",
};

export const EMPTY_CHECK_IN_DRAFT: CheckInDraft = {
  contextTags: [],
};

export function createSubmitCheckInRequest(
  draft: CheckInDraft,
  occurredAt = new Date(),
  timeZone = resolvedTimeZone(),
): ProtocolRequest {
  validateDraft(draft);
  const record: CheckInRecord = {
    check_in_id: uuidV7(occurredAt.getTime()),
    occurred_at: occurredAt.toISOString(),
    time_zone: timeZone,
    local_date: localDate(occurredAt, timeZone),
    sleep_minutes: draft.sleepMinutes,
    perceived_sleep_need: draft.perceivedSleepNeed,
    mood: draft.mood,
    energy: draft.energy,
    irritability: draft.irritability,
    concentration: draft.concentration,
    impulsivity: draft.impulsivity,
    medication_taken: draft.medicationTaken,
    substance_use: draft.substanceUse,
    context_tags: [...draft.contextTags],
  };

  return {
    type: "submit_check_in",
    protocol_version: PROTOCOL_VERSION,
    check_in: record,
  };
}

export function hasCheckInAnswers(draft: CheckInDraft): boolean {
  return (
    draft.sleepMinutes !== undefined ||
    draft.perceivedSleepNeed !== undefined ||
    draft.mood !== undefined ||
    draft.energy !== undefined ||
    draft.irritability !== undefined ||
    draft.concentration !== undefined ||
    draft.impulsivity !== undefined ||
    draft.medicationTaken !== undefined ||
    draft.substanceUse !== undefined ||
    draft.contextTags.length > 0
  );
}

export function toggleContextTag(
  tags: ContextTag[],
  tag: ContextTag,
): ContextTag[] {
  if (tags.includes(tag)) {
    return tags.filter((existing) => existing !== tag);
  }
  if (tags.length >= MAX_CONTEXT_TAGS) {
    throw new RangeError(`Choose at most ${MAX_CONTEXT_TAGS} context tags.`);
  }
  return [...tags, tag];
}

export function contextTagOptions(): ContextTag[] {
  return [...CONTEXT_TAGS];
}

function validateDraft(draft: CheckInDraft): void {
  if (!hasCheckInAnswers(draft)) {
    throw new TypeError("Answer at least one field or choose a context tag.");
  }
  if (
    draft.sleepMinutes !== undefined &&
    (!Number.isInteger(draft.sleepMinutes) ||
      draft.sleepMinutes < 0 ||
      draft.sleepMinutes > MAX_SLEEP_MINUTES)
  ) {
    throw new RangeError(
      `Sleep must be between 0 and ${MAX_SLEEP_MINUTES} minutes.`,
    );
  }

  for (const value of [
    draft.perceivedSleepNeed,
    draft.mood,
    draft.energy,
    draft.irritability,
    draft.concentration,
    draft.impulsivity,
  ]) {
    if (
      value !== undefined &&
      (!Number.isInteger(value) ||
        value < MIN_CHECK_IN_SCALE ||
        value > MAX_CHECK_IN_SCALE)
    ) {
      throw new RangeError(
        `Check-in scales run from ${MIN_CHECK_IN_SCALE} to ${MAX_CHECK_IN_SCALE}.`,
      );
    }
  }

  if (new Set(draft.contextTags).size !== draft.contextTags.length) {
    throw new TypeError("Context tags must be unique.");
  }
  if (draft.contextTags.length > MAX_CONTEXT_TAGS) {
    throw new RangeError(`Choose at most ${MAX_CONTEXT_TAGS} context tags.`);
  }
}

function resolvedTimeZone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
}

function localDate(date: Date, timeZone: string): string {
  const parts = new Intl.DateTimeFormat("en-CA", {
    timeZone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).formatToParts(date);
  const byType = new Map(parts.map((part) => [part.type, part.value]));
  return `${byType.get("year")}-${byType.get("month")}-${byType.get("day")}`;
}

function uuidV7(nowMs: number): string {
  if (!Number.isSafeInteger(nowMs) || nowMs < 0) {
    throw new RangeError("A check-in timestamp must be a valid date.");
  }

  const bytes = crypto.getRandomValues(new Uint8Array(16));
  let timestamp = BigInt(nowMs);
  for (let index = 5; index >= 0; index -= 1) {
    bytes[index] = Number(timestamp & 0xffn);
    timestamp >>= 8n;
  }
  bytes[6] = (bytes[6] & 0x0f) | 0x70;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;

  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0"));
  return [
    hex.slice(0, 4).join(""),
    hex.slice(4, 6).join(""),
    hex.slice(6, 8).join(""),
    hex.slice(8, 10).join(""),
    hex.slice(10).join(""),
  ].join("-");
}
