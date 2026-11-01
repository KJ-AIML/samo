#!/usr/bin/env bash
# Builds an extension and lays it out as a package in target/ext-dev/<id>/, ready for SAMO_EXT_DEV_DIR.
#
#   scripts/ext-dev.sh extensions/hello
#   SAMO_EXT_DEV_DIR="$PWD/target/ext-dev" cargo run --release -p samo-gpui
set -euo pipefail
cd "$(dirname "$0")/.."

src="${1:?usage: scripts/ext-dev.sh <extension folder>}"
manifest="$src/extension.toml"
field() { sed -n "s/^$1 *= *\"\\(.*\\)\"/\\1/p" "$2" | head -1; }
id=$(field id "$manifest")
helper=$(field helper "$manifest")
package=$(field name "$src/Cargo.toml")
bin=$(basename "$helper")

cargo build --release -p "$package" --bin "$bin"

out="target/ext-dev/$id"
rm -rf "$out"
mkdir -p "$out/$(dirname "$helper")"
cp "$manifest" "$out/extension.toml"
# A copy, not cargo's hard link: Samo refuses hard links in a package (ADR-002 R5).
cp "target/release/$bin" "$out/$helper"
echo "Laid out $id in $out"
echo "Run: SAMO_EXT_DEV_DIR=\"$PWD/target/ext-dev\" cargo run --release -p samo-gpui"
