import { describe, expect, it } from "vitest";

import { PROTOCOL_VERSION } from "@mindcanary/protocol";

import {
  createLocalBackupRequest,
  createRestoreLocalBackupRequest,
  createVerifyLocalBackupRequest,
  toBackupConfirmationModel,
  toCreatedBackupModel,
  toRestoredBackupModel,
  toVerifiedBackupModel,
} from "./backup";

const summary = {
  aggregate_batch_count: 10,
  aggregate_metric_count: 20,
  check_in_count: 2,
  context_tag_count: 1,
  annotation_count: 3,
  annotation_context_tag_count: 2,
};

describe("encrypted local backup models", () => {
  it("builds strict create, verify, and restore requests", () => {
    expect(createLocalBackupRequest("token", " /tmp/a.mcbak ")).toEqual({
      type: "create_local_backup",
      protocol_version: PROTOCOL_VERSION,
      confirmation_token: "token",
      backup_path: "/tmp/a.mcbak",
    });
    expect(createVerifyLocalBackupRequest("/tmp/a.mcbak", "secret")).toEqual({
      type: "verify_local_backup",
      protocol_version: PROTOCOL_VERSION,
      backup_path: "/tmp/a.mcbak",
      recovery_secret: "secret",
    });
    expect(createRestoreLocalBackupRequest("/tmp/a.mcbak", "secret")).toEqual({
      type: "restore_local_backup",
      protocol_version: PROTOCOL_VERSION,
      backup_path: "/tmp/a.mcbak",
      recovery_secret: "secret",
    });
    expect(() => createLocalBackupRequest("", "/tmp/a")).toThrow();
  });

  it("maps confirmation, created, verified, and restored responses", () => {
    expect(
      toBackupConfirmationModel({
        type: "create_local_backup_confirmation",
        protocol_version: PROTOCOL_VERSION,
        confirmation_token: "token",
        expires_at: "2026-06-19T07:05:00Z",
        summary,
      }),
    ).toMatchObject({
      confirmationToken: "token",
      isEmpty: false,
      summaryText: "10 aggregate batches, 2 check-ins, 3 annotations",
    });

    const backup = {
      backup_path: "/tmp/a.mcbak",
      created_at: "2026-06-19T07:00:00Z",
      format_version: 1,
      schema_version: 4,
    };
    expect(
      toCreatedBackupModel({
        type: "local_backup_created",
        protocol_version: PROTOCOL_VERSION,
        backup: { ...backup, recovery_secret: "secret", summary },
      }),
    ).toMatchObject({ backupPath: "/tmp/a.mcbak", recoverySecret: "secret" });
    expect(
      toVerifiedBackupModel({
        type: "local_backup_verified",
        protocol_version: PROTOCOL_VERSION,
        backup,
      }),
    ).toMatchObject({ backupPath: "/tmp/a.mcbak", formatVersion: 1 });
    expect(
      toRestoredBackupModel({
        type: "local_backup_restored",
        protocol_version: PROTOCOL_VERSION,
        backup,
        restored: summary,
      }),
    ).toMatchObject({
      summaryText: "10 aggregate batches, 2 check-ins, 3 annotations",
    });
  });
});
