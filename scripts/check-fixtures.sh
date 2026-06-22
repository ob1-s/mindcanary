#!/usr/bin/env bash
set -euo pipefail

temporary_dir="$(mktemp -d)"
trap 'rm -rf "$temporary_dir"' EXIT

cargo run -q -p mindcanary-test-support --bin generate-fixtures -- "$temporary_dir"

for fixture_file in fixtures/synthetic-browser.jsonl fixtures/synthetic-check-ins.jsonl; do
  generated_file="$temporary_dir/$(basename "$fixture_file")"
  if cmp -s "$fixture_file" "$generated_file"; then
    continue
  fi

  echo "$fixture_file is out of date." >&2
  echo "Run: pnpm fixtures:generate" >&2
  diff -u "$fixture_file" "$generated_file" || true
  exit 1
done
