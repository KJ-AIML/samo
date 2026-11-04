#!/usr/bin/env bash
# Builds an extension and lays it out as a package in target/ext-dev/<id>/, ready for SAMO_EXT_DEV_DIR.
# The work is done by `cargo run -p xtask -- ext-dev`, which also runs on Windows.
#
#   scripts/ext-dev.sh extensions/hello
#   SAMO_EXT_DEV_DIR="$PWD/target/ext-dev" cargo run --release -p samo-gpui
set -euo pipefail
cd "$(dirname "$0")/.."
exec cargo run -q -p xtask -- ext-dev "${1:?usage: scripts/ext-dev.sh <extension folder>}"
