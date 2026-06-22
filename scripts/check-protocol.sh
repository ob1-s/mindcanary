#!/usr/bin/env bash
set -euo pipefail

generated_file="packages/protocol-ts/src/generated.ts"
temporary_file="$(mktemp)"
trap 'rm -f "$temporary_file"' EXIT

cargo run -q -p mindcanary-protocol --bin export-typescript -- "$temporary_file"

if ! cmp -s "$generated_file" "$temporary_file"; then
  echo "$generated_file is out of date." >&2
  echo "Run: pnpm protocol:generate" >&2
  diff -u "$generated_file" "$temporary_file" || true
  exit 1
fi
