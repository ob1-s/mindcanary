import { describe, expect, it } from "vitest";

import { resolveFirefoxExtensionId } from "./firefox-identity";

describe("Firefox extension identity", () => {
  it("uses a stable development ID and fails closed for release", () => {
    expect(resolveFirefoxExtensionId("development")).toBe(
      "development@mindcanary.local",
    );
    expect(() => resolveFirefoxExtensionId("release")).toThrow(
      /not configured/,
    );
  });
});
