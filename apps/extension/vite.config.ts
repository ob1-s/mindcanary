import { readFileSync } from "node:fs";

import { defineConfig, type Plugin } from "vite";

import {
  resolveExtensionIdentity,
  type ExtensionChannel,
} from "./extension-identity";
import { resolveFirefoxExtensionId } from "./firefox-identity";

type BrowserTarget = "chrome" | "firefox";

export default defineConfig(({ mode }) => {
  const { browser, channel } = buildTarget(mode);
  const chromeIdentity =
    browser === "chrome" ? resolveExtensionIdentity(channel) : undefined;
  const extensionId =
    chromeIdentity?.extensionId ?? resolveFirefoxExtensionId(channel);

  return {
    define: {
      __MINDCANARY_EXTENSION_CHANNEL__: JSON.stringify(channel),
      __MINDCANARY_EXPECTED_EXTENSION_ID__: JSON.stringify(extensionId),
      __MINDCANARY_BROWSER_TARGET__: JSON.stringify(browser),
    },
    plugins: [
      manifestPlugin(browser, extensionId, chromeIdentity?.manifestKey ?? null),
    ],
    build: {
      outDir: browser === "firefox" ? "dist-firefox" : "dist",
      emptyOutDir: true,
      rollupOptions: {
        input: {
          popup: "popup.html",
          "service-worker": "src/service-worker.ts",
          "scroll-observer": "src/scroll-observer.ts",
        },
        output: {
          entryFileNames: (chunk) =>
            chunk.name === "service-worker"
              ? "service-worker.js"
              : chunk.name === "scroll-observer"
                ? "scroll-observer.js"
                : "assets/[name].js",
        },
      },
      sourcemap: true,
    },
  };
});

function buildTarget(mode: string): {
  browser: BrowserTarget;
  channel: ExtensionChannel;
} {
  if (mode === "development" || mode === "release") {
    return { browser: "chrome", channel: mode };
  }
  if (mode === "firefox-development") {
    return { browser: "firefox", channel: "development" };
  }
  if (mode === "firefox-release") {
    return { browser: "firefox", channel: "release" };
  }
  if (mode === "test") {
    return { browser: "chrome", channel: "development" };
  }
  throw new Error(
    `Unsupported extension build mode "${mode}"; use development or release`,
  );
}

function manifestPlugin(
  browser: BrowserTarget,
  extensionId: string,
  manifestKey: string | null,
): Plugin {
  return {
    name: "mindcanary-extension-manifest",
    generateBundle() {
      const baseManifest = JSON.parse(
        readFileSync(new URL("./manifest.base.json", import.meta.url), "utf8"),
      ) as Record<string, unknown>;
      const manifest =
        browser === "firefox"
          ? firefoxManifest(baseManifest, extensionId)
          : manifestKey === null
            ? baseManifest
            : { ...baseManifest, key: manifestKey };

      this.emitFile({
        type: "asset",
        fileName: "manifest.json",
        source: `${JSON.stringify(manifest, null, 2)}\n`,
      });
    },
  };
}

function firefoxManifest(
  baseManifest: Record<string, unknown>,
  extensionId: string,
): Record<string, unknown> {
  const portable = { ...baseManifest };
  delete portable.minimum_chrome_version;
  return {
    ...portable,
    background: { scripts: ["service-worker.js"] },
    browser_specific_settings: {
      gecko: {
        id: extensionId,
        strict_min_version: "121.0",
      },
    },
  };
}
