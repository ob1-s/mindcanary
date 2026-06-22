#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

fail() {
  printf 'preflight failed: %s\n' "$1" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

printf 'MindCanary launch engineering preflight\n'

for command in git node pnpm rustc cargo pkg-config; do
  require_command "$command"
done

git rev-parse --is-inside-work-tree >/dev/null 2>&1 ||
  fail "workspace is not a valid Git repository"

node_major="$(node --version | sed -E 's/^v([0-9]+).*/\1/')"
[[ "$node_major" -ge 22 ]] || fail "Node 22 or later is required"

[[ "$(pnpm --version)" == "10.8.1" ]] ||
  fail "pnpm 10.8.1 is required by packageManager"

[[ "$(rustc --version)" == rustc\ 1.86.0* ]] ||
  fail "Rust 1.86.0 is required by rust-toolchain.toml"

for module in gtk+-3.0 webkit2gtk-4.1 openssl librsvg-2.0; do
  pkg-config --exists "$module" ||
    fail "missing native development package for pkg-config module: $module"
done

if command -v google-chrome >/dev/null 2>&1 ||
  command -v chromium >/dev/null 2>&1 ||
  command -v chromium-browser >/dev/null 2>&1; then
  printf 'Browser prerequisite: ready\n'
else
  fail "Chrome or Chromium is required for connector testing"
fi

for file in \
  Cargo.lock \
  pnpm-lock.yaml \
  apps/desktop/src-tauri/tauri.conf.json \
  apps/extension/manifest.base.json \
  config/chrome-extension-identities.json \
  docs/architecture.md; do
  [[ -f "$file" ]] || fail "missing required project file: $file"
done

printf 'Toolchain and native prerequisites: ready\n'
printf 'Running repository checks...\n'
bash scripts/check.sh

printf 'Checking the native Tauri shell...\n'
cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml

printf '\nEngineering preflight passed.\n'
printf 'Review docs/known-limitations.md before distributing a build.\n'
