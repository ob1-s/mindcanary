import { describe, expect, it } from "vitest";

import { PROTOCOL_VERSION, type ProtocolResponse } from "@mindcanary/protocol";

import {
  createPlatformCapabilitiesRequest,
  toPlatformCapabilityModel,
} from "./platform";

const BLOCKED_LANGUAGE =
  /mania|manic|depression|depressive|psychosis|diagnosis|warning|risk|alert/i;

describe("platform capability model", () => {
  it("builds the typed daemon request", () => {
    expect(createPlatformCapabilitiesRequest()).toEqual({
      type: "get_platform_capabilities",
      protocol_version: PROTOCOL_VERSION,
    });
  });

  it("renders OS capability status without implying collection", () => {
    const response: ProtocolResponse = {
      type: "platform_capabilities",
      protocol_version: PROTOCOL_VERSION,
      capabilities: {
        operating_system: "linux",
        desktop_environment: "gnome",
        session_type: "x11",
        capabilities: [
          {
            capability: "os_lock_and_session_events",
            status: "planned",
            detail:
              "GNOME/X11 environment detected; lock and session events still require a separate adapter.",
          },
          {
            capability: "os_active_idle_duration",
            status: "available",
            detail:
              "GNOME/X11 idle-time adapter is available; collection remains off until explicitly enabled.",
          },
          {
            capability: "foreground_application_category",
            status: "planned",
            detail:
              "GNOME/X11 environment detected; foreground categories still require a separate opt-in adapter.",
          },
        ],
      },
    };

    const model = toPlatformCapabilityModel(response);

    expect(model.state).toBe("ready");
    if (model.state !== "ready") {
      throw new Error("expected ready platform model");
    }
    expect(model.environmentText).toBe("Linux · GNOME · X11");
    expect(model.coverageText).toBe(
      "1 available signal, 2 planned signals, 0 unavailable signals",
    );
    expect(model.capabilities[0]?.statusLabel).toBe("Planned");
    expect(model.capabilities[1]?.statusLabel).toBe("Available");
    expect(JSON.stringify(model)).not.toMatch(BLOCKED_LANGUAGE);
  });

  it("uses a calm unavailable state", () => {
    const model = toPlatformCapabilityModel({
      type: "error",
      protocol_version: PROTOCOL_VERSION,
      code: "internal",
    });

    expect(model.state).toBe("unavailable");
    expect(JSON.stringify(model)).not.toMatch(BLOCKED_LANGUAGE);
  });
});
