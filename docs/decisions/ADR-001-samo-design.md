# ADR-001: Samo — Personal macOS Storage & System Instrument Design

**Date:** 2026-10-29  
**Status:** Proposed (Gate 2 Review)  
**Product:** Samo (`samo`)

---

## 1. Intent

**[KJ]** Samo is a personal macOS monitoring and storage management app inspired by [TMOG](https://tmog.org/), built because KJ needs a way on his Mac to see what kind of files and projects are eating too much disk space (a feature TMOG locks behind its paid Pro tier), while also keeping TMOG's live CPU, memory, and process monitoring tabs in one place.

**What Samo is:**
- **[KJ]** A personal macOS instrument for understanding disk space by file type, folder, and project alongside live CPU, memory, and process telemetry.
- **[call]** A local-first Talvyn application whose UI and telemetry follow TMOG's precision-instrument discipline: every reading carries an explicit availability state (`Available(T)`, `Unsupported`, `Stopped`), stopped measurements leave honest gaps instead of interpolated lines, and process actions verify `(pid, start_time_secs)` identity before sending any signal.

**What Samo is not:**
- **[call]** Not a cloud SaaS agent that phones home telemetry or requires an external account.
- **[call]** Not an automated background "junk cleaner" that silently deletes files without explicit user action.
- **[call]** Not a cross-platform Windows/Linux port in v1; it targets KJ's personal macOS environment first.

**Why it is new rather than a reuse:**
- Existing Talvyn products (`talvyn-crm-archetype`, `talvyn-mesh`) are multi-user server applications for business workflows and team coordination. Samo applies the Talvyn layered architecture (`primitives → blueprint → domain → store → engine → integrations/collectors`) to a local macOS system and storage instrument where OS-level filesystem scanning and Mach/sysctl telemetry are the primary ports.

**Who it is for:**
- **[KJ]** KJ on his personal Mac (and developers with similar macOS workstations accumulating build artifacts, caches, media, and containers).

---

## 2. Goals, Non-Goals, Anti-Goals

### Goals
1. **[KJ]** Show exactly which file types, folders, and projects are consuming disk space on macOS, with TMOG-style visual inspection (packed balls / treemap, file-type breakdown, largest folders, largest files, and scan history).
2. **[KJ]** Provide live CPU, memory, and process monitoring tabs alongside Disk Space in the same instrument.
3. **[call]** Detect developer project bloat (`target/`, `node_modules/`, `.next/`, `.venv/`, `.git/`, Xcode/Cargo/npm caches) as first-class grouped project roots so reclaiming tens of gigabytes takes seconds.
4. **[call]** Maintain strict Talvyn engineering quality: pure domain logic, explicit availability semantics, zero `unwrap`/`expect`/`panic`/`unsafe` in application crates, and audited command execution.

### Non-Goals (v1)
- **[call]** Windows (`Win32`) and Linux (`Qt 6` / `/proc`) collector implementations.
- **[call]** Kernel extension (`.kext`) or EndpointSecurity root entitlements requiring SIP changes.
- **[call]** TMOG's secondary hardware tabs in v1: Benchmarks, Drivers, Users, Services, and `.tmogtrace` binary file export (deferred to v2 add-ons).
- **[call]** Cloud sync, multi-device fleet dashboards, or LLM calls inside the monitor.

### Anti-Goals
1. **[call] Anti-Goal 1 — The monitor becomes the resource hog:** Samo must never spin up the Mac's fans or consume runaway RAM while idle or scanning in the background.
2. **[call] Anti-Goal 2 — Destructive cleanup accidents:** Samo must never permanently `unlink` user files directly from a cleanup command or act on a stale scan entry whose path now resolves elsewhere.
3. **[call] Anti-Goal 3 — Fabricated zeros or smoothed gaps:** Samo must never report `0` when a macOS sensor or path is unreadable/unsupported, and must never draw a continuous line across missed sample intervals.
4. **[call] Anti-Goal 4 — Recycled-PID collateral damage:** Samo must never terminate a process by raw `pid` alone without verifying that the target process's creation identity still matches.

---

## 3. Core Features

| # | Feature | Covers (v1) | Does not cover (v1) |
| --- | --- | --- | --- |
| **S1** | **Disk Space & Project Analyzer** **[KJ]** | Parallel directory scan of a selected volume or root path; interactive **Balls** (packed circles) and **Treemap** views; breakdown by **File Type** (Media, Documents, Code, Archives, Applications, System, Other), **Largest Folders**, **Dev Projects** (grouping project root + reclaimable artifact dirs like `target/`, `node_modules/`, `.venv/`), and **Largest Files** (top 100). | Unmounted disks, iCloud stub hydration, content-hash duplicate byte scanning across the entire drive. |
| **S2** | **Safe Cleanup & Scan History** **[call]** | Move selected folder/artifact/file to macOS Trash (`~/.Trash`) after verifying path + inode identity; open path in Finder; persist completed scan snapshots in local store and compare total/folder deltas across history. | Permanent `rm -rf` bypass of Trash, scheduled background auto-deletion. |
| **S3** | **Summary & Live Performance** **[KJ]** | Live **Summary** and **Performance** tabs polling at configurable interval (default 1000 ms): Apple Silicon / macOS CPU load (overall + P-core / E-core breakdown where available), Memory (pressure state, used, wired, compressed, cached, swap), Disk I/O rates, and Network interface throughput with explicit sample gaps. | GPU frame-time HUD, synthetic CPU/disk stress benchmarks. |
| **S4** | **Identity-Safe Live Processes** **[KJ]** | Live **Processes** tab with sortable/filterable table and parent-child hierarchy; columns for CPU %, Memory (resident/footprint), Disk R/W, Energy/Power impact state, user, and `(pid, start_time_secs)` identity; graceful Terminate (`SIGTERM`) and Force Quit (`SIGKILL`) verified against process identity; self-monitoring toggle to include/exclude Samo's own process. | Attaching a debugger, editing process environment variables, or modifying `nice` priority from the UI. |
| **S5** | **Installed Apps & Startup Items** **[call]** | Scan `/Applications` and `~/Applications` bundles with associated `~/Library/{Application Support,Caches,Logs,Containers}` sizes; list `LaunchAgents` and `LaunchDaemons` plist items with status and executable path. | Full uninstaller scripts requiring root privilege escalation or App Store receipt removal. |
| **S6** | **Instrument Console & Phosphor Themes** **[call]** | TMOG-inspired left navigation rail, bottom telemetry status bar (provider health, boot reason/uptime, process count, primary local IP, scan progress), and 6 console palettes: **Dark**, **Light**, **Green Phosphor**, **Amber Phosphor**, **Blue Phosphor**, and **Mono**. | User-uploaded CSS stylesheets or plugin widgets. |

### Platform Layer Underneath

- **[call]** **Single Command Registry (`samo-engine`):** Every state-changing or persisted command (`disk.scan.begin`, `disk.scan.finish`, `disk.scan.list`, `disk.scan.get`, `disk.cleanup.begin`, `disk.cleanup.finish`, `process.terminate.begin`, `process.terminate.finish`, `apps.scan.record`, `apps.scan.list`, `settings.get`, `settings.theme.set`) is declared in `samo-blueprint` and registered in one `CommandRegistry` in `samo-engine` with explicit input/output schemas and access metadata. Live read-only OS streams (`telemetry.snapshot`, `telemetry.series`, `process.list`) are served from the `samo-macos` collector ring buffer by `apps/samo-server`.
- **[call]** **Availability Envelope (`samo-primitives`):** Every metric field uses `Reading<T> = Available { value: T, sampled_at_ms: i64 } | Unsupported { reason: UnsupportedReason } | Stopped { since_ms: i64 }`.
- **[call]** **Audit & Event Outbox (`samo-store`):** Every state-changing command writes an immutable audit record and outbox event in the same transaction, with zero OS/network I/O inside `Engine::execute`.

### How Each Feature Behaves

#### S1 — Disk Space & Project Analyzer
1. User selects a volume or folder (default `/Users/<current_user>` or `/`) and clicks **Start** (`disk.scan.begin`).
2. `samo-engine` validates the root path, inserts a `scanning` session row in `samo-store`, and returns a `ScanId`. `apps/samo-server` then runs the bounded filesystem walker in `samo-macos` outside any DB transaction (capped worker threads so disk/CPU stays responsive).
3. As directories are traversed:
   - **APFS & Byte Accounting (`[call]`):** File size uses physical allocated bytes (`blocks * 512` on Unix metadata, falling back to logical `len()` only if block metadata is absent) so dataless iCloud stubs (`0` blocks) and sparse files do not inflate disk usage. Symlinks are never followed (`follow_links = false`), hardlinks with identical `(dev, ino)` within a scan are counted once, and scanning `/` skips the APFS firmlink mirror `/System/Volumes/Data` and `/dev` so user files are never double-counted.
   - **Atomic `.app` Bundles (`[call]`):** Any directory ending in `.app` is summed into a single atomic entry under the `Applications` category rather than listing its internal `Contents/MacOS` binaries in `Largest Files` or `Code`.
   - **File Type Categorization (`[call]`):** Non-bundle files are classified into the 7 TMOG categories (`Media`, `Documents`, `Code`, `Archives`, `Applications`, `System`, `Other`) via deterministic extension + path rules in `samo-domain`.
   - **Outermost Project Bloat Detection (`[call]`):** Project roots are identified when marker files (`Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, `.git`) are seen. When a project root is found, it becomes the outermost `ProjectSummary` for its subtree (absorbing nested workspace sub-packages so monorepos are never double-counted) and separates source bytes from reclaimable artifact directories (`target`, `node_modules`, `.venv`, `.next`, `dist`, `.cache`).
   - **Explicit Unreadable Accounting (`[call]`):** Unreadable paths (`EACCES` / TCC-protected folders) are counted under `Unsupported` / `unreadable_paths_count` rather than silently treated as 0-byte folders.
4. Upon completion (`disk.scan.finish`), the aggregated tree summary, top 100 largest files, top 50 largest folders, project bloat list, packed-circle layout nodes, and category totals are saved as a scan snapshot.
5. The UI renders the **Balls** (packed circles sized by bytes, colored by category or folder group) or **Treemap** view, the right-hand **Sample Disk / File Type / Largest Folders / History** panels, and the expandable **Largest Files** drawer.

#### S2 — Safe Cleanup & Scan History
1. From the Disk Space view, the user selects a folder, build artifact directory, or file and triggers **Move to Trash** (`disk.cleanup.begin` → OS Trash port → `disk.cleanup.finish`).
2. `samo-domain` checks `R2` (path is not on the protected system path denylist and is inside an allowed user/volume root) and verifies that the live filesystem `(device_id, inode)` matches the scanned entry (directory `mtime` is intentionally not required on folders because compiler/IDE watchers touch files inside `target/` or `node_modules/`).
3. Outside the DB transaction, the `samo-macos` Trash port moves the item to the user's `~/.Trash` using collision-safe naming (`<name>`, `<name> 2`, `<name> 3` so trashing `target/` from multiple Rust projects never collides with `EEXIST`), and never calls permanent `remove_dir_all` or `remove_file`.
4. `disk.cleanup.finish` records the reclaimed bytes and audit entry. The **History** panel lists prior scan snapshots with timestamp and used space so the user can click any past scan to compare delta bytes.

#### S3 — Summary & Live Performance
1. A background collector tick in `apps/samo-server` samples macOS host stats at the configured cadence (default `1000 ms`) into a bounded in-memory ring buffer (60 minutes of 1-second samples).
2. **Summary** shows a single-screen dashboard: CPU total + P/E core bars, Memory pressure + breakdown, Disk space + live R/W, Network up/down, and top 5 resource consumers.
3. **Performance** shows detailed time-series charts for CPU (Overall vs Per-Core P/E), Memory (Wired, Active, Compressed, Cached, Swap), Disk I/O, and Network interfaces.
4. If the Mac sleeps or sampling is paused, the ring buffer records a `Stopped` marker; the UI breaks the polyline and renders an explicit gap (`R3`).

#### S4 — Identity-Safe Live Processes
1. `process.list` returns live processes with `ProcessIdentity { pid: u32, start_time_secs: u64, executable_path: String }`, CPU %, memory footprint, disk R/W bytes/s, parent PID, user, and energy impact estimate.
2. When self-monitoring (`include_self`) is `false`, Samo's own PID is excluded from the top consumers table (while still reported in the status bar footer if requested).
3. When the user clicks **Terminate** or **Force Quit** (`process.terminate.begin` → OS signal port → `process.terminate.finish`), the command requires the full `ProcessIdentity`. Before sending `SIGTERM` or `SIGKILL`, the macOS port re-queries the OS for `pid`'s current `start_time_secs` and verifies it via `samo-domain::process::verify_identity`. If they differ, the action aborts with `ErrorCode::ProcessIdentityMismatch` (`R4`).

#### S5 — Installed Apps & Startup Items
1. `apps.scan.record` / `apps.scan.list` enumerates `/Applications` and `~/Applications`, reads `Info.plist` (`CFBundleIdentifier`, `CFBundleShortVersionString`), and matches `~/Library/Application Support/<bundle_id_or_name>`, `~/Library/Caches/<bundle_id>`, and `~/Library/Containers/<bundle_id>`.
2. Each app row shows **App Bundle Size**, **Support & Cache Size**, **Total Size**, and **Last Used**.
3. The **Startup Apps** section lists user and system `LaunchAgents` / `LaunchDaemons` with label, program path, run-at-load flag, and current running PID (if active).

#### S6 — Instrument Console & Phosphor Themes
1. The UI shell mirrors TMOG's layout: left sidebar (Summary, Performance, Processes, Startup Apps, Installed Apps, Disk Space), main viewport, right inspector rail, and bottom status bar (`Native providers healthy • <N> processes • IP <local_ip> • Generation <tick>`).
2. Clicking **Colors** in the bottom-left switches between `Dark`, `Light`, `Green`, `Amber`, `Blue`, and `Mono` phosphor palettes, persisted via `settings.theme.set`.

---

## 4. Rules That Protect the Anti-Goals

Every rule is enforced in `samo-domain` or `samo-engine` and verified by automated tests.

| Rule | Protects Against | Enforcement in Domain / Engine | Required Test |
| --- | --- | --- | --- |
| **R1 — Bounded Scanner & Ring Buffer Budget** | Anti-Goal 1 (Monitor hogging CPU/RAM) | `ScanConfig` enforces a hard cap on concurrent scanner threads (`max_threads <= 4`), max tracked individual files per snapshot (`top_files_limit = 500`), and ring buffer capacity (`max_samples = 3600`). Inputs exceeding caps are clamped or rejected in `samo-domain`. | `domain::tests::scanner_and_ring_buffer_caps_enforced` |
| **R2 — Trash-Only & Protected-Path Guard** | Anti-Goal 2 (Destructive cleanup accidents) | `samo-domain::cleanup::validate_trash_target` rejects system roots (`/`, `/System`, `/Library`, `/Applications`, `/bin`, `/sbin`, `/usr`, `/var`, `/private`, `/dev`, `/Users` root itself, and `~/.Trash`) and requires a matching `(device_id, inode)` fingerprint from a valid scan snapshot before the Trash port can be called. No permanent delete port exists in the codebase. | `domain::tests::protected_paths_and_inode_mismatch_rejected` + `xtask` purity check forbidding `std::fs::remove_dir_all` / `remove_file` in `samo-macos` cleanup module |
| **R3 — Explicit Availability & Honest Gaps** | Anti-Goal 3 (Fabricated zeros or smoothed gaps) | All metric values use `Reading<T>`. If `delta_ms > sample_interval_ms * 2`, `samo-domain::telemetry::append_sample` inserts an explicit `Reading::Stopped` gap entry before the new reading. `0` is only constructed when a provider returns `Ok(0)`. | `domain::tests::missed_intervals_insert_stopped_gap_and_never_zero_on_err` |
| **R4 — PID + Creation-Time Identity Verification** | Anti-Goal 4 (Recycled-PID collateral damage) | `process.terminate.begin` accepts `ProcessIdentity { pid, start_time_secs }`. `samo-domain::process::verify_identity(requested, live)` fails with `ProcessIdentityMismatch` if `(requested.pid, requested.start_time_secs) != (live.pid, live.start_time_secs)`. | `engine::tests::terminate_rejects_recycled_pid_with_different_start_time` |

---

## 5. How It Gets Made

### KJ's Decisions
- **[KJ]** Product name is **Samo** (`samo`).
- **[KJ]** Reference product and visual/functional benchmark is **TMOG** (`https://tmog.org/`), specifically unlocking the **Disk Space** experience (File Type breakdown, Balls/Treemap visual, Largest Folders, Largest Files, History) for personal use on macOS.
- **[KJ]** Also includes live **CPU, memory, and process** monitoring tabs like the rest of TMOG.
- **[KJ]** Built using the Talvyn engineering skills and discipline.

### Architecture Calls (`[call]`)

1. **[call] Layered Rust 2024 Workspace (`talvyn-architecture-baseline`):**
   ```text
   crates/
     samo-primitives/   # L0: IDs, timestamps, Reading<T>, ProcessIdentity, ByteSize, ErrorCode (pure)
     samo-blueprint/    # L1: Command specs, permissions, surface declarations, config limits (pure)
     samo-domain/       # L2: File categorization, project bloat detection, R1-R4 rules, circle-packing layout math, gap insertion (pure)
     samo-store/        # L3: Persistence for scan snapshots, history, settings, audit log, outbox (only sqlx user)
     samo-engine/       # L4: Command trait, CommandRegistry, Engine::execute pipeline with catch_unwind & audit logging
     samo-macos/        # L5: macOS collector & filesystem ports (sysctl/Mach/sysinfo metrics, parallel jwalk/fs scanner, Trash mover, PID identity lookup)
     samo-testkit/      # Fake macOS collector port, temporary directory fixture builder, test DB helpers
   apps/
     samo-server/       # Local Axum HTTP + WebSocket/SSE stream + static UI server (binds 127.0.0.1:7420 by default)
     samo-cli/          # CLI surface for triggering scans, viewing top bloat, and exporting JSON
   ui/                  # React + Vite + TypeScript frontend consuming ts-rs generated types
   xtask/               # cargo xtask verify gates: layers, purity, tests, rls, commands-complete, codegen
   ```
2. **[call] Baseline Deviation Recorded in ADR-001 (Storage Engine Mode):**
   - **What the baseline rule says:** `talvyn-architecture-baseline` specifies Postgres 15+ (17 in Docker) with forced RLS on every tenant table so one tenant cannot read another tenant's data.
   - **Why Samo deviates for personal Mac runtime:** Samo is a single-user personal macOS desktop instrument. Requiring Docker Desktop + a Postgres 17 container to be running just to check why your Mac's disk is full or why your fan spun up violates Anti-Goal 1 (Docker Desktop itself consumes several GB of RAM and tens of GB of disk space).
   - **How Samo still protects what the rule protects:**
     - `samo-store` remains **the only crate allowed to use `sqlx`**, enforced by `cargo xtask verify` (`layers` and `purity` gates).
     - Every table still leads with `tenant_id` (bound from the authenticated local `TenantId` context in `TenantTx`, never from command parameters) and keeps `TenantTx`'s inner transaction private so all queries remain scoped and auditable in one place.
     - `samo-store` uses `sqlx` with **embedded SQLite (`~/.samo/samo.db`, WAL mode)** by default for zero-dependency personal Mac startup, while preserving the exact `TenantTx` / `SystemTx` transaction boundary and `tenant_id` column discipline from the baseline.
3. **[call] Two-Phase Non-Transactional OS I/O Lifecycle (Following the Baseline Worked Example):**
   - Just like Talvyn AI Proxy's `completion.begin` / `completion.finish` pattern in `talvyn-architecture-baseline`, filesystem scanning, moving multi-gigabyte folders to Trash, and signaling OS processes are I/O operations that must not execute inside an open DB transaction (`L4 samo-engine` remains pure of `L5 samo-macos` dependencies):
     1. `*.begin` (`disk.scan.begin`, `disk.cleanup.begin`, `process.terminate.begin`) runs as a short engine command: validates domain rules (`R1`–`R4`), checks permissions, and inserts a `pending` / `scanning` row plus audit entry (Transaction 1).
     2. `apps/samo-server` invokes the `samo-macos::MacOsPort` implementation with no DB transaction open.
     3. `*.finish` (`disk.scan.finish`, `disk.cleanup.finish`, `process.terminate.finish`) records the completed snapshot, reclaimed bytes, or failure status (Transaction 2).
     4. On `samo-server` startup only, a reconcile sweep marks any leftover `scanning` or `pending` rows from a prior crash/quit as `interrupted`.
4. **[call] Surfaces:**
   - **Web / Local Desktop UI (`127.0.0.1:7420`):** React + Vite console styled after TMOG (`Disk Space`, `Summary`, `Performance`, `Processes`, `Installed Apps`, `Startup Apps`).
   - **CLI (`samo-cli`):** Thin HTTP client talking to `samo-server` (`127.0.0.1:7420`) when running, or running a one-shot read-only/scan flow without triggering the server's startup reconcile sweep.
   - **Internal Collector Loop:** Pushes live telemetry ticks into the server's ring buffer.
5. **[call] Quality Gates (`cargo xtask verify`):**
   - Workspace-wide deny lints: `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`, `unreachable`, `dbg_macro`, and `unsafe_code` (`samo-macos` uses safe crates `sysinfo` and safe macOS command/sysctl wrappers so `#![forbid(unsafe_code)]` holds across every crate in the workspace).
   - `xtask` verifies crate dependency layers (`L0 → L1 → L2 → L3 → L4 → L5`), purity of `primitives/blueprint/domain` (no `std::fs`, `std::net`, `std::process`, `sqlx`, `tokio`), `tests`, `rls` (verifying every non-allowlisted table in `crates/samo-store/migrations/*.sql` has `tenant_id TEXT NOT NULL`, indexes/uniques lead with `tenant_id`, and `TenantTx` keeps its inner transaction private), and `commands-complete` (every `CommandSpec` in `samo-blueprint` is registered in `samo-engine`).

---

## 6. Build Order

### Phase 1 — Foundation, Engine & Disk Space Instrument (S1 + S2)
- **Builds:**
  - Workspace scaffold, `rust-toolchain.toml`, lint rules, `xtask` gates (`layers`, `purity`, `tests`, `rls`, `commands-complete`), `samo-primitives`, `samo-blueprint`, `samo-domain` (file-type classifier, dev-project bloat detector, circle-packing layout calculator, `R1` & `R2`), `samo-store` (SQLite WAL migrations for scans, snapshots, audit log, settings), `samo-engine` (`CommandRegistry`, `Engine::execute` with `catch_unwind`), and `samo-macos` filesystem scanner + safe Trash mover.
- **Done when:**
  - `cargo xtask verify` passes all gates, and scanning a directory produces deterministic category totals, project bloat rankings, packed circle coordinates, largest folders/files, and safe Trash movement with inode verification in automated tests.

### Phase 2 — Live macOS Telemetry, Process Identity & End-to-End TMOG Console Demo (S3 + S4 + S6)
- **Builds:**
  - `samo-macos` live telemetry collector (CPU overall + P/E core awareness, Memory pressure/wired/compressed/swap, Disk I/O, Network throughput, and live process table with `(pid, start_time_secs)` verification), `R3` & `R4` enforcement, `apps/samo-server` (Axum API + SSE live stream), and the `ui/` React + Vite TMOG-style console (Disk Space Balls/Treemap view, Summary, Performance, Processes, and 6 phosphor color themes).
- **Done when (End-to-End Demo Pass Criteria):**
  1. Running `cargo xtask verify` and `npm --prefix ui run build` succeeds with zero warnings/errors.
  2. Starting `samo-server` and opening the UI on macOS shows live **Summary**, **Performance**, and **Processes** data with explicit availability states and phosphor theme switching.
  3. Clicking **Start** in **Disk Space** on a target folder renders the interactive **Balls** visualization, **File Type** breakdown (Media, Documents, Code, Archives, Applications, System, Other), **Largest Folders / Dev Projects**, **Largest Files**, and records the scan in **History**.
  4. Triggering cleanup on a test folder moves it to macOS Trash and logs an audit entry; triggering process termination with a mismatched `start_time_secs` is rejected by `R4`.

### Phase 3 — Installed Apps, Startup Items & CLI Polish (S5)
- **Builds:**
  - `apps.scan.record` / `apps.scan.list` in `samo-macos` + `samo-engine`, the **Installed Apps** and **Startup Apps** UI tabs, and `apps/samo-cli` commands.
- **Done when:**
  - Installed macOS `.app` bundles are listed alongside their `~/Library` support/cache footprint, LaunchAgents are inspectable in the UI, and `cargo xtask verify` passes end to end.

---

## 7. What Carries Over

| Concept | What It Becomes in Samo | Source Reference |
| --- | --- | --- |
| Layered crate boundaries (`L0`–`L5`) & `xtask` gates | `samo-primitives`, `samo-blueprint`, `samo-domain`, `samo-store`, `samo-engine`, `samo-macos`, `xtask` | `talvyn-crm-archetype` (`xtask/src/*.rs`, `ADR-001`) |
| Single `Command` registry & `Engine::execute` | All UI and CLI operations execute through `Engine::execute` with `catch_unwind` and automatic audit logging | `talvyn-crm-archetype` (`crates/crm-engine/src/{command,spec,registry,engine}.rs`) |
| 2-phase non-transactional I/O (`begin` / `finish` + reconcile) | `disk.scan.begin` → lock-free filesystem walk → `disk.scan.finish` + startup interrupted-scan sweep | `talvyn-architecture-baseline` (AI Proxy worked example) |
| Explicit sensor availability (`Available`, `Unsupported`, `Stopped`) & gap rendering | `Reading<T>` in `samo-primitives` and gap insertion in `samo-domain` (`R3`) | TMOG (`https://tmog.org/` facts #3 and #4) |
| Disk Space Balls view, File Type breakdown, Largest Folders/Files, History | `S1` & `S2` Disk Space screen and deterministic circle-packing layout in `samo-domain` | TMOG Disk Space screen reference |
| PID + Creation Identity pairing | `ProcessIdentity { pid, start_time_secs }` checked before any signal (`R4`) | TMOG fact #9 ("A PID is not enough") |
| Phosphor console palettes | `Dark`, `Light`, `Green`, `Amber`, `Blue`, `Mono` theme tokens (`S6`) | TMOG ("The phosphor belongs to the product") |

**What is left behind:**
- Multi-tenant cloud OAuth / cookie session brokers and external webhook outbox workers from `talvyn-crm-archetype`.
- Windows `Win32` and Linux `Qt 6` native UI toolkits from TMOG (Samo uses Rust + React/Vite local console).
- Paid license key activation banners ("Disk Space is a TMOG Pro feature" — unlocked natively in Samo).

---

## 8. Customisation per Machine

- **[call] Configuration (`~/.samo/config.toml` or env `SAMO_*`):**
  - `SAMO_BIND_ADDR` (default `127.0.0.1:7420`)
  - `SAMO_DB_PATH` (default `~/.samo/samo.db`)
  - `SAMO_DEFAULT_SCAN_ROOT` (default user's home directory `~`)
  - `SAMO_SAMPLE_INTERVAL_MS` (default `1000`, clamped `250..=10000`)
  - `SAMO_MAX_SCAN_THREADS` (default `4`, clamped `1..=4` per `R1`)
- **[call] Extensible Project Signatures (`samo-domain`):**
  - Project artifact detectors are data-driven (`Cargo.toml → target/`, `package.json → node_modules/,.next/,dist/`, `pyproject.toml → .venv/,__pycache__/`, `Podfile/xcodeproj → Pods/,DerivedData/`) so new project types can be added as pure table entries.
- **[call] Collector Port (`samo-macos::MacOsPort` trait):**
  - All OS interactions sit behind a trait port so `samo-testkit` can inject deterministic fake filesystems, fake Apple Silicon P/E core topologies, and simulated recycled PIDs in unit and integration tests.

---

## 9. Left to the Builder

- Exact SVG/Canvas rendering details of the packed-circle **Balls** animation and hover tooltips in the React UI, provided circle radii are proportional to `sqrt(bytes)` and colored by category.
- Choice of pure-Rust circle-packing algorithm in `samo-domain` (e.g., deterministic front-chain / spiral packing sorted by descending size).
- Internal SQLite index names and exact `sysinfo` / `libc` / `objc2` crate bindings inside `samo-macos`, as long as `unsafe` remains forbidden or isolated strictly to standard safe OS wrapper crates.

---

## 10. Open Questions

None blocking. The single baseline adaptation (embedded SQLite WAL via `sqlx` in `samo-store` vs. requiring Docker Postgres 17 for a personal Mac monitor) is documented as `[call]` #2 in Section 5 for KJ's review.
