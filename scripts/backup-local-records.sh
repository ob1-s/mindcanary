#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if ! command -v cargo >/dev/null 2>&1 && [[ -f "${HOME:-}/.cargo/env" ]]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

default_backup_root="${HOME:-$root/target}/Documents/mindcanary-backups"
backup_root="${MINDCANARY_BACKUP_DIR:-$default_backup_root}"
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"

mkdir -p "$backup_root"
backup_root="$(cd "$backup_root" && pwd)"
export_dir="$backup_root/$timestamp-local-export"

ctl_args=()
if [[ -n "${MINDCANARY_SOCKET:-}" ]]; then
  ctl_args+=(--socket "$MINDCANARY_SOCKET")
fi

printf 'Preparing the local safety-export client...\n'
cargo build -q -p mindcanary-client --bin mindcanaryctl

target_dir="${CARGO_TARGET_DIR:-$root/target}"
ctl="$target_dir/debug/mindcanaryctl"
request_timeout="${MINDCANARY_BACKUP_TIMEOUT_SECONDS:-20}"

set +e
timeout --foreground "${request_timeout}s" "$ctl" \
  "${ctl_args[@]}" \
  export \
  --directory "$export_dir"
status=$?
set -e

if [[ $status -eq 124 ]]; then
  printf 'MindCanary local export timed out after %s seconds.\n' "$request_timeout" >&2
  printf 'The local daemon is not responding; restart it, then run this command again.\n' >&2
  exit 1
fi
if [[ $status -ne 0 ]]; then
  exit "$status"
fi

printf '\nMindCanary local export written to %s\n' "$export_dir"
printf 'Keep this directory private; it contains readable local records.\n'
