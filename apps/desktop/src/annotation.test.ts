import { describe, expect, it, vi } from "vitest";

import {
  annotationDraft,
  createSaveAnnotationRequest,
  emptyAnnotationDraft,
  toAnnotationDeletionConfirmation,
  updateAnnotationDraftTextField,
} from "./annotation";

describe("user annotations", () => {
  it("updates the selected local day without retaining an input event", () => {
    const draft = emptyAnnotationDraft("2026-06-14");

    expect(
      updateAnnotationDraftTextField(draft, "localDate", "2026-06-15"),
    ).toMatchObject({
      localDate: "2026-06-15",
      startTime: "",
      endTime: "",
    });
  });

  it("creates day annotations without inventing a time window", () => {
    vi.stubGlobal("crypto", { randomUUID: () => "annotation-id" });
    const request = createSaveAnnotationRequest({
      ...emptyAnnotationDraft("2026-06-19"),
      text: "Power outage changed the afternoon",
      contextTags: ["other"],
    });

    expect(request).toMatchObject({
      type: "save_annotation",
      annotation: {
        annotation_id: "annotation-id",
        local_date: "2026-06-19",
        start_minute: undefined,
        end_minute: undefined,
        text: "Power outage changed the afternoon",
        context_tags: ["other"],
      },
    });
    vi.unstubAllGlobals();
  });

  it("converts same-day time windows to minutes and back", () => {
    vi.stubGlobal("crypto", { randomUUID: () => "annotation-id" });
    const request = createSaveAnnotationRequest({
      ...emptyAnnotationDraft("2026-06-19"),
      startTime: "13:15",
      endTime: "14:45",
      text: "Afternoon nap",
      contextTags: [],
    });
    if (request.type !== "save_annotation") {
      throw new TypeError("expected annotation request");
    }
    expect(request.annotation.start_minute).toBe(795);
    expect(request.annotation.end_minute).toBe(885);
    expect(annotationDraft(request.annotation)).toMatchObject({
      startTime: "13:15",
      endTime: "14:45",
    });
    vi.unstubAllGlobals();
  });

  it("rejects partial or reversed windows", () => {
    expect(() =>
      createSaveAnnotationRequest({
        ...emptyAnnotationDraft("2026-06-19"),
        startTime: "13:00",
        text: "Partial window",
      }),
    ).toThrow(/both a start and end/i);
    expect(() =>
      createSaveAnnotationRequest({
        ...emptyAnnotationDraft("2026-06-19"),
        startTime: "14:00",
        endTime: "13:00",
        text: "Reversed window",
      }),
    ).toThrow(/later than/i);
  });

  it("maps daemon-bound deletion confirmation", () => {
    expect(
      toAnnotationDeletionConfirmation({
        type: "delete_annotation_confirmation",
        protocol_version: 1,
        confirmation_token: "token",
        expires_at: "2026-06-19T12:05:00Z",
        annotation_id: "annotation-id",
      }),
    ).toEqual({
      annotationId: "annotation-id",
      confirmationToken: "token",
      expiresAt: "2026-06-19T12:05:00Z",
    });
  });
});
