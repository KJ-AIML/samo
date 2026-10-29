#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> [1/3] Checking Rust formatting..."
cargo fmt --all -- --check

echo "==> [2/3] Running Clippy with strict deny warnings..."
cargo clippy --workspace --all-targets -- -D warnings

echo "==> [3/3] Running cargo xtask verify..."
cargo run -p xtask -- verify
