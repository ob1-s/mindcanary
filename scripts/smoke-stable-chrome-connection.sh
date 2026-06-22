#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

action="${1:-baseline}"
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
report_dir="${MINDCANARY_SMOKE_DIR:-$root/target/stable-chrome-smoke}"
report="$report_dir/$timestamp-$action.txt"

mkdir -p "$report_dir"

ctl() {
  if [[ -x "$root/target/debug/mindcanaryctl" ]]; then
    "$root/target/debug/mindcanaryctl" "$@"
  else
    cargo run -q -p mindcanary-client --bin mindcanaryctl -- "$@"
  fi
}

native_host() {
  if [[ -x "$root/target/debug/mindcanary-native-host" ]]; then
    "$root/target/debug/mindcanary-native-host" "$@"
  else
    cargo run -q -p mindcanary-native-host -- "$@"
  fi
}

section() {
  printf '\n## %s\n' "$1"
}

capture() {
  {
    printf '# MindCanary Stable Chrome Connection Smoke\n'
    printf 'Action: %s\n' "$action"
    printf 'Captured at: %s\n' "$(date -Is)"
    printf 'Repository: %s\n' "$root"
    printf 'Git branch: %s\n' "$(git branch --show-current 2>/dev/null || printf unknown)"
    printf 'Git commit: %s\n' "$(git rev-parse --short HEAD 2>/dev/null || printf unknown)"

    section "Daemon Health"
    ctl health

    section "Source Status"
    ctl source-status

    section "Local Data Summary"
    ctl summary

    section "Native Host Manifest"
    native_host \
      --check-manifest \
      --browser chrome \
      --channel development \
      --host-path "$root/target/debug/mindcanary-native-host"

    section "Systemd User Service"
    systemctl --user is-active mindcanaryd.service 2>/dev/null || true
    systemctl --user show mindcanaryd.service \
      --property=ActiveState,SubState,ExecMainPID,NRestarts \
      --no-pager 2>/dev/null || true
  } | tee "$report"
}

case "$action" in
  baseline)
    capture
    ;;
  after-manual-step)
    capture
    ;;
  restart-daemon)
    systemctl --user restart mindcanaryd.service
    sleep 2
    capture
    ;;
  *)
    cat >&2 <<'EOF'
Usage: scripts/smoke-stable-chrome-connection.sh [baseline|after-manual-step|restart-daemon]

baseline:
  Capture current daemon, source, manifest, and local-count evidence.

after-manual-step:
  Capture the same evidence after you manually reload, disable, remove, or
  re-enable the Chrome extension.

restart-daemon:
  Restart the user-level mindcanaryd service, then capture evidence.

Reports are written under target/stable-chrome-smoke/ by default.
EOF
    exit 2
    ;;
esac

printf '\nSmoke report written to %s\n' "$report"
