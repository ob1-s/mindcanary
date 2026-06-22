#!/usr/bin/env bash
set -euo pipefail

cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
pnpm protocol:check
pnpm fixtures:check
pnpm format:check
pnpm lint
pnpm test
pnpm build
