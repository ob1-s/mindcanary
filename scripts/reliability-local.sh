#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if ! command -v cargo >/dev/null 2>&1 && [[ -f "${HOME:-}/.cargo/env" ]]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi
if ! command -v pnpm >/dev/null 2>&1 && [[ -f "${HOME:-}/.nvm/nvm.sh" ]]; then
  # shellcheck disable=SC1091
  source "$HOME/.nvm/nvm.sh"
fi

timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
report_root="${MINDCANARY_RELIABILITY_DIR:-$root/target/reliability-local}"
run_dir="$report_root/$timestamp"
profile_dir="$run_dir/profile"
runtime_dir="${TMPDIR:-/tmp}/mindcanary-rel-$timestamp"
data_dir="$profile_dir/data"
config_dir="$profile_dir/config"
socket="$runtime_dir/mindcanaryd.sock"
database="$data_dir/mindcanary/mindcanary.db"
report="$run_dir/report.md"
daemon_log="$run_dir/daemon.log"
export_dir="$run_dir/export"
manifest_dir="$run_dir/native-host-manifest"
cleanup_service_dir="$run_dir/local-removal-service"
cleanup_manifest_dir="$run_dir/local-removal-native-manifest"
cleanup_config_home="$run_dir/local-removal-config"
cleanup_runtime_dir="$run_dir/local-removal-runtime"
daemon_pid=""
restart_count=0

mkdir -p \
  "$runtime_dir" \
  "$data_dir/mindcanary" \
  "$config_dir" \
  "$run_dir" \
  "$manifest_dir" \
  "$cleanup_service_dir" \
  "$cleanup_manifest_dir" \
  "$cleanup_config_home/mindcanary" \
  "$cleanup_runtime_dir/mindcanary"
chmod 700 "$profile_dir" "$runtime_dir" "$data_dir" "$config_dir"

cleanup() {
  stop_daemon
  rm -rf "$runtime_dir"
}
trap cleanup EXIT

