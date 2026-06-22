#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary_dir="$root/apps/desktop/src-tauri/binaries"

cd "$root"

cargo build \
  --release \
  --locked \
  -p mindcanaryd \
  -p mindcanary-native-host

mkdir -p "$binary_dir"
install -m 0755 "$root/target/release/mindcanaryd" \
  "$binary_dir/mindcanaryd"
install -m 0755 "$root/target/release/mindcanary-native-host" \
  "$binary_dir/mindcanary-native-host"

printf 'Prepared MindCanary package helpers in %s\n' "$binary_dir"
