import type {
  ProtocolResponse,
  SourceHealth,
  SourceStatus,
  SourceType,
} from "@mindcanary/protocol";

import type { ChromeConnectorStatus } from "./daemon-api";

export interface ConnectionAction {
  type: "connect_chrome" | "setup_command";
  command?: string;
}

export interface ConnectionStatusItemModel {
  id: "daemon" | SourceType;
  label: string;
  statusLabel: string;
  detail: string;
  tone: "positive" | "neutral" | "attention";
  action?: ConnectionAction;
}

export interface ConnectionStatusModel {
  state: "ready" | "unavailable";
  items: ConnectionStatusItemModel[];
  message?: string;
}

export function toConnectionStatusModel(
  response: ProtocolResponse,
  connector?: ChromeConnectorStatus,
): ConnectionStatusModel {
  if (response.type !== "source_status") {
    return {
      state: "unavailable",
      items: [],
      message: "Connection status is unavailable from the local service.",
    };
  }

  const generatedAt = new Date(response.generated_at);
  return {
    state: "ready",
    items: response.sources.map((status) =>
      toConnectionItem(status, generatedAt, connector),
    ),
  };
}

export function daemonConnectionItem(
  ready: boolean,
): ConnectionStatusItemModel {
  return {
    id: "daemon",
    label: "Local service",
    statusLabel: ready ? "Running" : "Unavailable",
    detail: ready
      ? "Your records and local APIs are available."
      : "The app cannot reach the local service.",
    tone: ready ? "positive" : "attention",
  };
}

function toConnectionItem(
  status: SourceStatus,
  generatedAt: Date,
  connector?: ChromeConnectorStatus,
): ConnectionStatusItemModel {
  const labels: Record<SourceType, string> = {
    browser: "Chrome extension",
    os: "Computer activity",
    check_in: "Check-ins",
  };
  const statusLabels: Record<SourceType, Record<SourceHealth, string>> = {
    browser: {
      never_seen: "Waiting for data",
      active: "Connected",
      stale: "Not recently connected",
      disabled: "Disabled",
      unavailable: "Unavailable",
    },
    os: {
      never_seen: "Waiting for data",
      active: "Active",
      stale: "No recent sample",
      disabled: "Disabled",
      unavailable: "Unavailable",
    },
    check_in: {
      never_seen: "Ready",
      active: "Ready",
      stale: "Ready",
      disabled: "Disabled",
      unavailable: "Unavailable",
    },
  };

  let detail = sourceDetail(status, generatedAt);
  let statusLabel = statusLabels[status.source][status.health];
  let tone = sourceTone(status.health);
  let action: ConnectionAction | undefined;

  if (status.source === "browser" && connector) {
    if (connector.health === "helper_missing") {
      statusLabel = "Unavailable";
      detail =
        connector.runtime === "development"
          ? "Optional browser aggregates need the development native-host helper to be built."
          : "The local bridge helper is missing.";
      tone = connector.runtime === "development" ? "neutral" : "attention";
      if (connector.runtime === "development" && connector.setup_command) {
        action = { type: "setup_command", command: connector.setup_command };
      }
    } else if (
      connector.health === "missing" ||
      connector.health === "needs_repair"
    ) {
      statusLabel = "Not connected";
      detail =
        connector.runtime === "development"
          ? "Optional browser aggregates are not connected for this development profile."
          : "Chrome can be connected to send optional local aggregates.";
      tone = "neutral";
      if (connector.runtime === "packaged") {
        action = { type: "connect_chrome" };
      } else if (connector.setup_command) {
        action = { type: "setup_command", command: connector.setup_command };
      }
    } else if (connector.health === "ready") {
      if (status.health === "never_seen") {
        statusLabel = "Extension not seen yet";
        detail =
          "Chrome can reach the local bridge, but no browser aggregate has arrived yet. Buckets usually arrive on a 15-minute cadence while Chrome is active.";
        tone = "attention";
      } else if (status.health === "stale") {
        statusLabel = "No recent browser data";
        detail = browserStaleDetail(status, generatedAt);
        tone = "attention";
      }
    }
  }

  return {
    id: status.source,
    label: labels[status.source],
    statusLabel,
    detail,
    tone,
    action,
  };
}

function browserStaleDetail(status: SourceStatus, generatedAt: Date): string {
  if (
    status.last_received_at !== null &&
    status.last_received_at !== undefined
  ) {
    const receivedAt = new Date(status.last_received_at);
    const ageMs = Math.max(0, generatedAt.getTime() - receivedAt.getTime());
    return `Last browser aggregate arrived ${formatAge(ageMs)}. Chrome may be closed, disabled, or waiting for its next 15-minute bucket.`;
  }

  return "Chrome can reach the local bridge, but no recent browser aggregate has arrived. Buckets usually arrive on a 15-minute cadence while Chrome is active.";
}

function sourceDetail(status: SourceStatus, generatedAt: Date): string {
  if (
    status.last_received_at !== null &&
    status.last_received_at !== undefined
  ) {
    const receivedAt = new Date(status.last_received_at);
    const ageMs = Math.max(0, generatedAt.getTime() - receivedAt.getTime());
    const cadence =
      status.source === "check_in"
        ? ""
        : " New local buckets usually arrive every 15 minutes while the source is active.";
    return `Last data received ${formatAge(ageMs)}.${cadence}`;
  }

  switch (status.health) {
    case "disabled":
      return "Collection is off on this device.";
    case "unavailable":
      return "This source is not available in the current environment.";
    case "active":
      return status.source === "check_in"
        ? "Available whenever you choose to add one."
        : "Connected; waiting for the first local aggregate. Buckets usually arrive every 15 minutes while the source is active.";
    case "never_seen":
    case "stale":
      return "No local data has been received yet.";
  }
}

function sourceTone(health: SourceHealth): ConnectionStatusItemModel["tone"] {
  switch (health) {
    case "active":
      return "positive";
    case "stale":
      return "attention";
    case "never_seen":
    case "disabled":
    case "unavailable":
      return "neutral";
  }
}

function formatAge(ageMs: number): string {
  const minutes = Math.floor(ageMs / 60_000);
  if (minutes < 1) {
    return "just now";
  }
  if (minutes < 60) {
    return `${minutes} ${minutes === 1 ? "minute" : "minutes"} ago`;
  }
  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    return `${hours} ${hours === 1 ? "hour" : "hours"} ago`;
  }
  const days = Math.floor(hours / 24);
  return `${days} ${days === 1 ? "day" : "days"} ago`;
}
