import { describe, expect, it } from "vitest";

import {
  LOCAL_REMOVAL_CONFIRMATION_PHRASE,
  localRemovalModel,
  localRemovalResultText,
} from "./local-removal";

describe("local removal model", () => {
  it("keeps complete removal scoped to app-owned local data", () => {
    const model = localRemovalModel();

    expect(model.confirmationPhrase).toBe(LOCAL_REMOVAL_CONFIRMATION_PHRASE);
    expect(model.summaryText).toContain("encrypted database profile");
    expect(model.excludedText).toContain("Chrome extension storage");
    expect(model.excludedText).toContain("exports or backups");
    expect(JSON.stringify(model)).not.toMatch(
      /all traces|everything|clinical|diagnosis|warning/i,
    );
  });

  it("never claims browser extension storage or exports were removed", () => {
    const text = localRemovalResultText({
      user_service_removed: true,
      native_host_manifests_removed: ["chrome", "chromium"],
      database_profile_destroyed: true,
      package_marker_removed: false,
      runtime_socket_dir_removed: true,
      browser_extension_storage_removed: false,
      user_exports_removed: false,
    });

    expect(text).toContain(
      "Chrome extension storage and user exports were not removed",
    );
    expect(text).not.toMatch(/all traces|everything/i);
  });
});
