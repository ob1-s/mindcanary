import { describe, expect, it } from "vitest";

import {
  extensionIdFromManifestKey,
  resolveExtensionIdentity,
} from "./extension-identity";

describe("Chrome extension identity", () => {
  it("pins development builds to the checked-in manifest key", () => {
    const identity = resolveExtensionIdentity("development");

    expect(identity.extensionId).toBe("agokdhalkipifklmbipkgmfakdcaekbj");
    expect(identity.manifestKey).not.toBeNull();
    expect(extensionIdFromManifestKey(identity.manifestKey!)).toBe(
      identity.extensionId,
    );
  });

  it("fails closed until a real Web Store release ID is configured", () => {
    expect(() => resolveExtensionIdentity("release")).toThrow(
      /release extension ID is not configured/,
    );
  });
});
