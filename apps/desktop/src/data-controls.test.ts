import { describe, expect, it } from "vitest";

import { PROTOCOL_VERSION } from "@mindcanary/protocol";

import {
  createSupportDiagnostics,
  createClearLocalRecordsRequest,
  createExportLocalRecordsRequest,
  createLocalDataSummaryRequest,
  createPrepareClearLocalRecordsRequest,
  createPrepareExportLocalRecordsRequest,
  toLocalDataControlModel,
} from "./data-controls";

describe("local data controls", () => {
  it("creates a preview-only support report without private record details", () => {
    const model = createSupportDiagnostics({
      appVersion: "0.1.2",
      serviceState: "ready",
      localDataAvailable: true,
      platform: {
        state: "ready",
        environmentText: "Linux · GNOME · X11",
        capabilities: [],
      },
      connections: {
        state: "ready",
        message: "Ready",
        items: [
          {
            id: "browser",
            label: "Chrome extension",
            statusLabel: "Connected",
            detail: "Private timing and /home/person/database.db",
            tone: "ready",
          },
        ],
      },
    });

    expect(model.reportText).toContain("App version: 0.1.2");
    expect(model.reportText).toContain("Chrome extension: Connected");
    expect(model.reportText).toContain("Linux · GNOME · X11");
    expect(model.reportText).not.toContain("Private timing");
    expect(model.reportText).not.toContain("/home/person");
  });

  it("builds typed summary and two-step clear requests", () => {
    expect(createLocalDataSummaryRequest()).toEqual({
      type: "get_local_data_summary",
      protocol_version: PROTOCOL_VERSION,
    });
    expect(createPrepareClearLocalRecordsRequest()).toEqual({
      type: "prepare_clear_local_records",
      protocol_version: PROTOCOL_VERSION,
    });
    expect(createPrepareExportLocalRecordsRequest()).toEqual({
      type: "prepare_export_local_records",
      protocol_version: PROTOCOL_VERSION,
    });
    expect(
      createExportLocalRecordsRequest(
        "01900000-0000-7000-8000-000000000001",
        "/tmp/mindcanary-export",
      ),
    ).toEqual({
      type: "export_local_records",
      protocol_version: PROTOCOL_VERSION,
      confirmation_token: "01900000-0000-7000-8000-000000000001",
      export_directory: "/tmp/mindcanary-export",
    });
    expect(
      createClearLocalRecordsRequest("01900000-0000-7000-8000-000000000001"),
    ).toEqual({
      type: "clear_local_records",
      protocol_version: PROTOCOL_VERSION,
      confirmation_token: "01900000-0000-7000-8000-000000000001",
    });
    expect(() => createClearLocalRecordsRequest("")).toThrow(TypeError);
    expect(() => createExportLocalRecordsRequest("", "/tmp/export")).toThrow(
      TypeError,
    );
    expect(() =>
      createExportLocalRecordsRequest(
        "01900000-0000-7000-8000-000000000001",
        "",
      ),
    ).toThrow(TypeError);
  });

  it("shows exactly which local records will be exported without clinical claims", () => {
    const model = toLocalDataControlModel({
      type: "export_local_records_confirmation",
      protocol_version: PROTOCOL_VERSION,
      confirmation_token: "01900000-0000-7000-8000-000000000001",
      expires_at: "2026-06-14T12:05:00Z",
      summary: {
        aggregate_batch_count: 20,
        aggregate_metric_count: 100,
        check_in_count: 5,
        context_tag_count: 7,
        annotation_count: 2,
        annotation_context_tag_count: 3,
      },
    });

    expect(model.title).toBe("Export local records");
    expect(model.confirmationText).toContain("your private notes");
    expect(model.confirmationText).toContain("No URLs");
    expect(model.confirmationText).toContain("titles");
    expect(model.confirmationText).not.toMatch(/predict|score|warning/i);
    expect(model).toHaveProperty("confirmationToken");
  });

  it("summarizes completed local exports as user-owned files", () => {
    const model = toLocalDataControlModel({
      type: "local_records_exported",
      protocol_version: PROTOCOL_VERSION,
      export: {
        export_directory: "/tmp/mindcanary-export",
        report_path: "/tmp/mindcanary-export/mindcanary-report.md",
        daily_browser_csv_path: "/tmp/mindcanary-export/daily-browser.csv",
        daily_os_csv_path: "/tmp/mindcanary-export/daily-os.csv",
        daily_check_in_csv_path: "/tmp/mindcanary-export/daily-check-ins.csv",
        annotations_csv_path: "/tmp/mindcanary-export/annotations.csv",
        summary: {
          aggregate_batch_count: 20,
          aggregate_metric_count: 100,
          check_in_count: 5,
          context_tag_count: 7,
          annotation_count: 2,
          annotation_context_tag_count: 3,
        },
      },
    });

    expect(model.title).toBe("Local export written");
    expect(model.confirmationText).toContain("written on this device");
    expect(model).toHaveProperty(
      "reportPath",
      "/tmp/mindcanary-export/mindcanary-report.md",
    );
    expect(model).toHaveProperty(
      "dailyOsCsvPath",
      "/tmp/mindcanary-export/daily-os.csv",
    );
  });

  it("shows exactly which local records will be cleared", () => {
    const model = toLocalDataControlModel({
      type: "clear_local_records_confirmation",
      protocol_version: PROTOCOL_VERSION,
      confirmation_token: "01900000-0000-7000-8000-000000000001",
      expires_at: "2026-06-14T12:05:00Z",
      summary: {
        aggregate_batch_count: 20,
        aggregate_metric_count: 100,
        check_in_count: 5,
        context_tag_count: 7,
        annotation_count: 2,
        annotation_context_tag_count: 3,
      },
    });

    expect(model.summaryText).toBe(
      "20 aggregate batches, 100 aggregate metrics, 5 check-ins, 7 context tags, 2 annotations, 3 annotation tags",
    );
    expect(model.confirmationText).toContain("does not uninstall mindcanary");
    expect(model).toHaveProperty("confirmationToken");
  });

  it("does not claim the database key was deleted", () => {
    const model = toLocalDataControlModel({
      type: "local_records_cleared",
      protocol_version: PROTOCOL_VERSION,
      deleted: {
        aggregate_batch_count: 20,
        aggregate_metric_count: 100,
        check_in_count: 5,
        context_tag_count: 7,
        annotation_count: 2,
        annotation_context_tag_count: 3,
      },
    });

    expect(model.title).toBe("Local records cleared");
    expect(model.confirmationText).toContain("app-owned records");
    expect(model.confirmationText).not.toMatch(
      /everything|all traces|permanently gone/i,
    );
  });
});
