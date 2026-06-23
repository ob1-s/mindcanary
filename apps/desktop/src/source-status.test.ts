import { describe, expect, it } from "vitest";

import { PROTOCOL_VERSION, type ProtocolResponse } from "@mindcanary/protocol";

import { daemonConnectionItem, toConnectionStatusModel } from "./source-status";

const response: ProtocolResponse = {
  type: "source_status",
  protocol_version: PROTOCOL_VERSION,
  generated_at: "2026-06-15T12:10:00Z",
  sources: [
    {
      source: "browser",
      health: "active",
      last_received_at: "2026-06-15T12:08:00Z",
    },
    {
      source: "os",
      health: "stale",
      last_received_at: "2026-06-15T10:10:00Z",
    },
    {
      source: "check_in",
      health: "active",
      last_received_at: null,
    },
  ],
};

describe("connection status model", () => {
  it("shows connected browser data with a relative receipt time", () => {
    const model = toConnectionStatusModel(response);
    const browser = model.items.find((item) => item.id === "browser");

    expect(browser).toMatchObject({
      label: "Chrome extension",
      statusLabel: "Connected",
      detail:
        "Last data received 2 minutes ago. New local buckets usually arrive every 15 minutes while the source is active.",
      tone: "positive",
    });
  });

  it("keeps stale and ready states descriptive", () => {
    const model = toConnectionStatusModel(response);

    expect(model.items.find((item) => item.id === "os")).toMatchObject({
      statusLabel: "No recent sample",
      tone: "attention",
    });
    expect(model.items.find((item) => item.id === "check_in")).toMatchObject({
      statusLabel: "Ready",
      detail: "Available whenever you choose to add one.",
      tone: "positive",
    });
    expect(JSON.stringify(model)).not.toMatch(/warning|diagnosis|mania/i);
  });

  it("reports the local daemon independently", () => {
    expect(daemonConnectionItem(true)).toMatchObject({
      label: "Local service",
      statusLabel: "Running",
      tone: "positive",
    });
  });

  it("exposes the connect chrome action when packaged manifest is missing", () => {
    const model = toConnectionStatusModel(response, {
      runtime: "packaged",
      health: "missing",
      setup_command: null,
    });
    const browser = model.items.find((item) => item.id === "browser");

    expect(browser).toMatchObject({
      statusLabel: "Not connected",
      detail: "Chrome can be connected to send optional local aggregates.",
      tone: "neutral",
      action: { type: "connect_chrome" },
    });
  });

  it("exposes the connect chrome action when packaged manifest needs repair", () => {
    const model = toConnectionStatusModel(response, {
      runtime: "packaged",
      health: "needs_repair",
      setup_command: null,
    });
    const browser = model.items.find((item) => item.id === "browser");

    expect(browser).toMatchObject({
      statusLabel: "Not connected",
      detail: "Chrome can be connected to send optional local aggregates.",
      tone: "neutral",
      action: { type: "connect_chrome" },
    });
  });

  it("exposes the development setup command when development manifest is missing", () => {
    const model = toConnectionStatusModel(response, {
      runtime: "development",
      health: "missing",
      setup_command: "cargo run --install-manifest",
    });
    const browser = model.items.find((item) => item.id === "browser");

    expect(browser).toMatchObject({
      statusLabel: "Not connected",
      detail:
        "Optional browser aggregates are not connected for this development profile.",
      tone: "neutral",
      action: {
        type: "setup_command",
        command: "cargo run --install-manifest",
      },
    });
  });

  it("distinguishes a ready bridge from an extension that has not delivered yet", () => {
    const model = toConnectionStatusModel(
      {
        ...response,
        sources: [
          {
            source: "browser",
            health: "never_seen",
            last_received_at: null,
          },
        ],
      },
      {
        runtime: "packaged",
        health: "ready",
        setup_command: null,
      },
    );

    expect(model.items[0]).toMatchObject({
      statusLabel: "Extension not seen yet",
      tone: "attention",
    });
    expect(model.items[0]?.detail).toContain("local bridge");
    expect(model.items[0]?.detail).toContain("no browser aggregate");
  });

  it("describes stale browser data as a quiet extension or bucket delay", () => {
    const model = toConnectionStatusModel(
      {
        ...response,
        sources: [
          {
            source: "browser",
            health: "stale",
            last_received_at: "2026-06-15T11:00:00Z",
          },
        ],
      },
      {
        runtime: "packaged",
        health: "ready",
        setup_command: null,
      },
    );

    expect(model.items[0]).toMatchObject({
      statusLabel: "No recent browser data",
      detail:
        "Last browser aggregate arrived 1 hour ago. Chrome may be closed, disabled, or waiting for its next 15-minute bucket.",
      tone: "attention",
    });
  });
});
