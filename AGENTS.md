# AGENTS.md — Samo

## Architecture & Layer Order
Every crate obeys strict layering (`cargo run -p xtask -- verify`):
1. `L0 samo-primitives` (pure)
2. `L1 samo-blueprint` (pure)
3. `L2 samo-domain` (pure: R1–R4 rules, disk classification, project bloat, circle packing, telemetry gaps)
4. `L3 samo-store` (only `sqlx` user; `TenantTx` keeps inner transaction private)
5. `L4 samo-engine` (`CommandRegistry`, default-deny `AccessPolicy`, `Engine::execute` with `catch_unwind` and automatic audit log)
6. `L5 samo-macos` (macOS filesystem scanner, collision-safe `~/.Trash` mover, live telemetry & process collector)
7. `apps/samo-server` & `apps/samo-cli`

## Golden Rules
- Never use `unwrap`, `expect`, `panic`, `todo`, `unimplemented`, `unreachable`, `dbg!`, or `unsafe`.
- Never put OS or network I/O inside `samo-domain` or inside an open `Engine::execute` database transaction; use the 2-phase `*.begin` / `*.finish` pattern.
- Never call `remove_dir_all` or `remove_file` in `samo-macos` cleanup — all user cleanup moves items to `~/.Trash` after verifying `(device_id, inode)` (`R2`).
- Every sensor reading uses `Reading<T>` (`Available`, `Unsupported`, `Stopped`) (`R3`).
- Every process signal verifies `ProcessIdentity { pid, start_time_secs }` (`R4`).
- Run `cargo run -p xtask -- verify` before finishing any task.
- Extensions (ADR-002): helpers run out of process only, never loaded into Samo (R8). Only `samo-extensions` may use an HTTP client, and only after a click (`UserAction`, R6). Unsigned code runs only from `SAMO_EXT_DEV_DIR` (R5).
- Claude Sessions: every change to `~/.claude/settings.json` is declared, backed up and exactly undone (R7); the hook prints nothing, exits 0 and finishes within 500 ms (R9); unknown is never `0` and a dead session is never shown working (R10).
