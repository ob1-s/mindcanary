#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v cargo >/dev/null 2>&1 && [[ -f "${HOME:-}/.cargo/env" ]]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

usage() {
  cat >&2 <<'EOF'
Usage: scripts/with-dev-profile.sh <profile-name> -- <command> [args...]

Runs a command with isolated XDG data, runtime, and config directories under
target/dev-profiles/<profile-name>/ so development can avoid the default
MindCanary dogfood profile.

Examples:
  scripts/with-dev-profile.sh onboarding -- cargo run -p mindcanaryd
  scripts/with-dev-profile.sh onboarding -- pnpm --filter @mindcanary/desktop tauri dev
  scripts/with-dev-profile.sh onboarding -- cargo run -q -p mindcanary-client --bin mindcanaryctl -- summary
EOF
}

if [[ $# -lt 3 || "$2" != "--" ]]; then
  usage
  exit 2
fi

profile="$1"
shift 2

case "$profile" in
  "" | *[!A-Za-z0-9._-]*)
    printf 'Profile name must contain only letters, numbers, dots, underscores, or dashes.\n' >&2
    exit 2
    ;;
esac

base="${MINDCANARY_DEV_PROFILE_ROOT:-$root/target/dev-profiles}/$profile"
export XDG_RUNTIME_DIR="$base/run"
export XDG_DATA_HOME="$base/data"
export XDG_CONFIG_HOME="$base/config"
export MINDCANARY_PROFILE="$profile"

mkdir -p "$XDG_RUNTIME_DIR" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME"
chmod 700 "$base" "$XDG_RUNTIME_DIR" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME"

printf 'Using isolated MindCanary profile: %s\n' "$profile"
printf '  XDG_RUNTIME_DIR=%s\n' "$XDG_RUNTIME_DIR"
printf '  XDG_DATA_HOME=%s\n' "$XDG_DATA_HOME"
printf '  XDG_CONFIG_HOME=%s\n\n' "$XDG_CONFIG_HOME"

exec "$@"
