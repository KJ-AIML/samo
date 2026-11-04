#!/usr/bin/env bash
# What the Windows CI job runs (ADR-006 R30), and what to run on a Windows PC before pushing.
#
# The whole workspace builds and tests on Windows, Clips for Windows (#11) included; its real-clipboard tests run on
# CI, or with SAMO_TEST_CLIPBOARD=1. Swift (Shots, the Mac's Clips) and the tests that run real unix helpers stay
# with the macOS jobs until Shots' Windows form comes (#12).
# On a PC whose rustup default host is GNU, run it with RUSTUP_TOOLCHAIN=1.99.0-x86_64-pc-windows-msvc.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> [1/4] Checking Rust formatting..."
cargo fmt --all -- --check

echo "==> [2/4] Running Clippy on the workspace..."
cargo clippy --workspace --all-targets -- -D warnings

echo "==> [3/4] Running the static xtask gates (Swift and the workspace tests are macOS jobs)..."
XTASK_SKIP_TESTS=1 cargo run -p xtask -- verify

echo "==> [4/4] Testing the workspace..."
cargo test --workspace
