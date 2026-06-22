#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if ! command -v cargo >/dev/null 2>&1 && [[ -f "${HOME:-}/.cargo/env" ]]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
default_audit_root="$root/target/removal-audit/$timestamp"
audit_root="${MINDCANARY_REMOVAL_AUDIT_DIR:-$default_audit_root}"

fail() {
  printf 'removal audit failed: %s\n' "$1" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

if [[ "${MINDCANARY_REMOVAL_AUDIT_PRIVATE_SESSION:-}" != "1" ]]; then
  require_command cargo
  require_command dbus-run-session
  require_command gnome-keyring-daemon

  cargo build -q \
    -p mindcanaryd \
    -p mindcanary-client \
    -p mindcanary-native-host \
    -p mindcanary-test-support

  mkdir -p "$audit_root/home" "$audit_root/config" "$audit_root/data" "$audit_root/runtime"
  chmod 700 "$audit_root" "$audit_root/home" "$audit_root/config" "$audit_root/data" "$audit_root/runtime"

  export MINDCANARY_REMOVAL_AUDIT_PRIVATE_SESSION=1
  export MINDCANARY_REMOVAL_AUDIT_ROOT="$audit_root"
  export HOME="$audit_root/home"
  export XDG_CONFIG_HOME="$audit_root/config"
  export XDG_DATA_HOME="$audit_root/data"
  export XDG_RUNTIME_DIR="$audit_root/runtime"

  printf 'Starting private MindCanary removal audit session:\n'
  printf '  audit root: %s\n' "$audit_root"
  printf '  private HOME: %s\n' "$HOME"
  printf '  private XDG_CONFIG_HOME: %s\n' "$XDG_CONFIG_HOME"
  printf '  private XDG_DATA_HOME: %s\n' "$XDG_DATA_HOME"
  printf '  private XDG_RUNTIME_DIR: %s\n\n' "$XDG_RUNTIME_DIR"

  exec dbus-run-session -- "$0" --inside-private-session
fi

[[ "${1:-}" == "--inside-private-session" ]] ||
  fail "private audit session was not entered correctly"

audit_root="${MINDCANARY_REMOVAL_AUDIT_ROOT:?}"
case "${HOME:-}" in
  "$audit_root"/home) ;;
  *) fail "HOME is not inside the private audit root: ${HOME:-unset}" ;;
esac
case "${XDG_CONFIG_HOME:-}" in
  "$audit_root"/config) ;;
  *) fail "XDG_CONFIG_HOME is not inside the private audit root: ${XDG_CONFIG_HOME:-unset}" ;;
esac
case "${XDG_DATA_HOME:-}" in
  "$audit_root"/data) ;;
  *) fail "XDG_DATA_HOME is not inside the private audit root: ${XDG_DATA_HOME:-unset}" ;;
esac
case "${XDG_RUNTIME_DIR:-}" in
  "$audit_root"/runtime) ;;
  *) fail "XDG_RUNTIME_DIR is not inside the private audit root: ${XDG_RUNTIME_DIR:-unset}" ;;
esac

daemon="$root/target/debug/mindcanaryd"
ctl="$root/target/debug/mindcanaryctl"
native_host="$root/target/debug/mindcanary-native-host"
seed_consent="$root/target/debug/seed-consent"
send_synthetic="$root/target/debug/send-synthetic"
check_keyring="$root/target/debug/check-keyring"

for binary in "$daemon" "$ctl" "$native_host" "$seed_consent" "$send_synthetic" "$check_keyring"; do
  [[ -x "$binary" ]] || fail "required binary is missing or not executable: $binary"
done

report="$audit_root/report.md"
daemon_log="$audit_root/daemon.log"
keyring_log="$audit_root/keyring.log"
database="$XDG_DATA_HOME/mindcanary/mindcanary.db"
service_file="$XDG_CONFIG_HOME/systemd/user/mindcanaryd.service"
chrome_manifest="$XDG_CONFIG_HOME/google-chrome/NativeMessagingHosts/app.mindcanary.collector.json"
chromium_manifest="$XDG_CONFIG_HOME/chromium/NativeMessagingHosts/app.mindcanary.collector.json"
marker="$XDG_CONFIG_HOME/mindcanary/daemon-package-version"
daemon_pid=""
keyring_pid=""

