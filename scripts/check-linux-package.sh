#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ $# -gt 0 ]]; then
  deb="$1"
else
  shopt -s nullglob
  packages=("$root"/apps/desktop/src-tauri/target/release/bundle/deb/*.deb)
  if [[ "${#packages[@]}" -ne 1 ]]; then
    printf 'Expected exactly one Linux package, found %s. Pass the .deb path explicitly.\n' "${#packages[@]}" >&2
    exit 1
  fi
  deb="${packages[0]}"
fi

if [[ ! -f "$deb" ]]; then
  printf 'Linux package not found: %s\n' "$deb" >&2
  exit 1
fi

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

dpkg-deb -x "$deb" "$workdir/root"

desktop="$workdir/root/usr/bin/mindcanary-desktop"
daemon="$workdir/root/usr/lib/mindcanary/mindcanaryd"
native_host="$workdir/root/usr/lib/mindcanary/mindcanary-native-host"

for executable in "$desktop" "$daemon" "$native_host"; do
  [[ -x "$executable" ]] || {
    printf 'Expected executable is missing from package: %s\n' "$executable" >&2
    exit 1
  }
done

control="$(dpkg-deb -f "$deb")"
if grep -Fq 'Description: (none)' <<<"$control"; then
  printf 'Debian package description is missing.\n' >&2
  exit 1
fi

service_dir="$workdir/systemd"
"$daemon" \
  --install-user-service \
  --daemon-path /usr/lib/mindcanary/mindcanaryd \
  --service-dir "$service_dir"
grep -Fq '/usr/lib/mindcanary/mindcanaryd' \
  "$service_dir/mindcanaryd.service"

manifest_dir="$workdir/native-messaging"
"$native_host" \
  --install-manifest \
  --browser chrome \
  --channel development \
  --host-path /usr/lib/mindcanary/mindcanary-native-host \
  --manifest-dir "$manifest_dir"
"$native_host" \
  --check-manifest \
  --browser chrome \
  --channel development \
  --host-path /usr/lib/mindcanary/mindcanary-native-host \
  --manifest-dir "$manifest_dir" | grep -Fxq 'ready'
grep -Fq '"path": "/usr/lib/mindcanary/mindcanary-native-host"' \
  "$manifest_dir/app.mindcanary.collector.json"
grep -Fq 'chrome-extension://agokdhalkipifklmbipkgmfakdcaekbj/' \
  "$manifest_dir/app.mindcanary.collector.json"

if grep -R -E '/home/|/target/' "$service_dir" "$manifest_dir"; then
  printf 'Generated package integration contains a development path.\n' >&2
  exit 1
fi

printf 'Linux package check passed: %s\n' "$deb"
