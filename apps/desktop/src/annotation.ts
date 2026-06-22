import {
  MAX_ANNOTATION_TEXT_BYTES,
  PROTOCOL_VERSION,
  type AnnotationRecord,
  type ContextTag,
  type ProtocolRequest,
  type ProtocolResponse,
} from "@mindcanary/protocol";

export interface AnnotationDraft {
  annotationId?: string;
  localDate: string;
  startTime: string;
  endTime: string;
  text: string;
  contextTags: ContextTag[];
}

export interface AnnotationDeletionConfirmationModel {
  annotationId: string;
  confirmationToken: string;
  expiresAt: string;
}

export type AnnotationDraftTextField =
  | "localDate"
  | "startTime"
  | "endTime"
  | "text";

export function emptyAnnotationDraft(
  localDate = todayLocalDate(),
): AnnotationDraft {
  return {
    localDate,
    startTime: "",
    endTime: "",
    text: "",
    contextTags: [],
  };
}

export function annotationDraft(record: AnnotationRecord): AnnotationDraft {
  return {
    annotationId: record.annotation_id,
    localDate: record.local_date,
    startTime: formatMinute(record.start_minute),
    endTime: formatMinute(record.end_minute),
    text: record.text,
    contextTags: [...record.context_tags],
  };
}

export function updateAnnotationDraftTextField(
  draft: AnnotationDraft,
  field: AnnotationDraftTextField,
  value: string,
): AnnotationDraft {
  return { ...draft, [field]: value };
}

export function createSaveAnnotationRequest(
  draft: AnnotationDraft,
): ProtocolRequest {
  const text = draft.text.trim();
  if (
    text.length === 0 ||
    new TextEncoder().encode(text).length > MAX_ANNOTATION_TEXT_BYTES
  ) {
    throw new RangeError("Note is too long — keep it under 1000 bytes.");
  }
  if (!/^\d{4}-\d{2}-\d{2}$/.test(draft.localDate)) {
    throw new TypeError("Choose a valid local date.");
  }
  const startMinute = parseMinute(draft.startTime);
  const endMinute = parseMinute(draft.endTime);
  if ((startMinute === undefined) !== (endMinute === undefined)) {
    throw new TypeError(
      "Choose both a start and end time, or leave both blank.",
    );
  }
  if (
    startMinute !== undefined &&
    endMinute !== undefined &&
    startMinute >= endMinute
  ) {
    throw new RangeError("The end time must be later than the start time.");
  }

  return {
    type: "save_annotation",
    protocol_version: PROTOCOL_VERSION,
    annotation: {
      annotation_id: draft.annotationId ?? crypto.randomUUID(),
      created_at: new Date().toISOString(),
      time_zone: Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC",
      local_date: draft.localDate,
      start_minute: startMinute,
      end_minute: endMinute,
      text,
      context_tags: [...draft.contextTags],
    },
  };
}

export function toAnnotationDeletionConfirmation(
  response: ProtocolResponse,
): AnnotationDeletionConfirmationModel {
  if (response.type !== "delete_annotation_confirmation") {
    throw new TypeError("Unexpected annotation deletion confirmation.");
  }
  return {
    annotationId: response.annotation_id,
    confirmationToken: response.confirmation_token,
    expiresAt: response.expires_at,
  };
}

function parseMinute(value: string): number | undefined {
  if (value === "") {
    return undefined;
  }
  const match = /^(\d{2}):(\d{2})$/.exec(value);
  if (match === null) {
    throw new TypeError("Use a valid local time.");
  }
  const hour = Number(match[1]);
  const minute = Number(match[2]);
  if (hour > 23 || minute > 59) {
    throw new RangeError("Use a valid local time.");
  }
  return hour * 60 + minute;
}

function formatMinute(value?: number | null): string {
  if (value === undefined || value === null) {
    return "";
  }
  const hour = Math.floor(value / 60);
  const minute = value % 60;
  return `${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
}

function todayLocalDate(): string {
  const now = new Date();
  const year = now.getFullYear();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}