fail() {
  printf 'reliability lab failed: %s\n' "$1" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

append_section() {
  printf '\n## %s\n\n' "$1" >>"$report"
}

append_command() {
  local title="$1"
  shift
  append_section "$title"
  {
    printf '```bash\n'
    printf '$'
    printf ' %q' "$@"
    printf '\n```\n\n'
    printf '```text\n'
    "$@"
    printf '```\n'
  } >>"$report" 2>&1
}

append_command_expect_exact() {
  local title="$1"
  local expected="$2"
  shift 2
  local output

  append_section "$title"
  {
    printf '```bash\n'
    printf '$'
    printf ' %q' "$@"
    printf '\n```\n\n'
  } >>"$report"

  if ! output="$("$@" 2>&1)"; then
    {
      printf '```text\n'
      printf '%s\n' "$output"
      printf '```\n'
    } >>"$report"
    fail "$title command failed"
  fi

  {
    printf '```text\n'
    printf '%s\n' "$output"
    printf '```\n'
  } >>"$report"

  [[ "$output" == "$expected" ]] || fail "$title expected '$expected' but got '$output'"
}

assert_file() {
  [[ -f "$1" ]] || fail "expected file does not exist: $1"
}

assert_contains() {
  local file="$1"
  local pattern="$2"
  rg -q "$pattern" "$file" || fail "expected $file to contain pattern: $pattern"
}

wait_for_daemon_ready() {
  local log_path="$1"
  for _ in {1..100}; do
    if "$root/target/debug/mindcanaryctl" --socket "$socket" health >/dev/null 2>&1; then
      return 0
    fi
    if [[ -n "$daemon_pid" ]] && ! kill -0 "$daemon_pid" >/dev/null 2>&1; then
      cat "$log_path" >&2 || true
      fail "isolated daemon exited before becoming ready"
    fi
    sleep 0.05
  done
  cat "$log_path" >&2 || true
  fail "isolated daemon did not become ready"
}

start_daemon() {
  local log_path="$1"
  "$root/target/debug/mindcanaryd" \
    --socket "$socket" \
    --database "$database" \
    >"$log_path" 2>&1 &
  daemon_pid="$!"
  wait_for_daemon_ready "$log_path"
}

stop_daemon() {
  if [[ -n "$daemon_pid" ]] && kill -0 "$daemon_pid" >/dev/null 2>&1; then
    kill "$daemon_pid" >/dev/null 2>&1 || true
    wait "$daemon_pid" >/dev/null 2>&1 || true
  fi
  daemon_pid=""
}

restart_daemon() {
  restart_count=$((restart_count + 1))
  stop_daemon
  start_daemon "$run_dir/daemon-restart-$restart_count.log"
}

package_artifact() {
  shopt -s nullglob
  local packages=("$root"/apps/desktop/src-tauri/target/release/bundle/deb/*.deb)
  shopt -u nullglob
  if [[ "${#packages[@]}" -eq 1 ]]; then
    printf '%s\n' "${packages[0]}"
  fi
}

privacy_scan() {
  local scan_dir="$1"
  local scan_output="$run_dir/privacy-scan.txt"
  if rg -n -i 'https?://|www\.|active_url|pendingUrl|favIconUrl|browser\.raw_url|page_text|search_term|typed_text|key_press|keystroke=' "$scan_dir" >"$scan_output"; then
    cat "$scan_output"
    fail "privacy scan found prohibited raw-content patterns"
  fi
  printf 'No prohibited raw-content patterns found under %s\n' "$scan_dir"
}

require_command cargo
require_command pnpm
require_command rg
require_command ps

cat >"$report" <<EOF
# MindCanary Local Reliability Report

- Captured at: $(date -Is)
- Repository: $root
- Branch: $(git branch --show-current 2>/dev/null || printf unknown)
- Commit: $(git rev-parse --short HEAD 2>/dev/null || printf unknown)
- Working tree dirty: $(if [[ -n "$(git status --short)" ]]; then printf yes; else printf no; fi)
- Isolated socket: $socket
- Isolated database: $database

This report uses an isolated development profile. It does not clear, migrate,
or destroy the default dogfood profile.
EOF

append_command "Tool Versions" bash -c 'rustc --version; cargo --version; node --version; pnpm --version'

append_command "Build Reliability Helpers" cargo build -q \
  -p mindcanaryd \
  -p mindcanary-client \
  -p mindcanary-native-host \
  -p mindcanary-test-support

native_host=("$root/target/debug/mindcanary-native-host")

append_command_expect_exact "Native Host Manifest Initially Missing" "missing" \
  "${native_host[@]}" \
  --check-manifest \
  --browser chrome \
  --channel development \
  --host-path "$root/target/debug/mindcanary-native-host" \
  --manifest-dir "$manifest_dir"

append_command "Install Native Host Manifest" \
  "${native_host[@]}" \
  --install-manifest \
  --browser chrome \
  --channel development \
  --host-path "$root/target/debug/mindcanary-native-host" \
  --manifest-dir "$manifest_dir"

append_command_expect_exact "Native Host Manifest Ready" "ready" \
  "${native_host[@]}" \
  --check-manifest \
  --browser chrome \
  --channel development \
  --host-path "$root/target/debug/mindcanary-native-host" \
  --manifest-dir "$manifest_dir"

append_command "Corrupt Native Host Manifest Host Path" \
  "${native_host[@]}" \
  --install-manifest \
  --browser chrome \
  --channel development \
  --host-path "$root/target/debug/mindcanaryd" \
  --manifest-dir "$manifest_dir"

append_command_expect_exact "Native Host Manifest Needs Repair" "needs_repair" \
  "${native_host[@]}" \
  --check-manifest \
  --browser chrome \
  --channel development \
  --host-path "$root/target/debug/mindcanary-native-host" \
  --manifest-dir "$manifest_dir"

append_command "Repair Native Host Manifest" \
  "${native_host[@]}" \
  --install-manifest \
  --browser chrome \
  --channel development \
  --host-path "$root/target/debug/mindcanary-native-host" \
  --manifest-dir "$manifest_dir"

append_command_expect_exact "Native Host Manifest Ready After Repair" "ready" \
  "${native_host[@]}" \
  --check-manifest \
  --browser chrome \
  --channel development \
  --host-path "$root/target/debug/mindcanary-native-host" \
  --manifest-dir "$manifest_dir"

append_command "Uninstall Native Host Manifest" \
  "${native_host[@]}" \
  --uninstall-manifest \
  --browser chrome \
  --manifest-dir "$manifest_dir"

append_command_expect_exact "Native Host Manifest Missing After Uninstall" "missing" \
  "${native_host[@]}" \
  --check-manifest \
  --browser chrome \
  --channel development \
  --host-path "$root/target/debug/mindcanary-native-host" \
  --manifest-dir "$manifest_dir"

append_command "Complete Local Removal Dry Run" env \
  MINDCANARY_DAEMON_BIN="$root/target/debug/mindcanaryd" \
  MINDCANARY_NATIVE_HOST_BIN="$root/target/debug/mindcanary-native-host" \
  MINDCANARY_SERVICE_DIR="$cleanup_service_dir" \
  MINDCANARY_NATIVE_MANIFEST_DIR="$cleanup_manifest_dir" \
  XDG_CONFIG_HOME="$cleanup_config_home" \
  XDG_RUNTIME_DIR="$cleanup_runtime_dir" \
  bash scripts/uninstall-local.sh \
  --dry-run \
  --database "$run_dir/local-removal/mindcanary.db"
assert_contains "$report" 'uninstall-user-service'
assert_contains "$report" 'uninstall-manifest'
assert_contains "$report" 'destroy-local-profile'
assert_contains "$report" 'No files, services, manifests, database keys, or browser-owned storage were'

append_command "Seed Fixture-Era Browser Consent" "$root/target/debug/seed-consent" \
  --database "$database" \
  --enabled-at "2026-01-01T00:00:00Z" \
  --browser-starter

append_section "Start Isolated Daemon"
{
  printf '```text\n'
  printf 'daemon: %s\n' "$root/target/debug/mindcanaryd"
  printf 'socket: %s\n' "$socket"
  printf 'database: %s\n' "$database"
  printf 'log: %s\n' "$daemon_log"
  printf '```\n'
} >>"$report"

start_daemon "$daemon_log"

ctl=("$root/target/debug/mindcanaryctl" --socket "$socket")
synthetic=("$root/target/debug/send-synthetic" --socket "$socket")

append_command "Initial Health" "${ctl[@]}" health
append_command "Initial Source Status" "${ctl[@]}" source-status
append_command "Initial Summary" "${ctl[@]}" summary
append_command "Seeded Collection Settings" "${ctl[@]}" settings
assert_contains "$report" 'Aggregate batches: 0'
assert_contains "$report" 'Check-ins: 0'

append_command "Consent Pause Backfill Test" cargo test -p mindcanaryd \
  delayed_retries_cannot_backfill_a_paused_period
append_command "Duplicate Browser Batch Test" cargo test -p mindcanaryd \
  duplicate_batches_are_idempotent
append_command "Duplicate Check-In Test" cargo test -p mindcanaryd \
  duplicate_check_ins_are_idempotent
append_command "Local Export Confirmation Test" cargo test -p mindcanaryd \
  local_export_requires_confirmation_and_writes_report_and_csvs
append_command "Local Clear Confirmation Test" cargo test -p mindcanaryd \
  local_record_clear_requires_a_valid_unexpired_confirmation
append_command "Signal Deletion Confirmation Test" cargo test -p mindcanaryd \
  signal_deletion_confirmation_is_bound_and_preserves_other_records

append_command "Insight Synthetic False-Nudge Budget Test" cargo test -p mindcanary-analytics \
  launch_rule_meets_the_synthetic_false_nudge_budget

append_command "Native Host Unit Fault Tests" cargo test -p mindcanary-native-host

append_command "Focused Extension Fault Tests" pnpm --filter @mindcanary/extension test -- \
  status queue reducer tab-retention

append_command "Focused Desktop Onboarding Tests" pnpm --filter @mindcanary/desktop test -- \
  setup source-status data-controls local-removal

append_command "Desktop Lifecycle Command Tests" cargo test \
  --manifest-path apps/desktop/src-tauri/Cargo.toml \
  lifecycle

append_command "Check-In Only Ingestion" "${synthetic[@]}" --check-ins
append_command "Summary After Check-Ins" "${ctl[@]}" summary
assert_contains "$report" 'Check-ins: 5'

append_command "Synthetic Browser Ingestion With One Duplicate" \
  "${synthetic[@]}" --browser --repeat-first-browser
assert_contains "$report" 'duplicate=1'

append_command "Summary After Browser Ingestion" "${ctl[@]}" summary
assert_contains "$report" 'Aggregate batches: 20'

append_command "Source Status After Ingestion" "${ctl[@]}" source-status
append_command "Resource Snapshot" ps -o pid,ppid,stat,%cpu,rss,etime,comm -p "$daemon_pid"

append_section "Daemon Restart Recovery"
{
  printf '```text\n'
  printf 'Stopping and restarting isolated daemon against the same encrypted database.\n'
  printf 'Before restart PID: %s\n' "$daemon_pid"
  restart_daemon
  printf 'After restart PID: %s\n' "$daemon_pid"
  printf 'Restart log: %s\n' "$run_dir/daemon-restart-$restart_count.log"
  printf '```\n'
} >>"$report"

append_command "Health After Daemon Restart" "${ctl[@]}" health
append_command "Summary After Daemon Restart" "${ctl[@]}" summary
assert_contains "$report" 'Aggregate batches: 20'
assert_contains "$report" 'Check-ins: 5'
append_command "Source Status After Daemon Restart" "${ctl[@]}" source-status
append_command "Duplicate Replay After Daemon Restart" \
  "${synthetic[@]}" --browser --repeat-first-browser
assert_contains "$report" 'duplicate=21'
append_command "Summary After Duplicate Replay" "${ctl[@]}" summary
assert_contains "$report" 'Aggregate batches: 20'

append_command "Local Export Smoke" "${ctl[@]}" export --directory "$export_dir"
assert_file "$export_dir/mindcanary-report.md"
assert_file "$export_dir/daily-browser.csv"
assert_file "$export_dir/daily-check-ins.csv"
assert_file "$export_dir/daily-os.csv"

append_section "Export Privacy Scan"
{
  printf '```text\n'
  privacy_scan "$export_dir"
  printf '```\n'
} >>"$report"

package_path="$(package_artifact || true)"
if [[ -n "$package_path" ]]; then
  append_command "Linux Package Artifact Smoke" bash scripts/check-linux-package.sh "$package_path"
else
  append_section "Linux Package Artifact Smoke"
  {
    printf '```text\n'
    printf 'Skipped: no single .deb artifact found under apps/desktop/src-tauri/target/release/bundle/deb/.\n'
    printf 'Build one with `pnpm package:linux`, then rerun `pnpm reliability:local` to include package smoke evidence.\n'
    printf '```\n'
  } >>"$report"
fi

append_command "Daemon Log Tail" tail -80 "$daemon_log"
if [[ "$restart_count" -gt 0 ]]; then
  append_command "Restarted Daemon Log Tail" tail -80 "$run_dir/daemon-restart-$restart_count.log"
fi

printf '\nReliability report written to %s\n' "$report"
