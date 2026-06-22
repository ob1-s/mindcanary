import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

export type ExtensionChannel = "development" | "release";

interface ExtensionIdentityEntry {
  extension_id: string | null;
  manifest_key: string | null;
}

interface ExtensionIdentityRegistry {
  schema_version: number;
  development: ExtensionIdentityEntry;
  release: ExtensionIdentityEntry;
}

export interface ResolvedExtensionIdentity {
  channel: ExtensionChannel;
  extensionId: string;
  manifestKey: string | null;
}

const REGISTRY_PATH = fileURLToPath(
  new URL("../../config/chrome-extension-identities.json", import.meta.url),
);

export function resolveExtensionIdentity(
  channel: ExtensionChannel,
): ResolvedExtensionIdentity {
  const registry = readIdentityRegistry();
  const entry = registry[channel];

  if (entry.extension_id === null) {
    throw new Error(
      `Chrome ${channel} extension ID is not configured in config/chrome-extension-identities.json`,
    );
  }
  validateExtensionId(entry.extension_id, channel);

  if (channel === "development") {
    if (entry.manifest_key === null) {
      throw new Error("Chrome development identity requires a manifest key");
    }
    const derivedId = extensionIdFromManifestKey(entry.manifest_key);
    if (derivedId !== entry.extension_id) {
      throw new Error(
        `Chrome development manifest key derives ${derivedId}, not ${entry.extension_id}`,
      );
    }
  } else if (entry.manifest_key !== null) {
    throw new Error(
      "Chrome release identity must come from the Web Store and must not include a development manifest key",
    );
  }

  const otherEntry =
    channel === "development" ? registry.release : registry.development;
  if (otherEntry.extension_id === entry.extension_id) {
    throw new Error("Chrome development and release IDs must be different");
  }

  return {
    channel,
    extensionId: entry.extension_id,
    manifestKey: entry.manifest_key,
  };
}

export function extensionIdFromManifestKey(manifestKey: string): string {
  const publicKey = Buffer.from(manifestKey, "base64");
  if (publicKey.length === 0) {
    throw new Error("Chrome manifest key must contain a DER public key");
  }
  const digest = createHash("sha256")
    .update(publicKey)
    .digest("hex")
    .slice(0, 32);
  return [...digest]
    .map((nibble) =>
      String.fromCharCode("a".charCodeAt(0) + Number.parseInt(nibble, 16)),
    )
    .join("");
}

function readIdentityRegistry(): ExtensionIdentityRegistry {
  const registry = JSON.parse(
    readFileSync(REGISTRY_PATH, "utf8"),
  ) as ExtensionIdentityRegistry;
  if (registry.schema_version !== 1) {
    throw new Error(
      `Unsupported Chrome extension identity schema: ${registry.schema_version}`,
    );
  }
  return registry;
}

function validateExtensionId(
  extensionId: string,
  channel: ExtensionChannel,
): void {
  if (!/^[a-p]{32}$/.test(extensionId)) {
    throw new Error(
      `Chrome ${channel} extension ID must be 32 lowercase characters from a through p`,
    );
  }
}
