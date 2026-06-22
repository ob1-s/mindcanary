import { describe, expect, it } from "vitest";

import {
  createSubmitCheckInRequest,
  hasCheckInAnswers,
  toggleContextTag,
} from "./check-in";

describe("check-in request", () => {
  it("creates a private typed record from optional answers", () => {
    const request = createSubmitCheckInRequest(
      {
        sleepMinutes: 420,
        energy: 6,
        contextTags: ["deadline", "news_cycle"],
      },
      new Date("2026-06-14T02:30:00Z"),
      "America/Sao_Paulo",
    );

    expect(request.type).toBe("submit_check_in");
    if (request.type !== "submit_check_in") {
      throw new Error("unexpected request type");
    }
    expect(request.check_in.local_date).toBe("2026-06-13");
    expect(request.check_in.energy).toBe(6);
    expect(request.check_in.context_tags).toEqual(["deadline", "news_cycle"]);
    expect(JSON.stringify(request)).not.toMatch(
      /note|diagnosis|mania|psychosis/i,
    );
  });

  it("requires at least one answer and bounded scales", () => {
    expect(hasCheckInAnswers({ contextTags: [] })).toBe(false);
    expect(() => createSubmitCheckInRequest({ contextTags: [] })).toThrow(
      TypeError,
    );
    expect(() =>
      createSubmitCheckInRequest({ energy: 8, contextTags: [] }),
    ).toThrow(RangeError);
  });

  it("toggles context tags without duplicates", () => {
    expect(toggleContextTag(["exercise"], "deadline")).toEqual([
      "exercise",
      "deadline",
    ]);
    expect(toggleContextTag(["exercise"], "exercise")).toEqual([]);
  });
});