cleanup() {
  if [[ -n "$daemon_pid" ]] && kill -0 "$daemon_pid" >/dev/null 2>&1; then
    kill "$daemon_pid" >/dev/null 2>&1 || true
    wait "$daemon_pid" >/dev/null 2>&1 || true
  fi
  if [[ -n "$keyring_pid" ]] && kill -0 "$keyring_pid" >/dev/null 2>&1; then
    kill "$keyring_pid" >/dev/null 2>&1 || true
    wait "$keyring_pid" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

append() {
  printf '%s\n' "$*" >>"$report"
}

run_logged() {
  append ''
  append '```bash'
  {
    printf '$'
    printf ' %q' "$@"
    printf '\n'
  } >>"$report"
  append '```'
  append ''
  append '```text'
  "$@" >>"$report" 2>&1
  append '```'
}

assert_file() {
  [[ -f "$1" ]] || fail "expected file to exist: $1"
}

assert_missing() {
  [[ ! -e "$1" ]] || fail "expected path to be removed: $1"
}

wait_for_daemon() {
  for _ in {1..100}; do
    if "$ctl" health >/dev/null 2>&1; then
      return 0
    fi
    if [[ -n "$daemon_pid" ]] && ! kill -0 "$daemon_pid" >/dev/null 2>&1; then
      cat "$daemon_log" >&2 || true
      fail "daemon exited before becoming ready"
    fi
    sleep 0.05
  done
  cat "$daemon_log" >&2 || true
  fail "daemon did not become ready"
}

cat >"$report" <<EOF
# MindCanary Complete Local Removal Audit

- Captured at: $(date -Is)
- Repository: $root
- Branch: $(git branch --show-current 2>/dev/null || printf unknown)
- Commit: $(git rev-parse --short HEAD 2>/dev/null || printf unknown)
- Audit root: $audit_root
- Private HOME: $HOME
- Private XDG_CONFIG_HOME: $XDG_CONFIG_HOME
- Private XDG_DATA_HOME: $XDG_DATA_HOME
- Private XDG_RUNTIME_DIR: $XDG_RUNTIME_DIR

This audit uses a private HOME, private XDG directories, and a private DBus
session with a temporary GNOME keyring daemon. It intentionally runs the real
destructive local-removal command inside that disposable environment, not
against the dogfood profile.
EOF

mkdir -p "$XDG_DATA_HOME/mindcanary" "$XDG_RUNTIME_DIR/mindcanary" "$audit_root/keyring-control"
chmod 700 "$audit_root/keyring-control"
{
  printf '\n' | gnome-keyring-daemon \
    --login \
    --components=secrets \
    --control-directory "$audit_root/keyring-control"
  gnome-keyring-daemon \
    --start \
    --components=secrets \
    --control-directory "$audit_root/keyring-control"
} >"$keyring_log" 2>&1

keyring_ready=false
for _ in {1..100}; do
  if "$check_keyring" --expect absent >>"$report" 2>&1; then
    keyring_ready=true
    break
  fi
  sleep 0.05
done
if [[ "$keyring_ready" != true ]]; then
  cat "$keyring_log" >&2 || true
  fail "private keyring did not become ready"
fi

run_logged "$seed_consent" \
  --database "$database" \
  --enabled-at "2026-01-01T00:00:00Z" \
  --browser-starter
run_logged "$check_keyring" --expect present

"$daemon" >"$daemon_log" 2>&1 &
daemon_pid="$!"
wait_for_daemon

run_logged "$send_synthetic" --browser --check-ins
run_logged "$ctl" summary

kill "$daemon_pid" >/dev/null 2>&1 || true
wait "$daemon_pid" >/dev/null 2>&1 || true
daemon_pid=""

run_logged "$daemon" --install-user-service --daemon-path "$daemon"
run_logged "$native_host" \
  --install-manifest \
  --browser chrome \
  --channel development \
  --host-path "$native_host"
run_logged "$native_host" \
  --install-manifest \
  --browser chromium \
  --channel development \
  --host-path "$native_host"

mkdir -p "$(dirname "$marker")" "$XDG_RUNTIME_DIR/mindcanary"
printf '0.1.0\n' >"$marker"
touch "$XDG_RUNTIME_DIR/mindcanary/stale-audit-socket"

assert_file "$database"
assert_file "$service_file"
assert_file "$chrome_manifest"
assert_file "$chromium_manifest"
assert_file "$marker"
assert_file "$XDG_RUNTIME_DIR/mindcanary/stale-audit-socket"

run_logged bash scripts/uninstall-local.sh --confirm-local-removal

assert_missing "$database"
assert_missing "$database-wal"
assert_missing "$database-shm"
assert_missing "$database-journal"
assert_missing "$service_file"
assert_missing "$chrome_manifest"
assert_missing "$chromium_manifest"
assert_missing "$marker"
assert_missing "$XDG_RUNTIME_DIR/mindcanary"
run_logged "$check_keyring" --expect absent

append ''
append 'PASS: complete local removal cleared app-owned files and the private keyring entry.'

printf 'MindCanary complete local removal audit passed.\n'
printf 'Report: %s\n' "$report"
