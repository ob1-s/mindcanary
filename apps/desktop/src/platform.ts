import {
  PROTOCOL_VERSION,
  type DesktopEnvironment,
  type OperatingSystem,
  type PlatformCapabilities,
  type PlatformCapability,
  type PlatformCapabilityId,
  type PlatformCapabilityStatus,
  type ProtocolRequest,
  type ProtocolResponse,
  type SessionType,
} from "@mindcanary/protocol";

export type PlatformCapabilityModel =
  | ReadyPlatformCapabilityModel
  | UnavailablePlatformCapabilityModel;

export interface ReadyPlatformCapabilityModel {
  state: "ready";
  environmentText: string;
  coverageText: string;
  capabilities: PlatformCapabilityCardModel[];
}

export interface UnavailablePlatformCapabilityModel {
  state: "unavailable";
  message: string;
  capabilities: [];
}

export interface PlatformCapabilityCardModel {
  id: PlatformCapabilityId;
  label: string;
  statusLabel: string;
  status: PlatformCapabilityStatus;
  detail: string;
}

const CAPABILITY_LABELS: Record<PlatformCapabilityId, string> = {
  os_lock_and_session_events: "Lock and session events",
  os_active_idle_duration: "Computer active or idle time",
  foreground_application_category: "Foreground app categories",
};

const STATUS_LABELS: Record<PlatformCapabilityStatus, string> = {
  available: "Available",
  planned: "Planned",
  unavailable: "Unavailable",
};

const OS_LABELS: Record<OperatingSystem, string> = {
  linux: "Linux",
  macos: "macOS",
  windows: "Windows",
  other: "Other OS",
};

const DESKTOP_LABELS: Record<DesktopEnvironment, string> = {
  gnome: "GNOME",
  kde: "KDE",
  other: "Other desktop",
  unknown: "Unknown desktop",
};

const SESSION_LABELS: Record<SessionType, string> = {
  x11: "X11",
  wayland: "Wayland",
  other: "Other session",
  unknown: "Unknown session",
};

export function createPlatformCapabilitiesRequest(): ProtocolRequest {
  return {
    type: "get_platform_capabilities",
    protocol_version: PROTOCOL_VERSION,
  };
}

export function toPlatformCapabilityModel(
  response: ProtocolResponse,
): PlatformCapabilityModel {
  if (response.type === "error") {
    return {
      state: "unavailable",
      message:
        "Device support is unavailable right now. No OS activity is being collected.",
      capabilities: [],
    };
  }

  if (response.type !== "platform_capabilities") {
    return {
      state: "unavailable",
      message: "The local service returned an unexpected response.",
      capabilities: [],
    };
  }

  return {
    state: "ready",
    environmentText: formatEnvironment(response.capabilities),
    coverageText: formatCoverage(response.capabilities.capabilities),
    capabilities: response.capabilities.capabilities.map(toCapabilityCard),
  };
}

function toCapabilityCard(
  capability: PlatformCapability,
): PlatformCapabilityCardModel {
  return {
    id: capability.capability,
    label: CAPABILITY_LABELS[capability.capability],
    statusLabel: STATUS_LABELS[capability.status],
    status: capability.status,
    detail: capability.detail,
  };
}

function formatEnvironment(capabilities: PlatformCapabilities): string {
  return [
    OS_LABELS[capabilities.operating_system],
    DESKTOP_LABELS[capabilities.desktop_environment],
    SESSION_LABELS[capabilities.session_type],
  ].join(" · ");
}

function formatCoverage(capabilities: PlatformCapability[]): string {
  const available = capabilities.filter(
    (capability) => capability.status === "available",
  ).length;
  const planned = capabilities.filter(
    (capability) => capability.status === "planned",
  ).length;
  const unavailable = capabilities.filter(
    (capability) => capability.status === "unavailable",
  ).length;

  return [
    pluralize(available, "available signal"),
    pluralize(planned, "planned signal"),
    pluralize(unavailable, "unavailable signal"),
  ].join(", ");
}

function pluralize(count: number, label: string): string {
  return `${count} ${label}${count === 1 ? "" : "s"}`;
}
