#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

timestamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

hash_file() {
  local path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
  else
    shasum -a 256 "$path" | awk '{print $1}'
  fi
}

hash_directory() {
  local path="$1"
  if [[ ! -d "$path" ]]; then
    return 1
  fi
  find "$path" -type f -print0 \
    | sort -z \
    | xargs -0 sha256sum \
    | sha256sum \
    | awk '{print $1}'
}

latest_reliability_report() {
  shopt -s nullglob
  local reports=("$root"/target/reliability-local/*/report.md)
  shopt -u nullglob
  if [[ "${#reports[@]}" -eq 0 ]]; then
    return 0
  fi
  printf '%s\n' "${reports[@]}" | sort | tail -1
}

printf '# MindCanary Release Snapshot\n\n'
printf -- '- Captured at: %s\n' "$timestamp"
printf -- '- Branch: %s\n' "$(git branch --show-current 2>/dev/null || printf unknown)"
printf -- '- Commit: %s\n' "$(git rev-parse HEAD 2>/dev/null || printf unknown)"
printf -- '- Working tree dirty: %s\n' "$(if [[ -n "$(git status --short)" ]]; then printf yes; else printf no; fi)"
printf -- '- Protocol version: 1\n'

printf '\n## Desktop Packages\n\n'
shopt -s nullglob
packages=("$root"/apps/desktop/src-tauri/target/release/bundle/deb/*.deb)
shopt -u nullglob
if [[ "${#packages[@]}" -eq 0 ]]; then
  printf 'No Debian package artifacts found.\n'
else
  for package in "${packages[@]}"; do
    printf -- '- `%s`: `%s`\n' "${package#$root/}" "$(hash_file "$package")"
  done
fi

printf '\n## Extension Build\n\n'
if extension_hash="$(hash_directory "$root/apps/extension/dist" 2>/dev/null)"; then
  printf -- '- `apps/extension/dist`: `%s`\n' "$extension_hash"
else
  printf 'No extension dist directory found.\n'
fi

printf '\n## Reliability\n\n'
report="$(latest_reliability_report)"
if [[ -n "$report" ]]; then
  printf -- '- Latest local reliability report: `%s`\n' "${report#$root/}"
else
  printf 'No local reliability report found.\n'
fi

cat <<'EOF'

## Public Alpha Boundaries

- License: AGPL-3.0-or-later
- Release host: GitHub Releases
- Browser extension: optional; no ordinary-user store release yet
- Package signing: checksum only during the early alpha
- Support: GitHub Issues for non-sensitive reports
- Security: GitHub Private Vulnerability Reporting
EOF
