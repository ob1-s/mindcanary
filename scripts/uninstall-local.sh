#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

confirm=false
dry_run=false
database=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --confirm-local-removal)
      confirm=true
      shift
      ;;
    --dry-run)
      dry_run=true
      shift
      ;;
    --database)
      database="${2:-}"
      [[ -n "$database" ]] || {
        printf '--database requires a path.\n' >&2
        exit 1
      }
      shift 2
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      exit 1
      ;;
  esac
done

if [[ "$confirm" != true && "$dry_run" != true ]]; then
  cat >&2 <<'EOF'
Refusing to remove local MindCanary state without --confirm-local-removal.

This command removes the user service, native-host manifests, default encrypted
database profile, SQLite sidecars, OS-keyring database key, package setup
marker, and runtime socket directory. It does not remove Chrome extension
storage or user-created exports/backups.
EOF
  exit 1
fi

daemon_bin="${MINDCANARY_DAEMON_BIN:-$root/target/debug/mindcanaryd}"
native_host_bin="${MINDCANARY_NATIVE_HOST_BIN:-$root/target/debug/mindcanary-native-host}"

require_executable() {
  [[ -x "$1" ]] || {
    printf 'Required executable is missing or not executable: %s\n' "$1" >&2
    printf 'Build local binaries with: cargo build -p mindcanaryd -p mindcanary-native-host\n' >&2
    exit 1
  }
}

append_if_set() {
  local -n target="$1"
  local name="$2"
  local value="$3"
  if [[ -n "$value" ]]; then
    target+=("$name" "$value")
  fi
}

require_executable "$daemon_bin"
require_executable "$native_host_bin"

run_command() {
  if [[ "$dry_run" == true ]]; then
    printf '$'
    printf ' %q' "$@"
    printf '\n'
    return 0
  fi

  "$@"
}

service_dir="${MINDCANARY_SERVICE_DIR:-}"
manifest_dir="${MINDCANARY_NATIVE_MANIFEST_DIR:-}"

if [[ -z "$service_dir" ]] && command -v systemctl >/dev/null 2>&1; then
  if [[ "$dry_run" == true ]]; then
    run_command systemctl --user disable --now mindcanaryd.service
  else
    systemctl --user disable --now mindcanaryd.service >/dev/null 2>&1 || true
  fi
fi

service_args=(--uninstall-user-service)
append_if_set service_args --service-dir "$service_dir"
run_command "$daemon_bin" "${service_args[@]}"

for browser in chrome chromium; do
  manifest_args=(--uninstall-manifest --browser "$browser")
  append_if_set manifest_args --manifest-dir "$manifest_dir"
  run_command "$native_host_bin" "${manifest_args[@]}"
done

destroy_args=(--destroy-local-profile --confirm-destroy-local-profile)
append_if_set destroy_args --database "$database"
run_command "$daemon_bin" "${destroy_args[@]}"

config_home="${XDG_CONFIG_HOME:-${HOME:-}/.config}"
if [[ -n "$config_home" ]]; then
  marker="$config_home/mindcanary/daemon-package-version"
  run_command rm -f "$marker"
  if [[ "$dry_run" == true ]]; then
    run_command rmdir "$config_home/mindcanary"
  else
    rmdir "$config_home/mindcanary" >/dev/null 2>&1 || true
  fi
fi

runtime_home="${XDG_RUNTIME_DIR:-}"
if [[ -n "$runtime_home" ]]; then
  run_command rm -rf "$runtime_home/mindcanary"
fi

if [[ "$dry_run" == true ]]; then
  cat <<'EOF'
MindCanary local removal dry run completed.

No files, services, manifests, database keys, or browser-owned storage were
removed.
EOF
  exit 0
fi

cat <<'EOF'
MindCanary local removal completed.

Chrome extension storage is owned by Chrome. Remove the extension or clear its
site data from Chrome if you also want to remove browser-owned extension state.
User-created exports and backups are external files and were not removed.
EOF
