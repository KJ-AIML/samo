#!/usr/bin/env bash
# What the Windows CI job runs (ADR-006 R30), and what to run on a Windows PC before pushing.
#
# Until #1 is done, only these crates build on Windows; the list grows with each PR until it is the workspace.
# On a PC whose rustup default host is GNU, run it with RUSTUP_TOOLCHAIN=1.99.0-x86_64-pc-windows-msvc.
set -euo pipefail

cd "$(dirname "$0")/.."

CRATES=(
  samo-primitives
  samo-blueprint
  samo-ext-protocol
  samo-domain
  samo-store
  samo-engine
  samo-testkit
  samo-ext-hello
  xtask
)
PACKAGES=()
for crate in "${CRATES[@]}"; do
  PACKAGES+=(-p "$crate")
done

echo "==> [1/4] Checking Rust formatting..."
cargo fmt --all -- --check

echo "==> [2/4] Running Clippy on the crates that build on Windows..."
cargo clippy "${PACKAGES[@]}" --all-targets -- -D warnings

echo "==> [3/4] Running the static xtask gates (Swift and the workspace tests are macOS jobs)..."
XTASK_SKIP_TESTS=1 cargo run -p xtask -- verify

echo "==> [4/4] Testing the crates that build on Windows..."
cargo test "${PACKAGES[@]}"
