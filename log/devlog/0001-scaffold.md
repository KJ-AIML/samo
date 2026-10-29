# Devlog 0001 — Samo Workspace Scaffold

- **Date:** 2026-10-29
- **Summary:** Initialized `repos/samo` workspace with Rust 2024 edition (`1.99.0`), strict workspace deny lints (`unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`, `unreachable`, `dbg_macro`, `unsafe_code`), copied `ADR-001-samo-design.md`, and wired `xtask` verification gates.
- **Rulings:**
  - Embedded SQLite WAL (`~/.samo/samo.db`) used in `samo-store` so personal macOS monitoring works with zero Docker overhead while preserving `TenantTx` isolation and `rls` migration checks.
