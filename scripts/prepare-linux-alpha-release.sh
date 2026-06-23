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

build=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --build)
      build=true
      shift
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'missing required command: %s\n' "$1" >&2
    exit 1
  }
}

hash_file() {
  local path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
  else
    shasum -a 256 "$path" | awk '{print $1}'
  fi
}

require_command git
require_command dpkg-deb
require_command sha256sum
require_command zip

if [[ "$build" == true ]]; then
  require_command pnpm
  pnpm --filter @mindcanary/extension build
  pnpm package:linux
fi

shopt -s nullglob
packages=("$root"/apps/desktop/src-tauri/target/release/bundle/deb/*.deb)
shopt -u nullglob
if [[ "${#packages[@]}" -ne 1 ]]; then
  printf 'Expected exactly one .deb artifact, found %s. Run with --build or clean old artifacts.\n' "${#packages[@]}" >&2
  exit 1
fi

source_deb="${packages[0]}"
version="$(dpkg-deb -f "$source_deb" Version)"
arch="$(dpkg-deb -f "$source_deb" Architecture)"
package_name="$(dpkg-deb -f "$source_deb" Package)"
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
release_name="mindcanary-${version}-linux-alpha-${timestamp}"
release_dir="$root/target/linux-alpha-release/$release_name"
latest_link="$root/target/linux-alpha-release/latest"
deb_name="MindCanary_${version}_${arch}.deb"
extension_source_dir="$root/apps/extension/dist"
extension_dir_name="chrome-extension"
extension_zip_name="MindCanary-Chrome-Extension_${version}.zip"

if [[ ! -f "$extension_source_dir/manifest.json" ]]; then
  printf 'Expected built Chrome extension at %s.\n' "$extension_source_dir" >&2
  printf 'Run `pnpm --filter @mindcanary/extension build`, or rerun this script with --build.\n' >&2
  exit 1
fi

rm -rf "$release_dir"
mkdir -p "$release_dir"
cp "$source_deb" "$release_dir/$deb_name"
cp -R "$extension_source_dir" "$release_dir/$extension_dir_name"
cp docs/alpha-install.md "$release_dir/INSTALL.md"
cp docs/alpha-feedback.md "$release_dir/FEEDBACK.md"
cp docs/privacy-policy.md "$release_dir/PRIVACY.md"
cp docs/known-limitations.md "$release_dir/KNOWN_LIMITATIONS.md"
cp docs/support.md "$release_dir/SUPPORT.md"

(
  cd "$release_dir"
  zip -qr "$extension_zip_name" "$extension_dir_name"
  sha256sum "$deb_name" "$extension_zip_name" >SHA256SUMS
)

deb_hash="$(hash_file "$release_dir/$deb_name")"
extension_hash="$(hash_file "$release_dir/$extension_zip_name")"
commit="$(git rev-parse HEAD 2>/dev/null || printf unknown)"
short_commit="$(git rev-parse --short HEAD 2>/dev/null || printf unknown)"
dirty="$(if [[ -n "$(git status --short)" ]]; then printf yes; else printf no; fi)"

cat >"$release_dir/RELEASE_NOTES.md" <<EOF
# MindCanary ${version} Linux Alpha

This is an early Linux alpha build for Pop!_OS, Ubuntu, and similar
Debian-based systems.

## Download

- Package: \`${deb_name}\`
- Optional Chrome extension: \`${extension_zip_name}\`
- SHA-256: \`${deb_hash}\`
- Extension SHA-256: \`${extension_hash}\`
- Install guide: \`INSTALL.md\`
- Feedback guide: \`FEEDBACK.md\`
- Privacy boundary: \`PRIVACY.md\`
- Known limitations: \`KNOWN_LIMITATIONS.md\`
- Support boundary: \`SUPPORT.md\`

## What This Build Is

MindCanary is a private local journal for noticing how your routines change
over time. It stores records locally and does not require an account, telemetry,
cloud sync, hosted dashboard, subscription, or AI service.

This build is not a medical, diagnostic, emergency, prediction, or treatment
product.

## What To Test First

1. Install the \`.deb\`.
2. Open MindCanary from the app launcher.
3. Save one check-in.
4. Close and reopen the app.
5. Confirm the check-in appears in daily history.
6. Open Data and preview support information; confirm it contains no private
   record values.

Chrome is optional. Testers who choose browser aggregates can unzip
\`${extension_zip_name}\`, load the bundled \`chrome-extension\` folder from
\`chrome://extensions\`, and wait up to one 15-minute period for the first
aggregate batch.

## Privacy Boundary

The current local product must not store URLs, page titles, page text, search
terms, message contents, screenshots, keystrokes, or raw browsing history.

Please do not send personal exports, databases, or screenshots containing
private records when reporting issues.

## Build Metadata

- Package name: \`${package_name}\`
- Version: \`${version}\`
- Architecture: \`${arch}\`
- Commit: \`${commit}\`
- Short commit: \`${short_commit}\`
- Working tree dirty when bundled: \`${dirty}\`
- License: AGPL-3.0-or-later
- Support: GitHub Issues for non-sensitive reports
- Security: GitHub Private Vulnerability Reporting

## Known Limitations

- Linux only for now.
- Chrome connector setup is optional and still rough without the Chrome Web
  Store listing. It uses the bundled unpacked extension instead.
- Pattern explanations need enough comparable local history.
- Missing days remain missing and are not treated as zero.
- Repeated same-day check-ins are stored separately but summarized by daily
  average and count in History and the current CSV export.
- Local encryption cannot protect against malware running as the same unlocked
  OS user.
- Support is best-effort during the early alpha.
EOF

cat >"$release_dir/GITHUB_RELEASE_DRAFT.md" <<EOF
# MindCanary ${version} Linux Alpha

Attach these files:

- \`${deb_name}\`
- \`${extension_zip_name}\`
- \`SHA256SUMS\`
- \`INSTALL.md\`
- \`FEEDBACK.md\`
- \`PRIVACY.md\`
- \`KNOWN_LIMITATIONS.md\`
- \`SUPPORT.md\`
- \`RELEASE_NOTES.md\`

Suggested release body:

\`\`\`markdown
$(cat "$release_dir/RELEASE_NOTES.md")
\`\`\`
EOF

ln -sfn "$release_name" "$latest_link"

printf 'Linux alpha release bundle prepared:\n'
printf '  %s\n' "$release_dir"
printf '  %s -> %s\n' "$latest_link" "$release_name"
printf '\nFiles:\n'
find "$release_dir" -maxdepth 1 -type f -printf '  %f\n' | sort
printf '\nSHA-256:\n'
cat "$release_dir/SHA256SUMS"
