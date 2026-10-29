#!/usr/bin/env bash
set -euo pipefail

DB_DIR="${SAMO_DATA_DIR:-$HOME/.samo}"
mkdir -p "$DB_DIR"
echo "Samo local SQLite WAL directory ready at: $DB_DIR"
