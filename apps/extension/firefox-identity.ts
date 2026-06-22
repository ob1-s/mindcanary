import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import type { ExtensionChannel } from "./extension-identity";

interface FirefoxIdentityEntry {
  extension_id: string | null;
}

interface FirefoxIdentityRegistry {
  schema_version: number;
  development: FirefoxIdentityEntry;
  release: FirefoxIdentityEntry;
}

const REGISTRY_PATH = fileURLToPath(
  new URL("../../config/firefox-extension-identities.json", import.meta.url),
);

export function resolveFirefoxExtensionId(channel: ExtensionChannel): string {
  const registry = JSON.parse(
    readFileSync(REGISTRY_PATH, "utf8"),
  ) as FirefoxIdentityRegistry;
  if (registry.schema_version !== 1) {
    throw new Error(
      `Unsupported Firefox extension identity schema: ${registry.schema_version}`,
    );
  }
  const extensionId = registry[channel].extension_id;
  if (extensionId === null) {
    throw new Error(
      `Firefox ${channel} extension ID is not configured in config/firefox-extension-identities.json`,
    );
  }
  if (
    extensionId.trim().length === 0 ||
    extensionId.length > 255 ||
    /\s/.test(extensionId)
  ) {
    throw new Error("Firefox extension ID must not be empty or contain spaces");
  }
  if (registry.development.extension_id === registry.release.extension_id) {
    throw new Error("Firefox development and release IDs must be different");
  }
  return extensionId;
}
