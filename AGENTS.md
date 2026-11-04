# AGENTS.md — Samo

## Architecture & Layer Order
Every crate obeys strict layering (`cargo run -p xtask -- verify`):
1. `L0 samo-primitives` (pure)
2. `L1 samo-blueprint` (pure)
3. `L2 samo-domain` (pure: R1–R4 rules, disk classification, project bloat, circle packing, telemetry gaps)
4. `L3 samo-store` (only `sqlx` user; `TenantTx` keeps inner transaction private)
5. `L4 samo-engine` (`CommandRegistry`, default-deny `AccessPolicy`, `Engine::execute` with `catch_unwind` and automatic audit log)
6. `L5 samo-macos` (macOS filesystem scanner, collision-safe `~/.Trash` mover, live telemetry & process collector) and `L5 samo-windows` (the same functions on Windows, ADR-006: scanner with `(volume serial, file index)` identity, Recycle Bin mover with Undo, `taskkill` after R4, telemetry); each is empty off its own OS, and apps take theirs as a target-specific dependency named `platform`
7. `apps/samo-server` & `apps/samo-cli`

## Golden Rules
- Never use `unwrap`, `expect`, `panic`, `todo`, `unimplemented`, `unreachable`, `dbg!`, or `unsafe`.
- Never put OS or network I/O inside `samo-domain` or inside an open `Engine::execute` database transaction; use the 2-phase `*.begin` / `*.finish` pattern.
- Never call `remove_dir_all`, `remove_file` or `purge_all` in `samo-macos` or `samo-windows` cleanup — all user cleanup moves items to `~/.Trash` (the Mac) or the Recycle Bin (Windows) after verifying `(device_id, inode)` (`R2`).
- Every sensor reading uses `Reading<T>` (`Available`, `Unsupported`, `Stopped`) (`R3`).
- Every process signal verifies `ProcessIdentity { pid, start_time_secs }` (`R4`).
- Run `cargo run -p xtask -- verify` before finishing any task.
- Extensions (ADR-002): helpers run out of process only, never loaded into Samo (R8). Only `samo-extensions` may use an HTTP client (`net.rs`, `ureq`), and only after a click (`UserAction`, R6). Unsigned code runs only from `SAMO_EXT_DEV_DIR` (R5). A helper's CPU and memory are measured only by `meter::Meter`, on a `sysinfo::System` nothing else refreshes (sharing one with the tree check showed 5–16 times the real CPU); one over a limit rests and starts again, and stays stopped the third time in an hour (`BackoffConfig::limits`).
- Store (ADR-003): the catalog's signature is checked before it is read, and every package, image and Samo zip against its SHA-256 in it; a package's manifest must equal its listing; an older catalog than one seen is refused (R11). `UserAction::clicked()` appears only in click handlers in `extensions_page.rs` and `settings_page.rs`; nothing fetches at launch or on a timer (R12). Caps and image headers are checked before anything is decoded; store work runs on the store worker thread (R13). A Samo update is checked before Samo quits, and the old app goes to the Trash and comes back if the swap fails (R14).
- Extensions with windows (ADR-004): an extension may open windows only if its manifest says `windows`, and is stopped otherwise; Samo checks every 2 s, on every Space, for the helper's whole process tree, and ends that tree when it stops (R15). Declared `[limits]` replace R8's, up to 1024 MB and one core (R17). Shots (Swift, `extensions/shots`) never goes online (R16, the `network` gate scans Swift), never crashes on purpose (the `swift` gate: no `try!`, `as!`, `fatalError`, `precondition`), reports shortcut clashes instead of forcing them (R18), and saves anything unsaved on shutdown (R19). A helper that asks macOS for a permission is packaged signed with the "Talvyn Samo Extensions" identity (`xtask/src/codesign.rs`), never ad hoc, so updates keep the grant; `ext-package` refuses otherwise.
- Clips (ADR-005): an extension may also ask for `paste-from-other-apps` and `accessibility`, and must say what it `watches` while idle; all shown before install (R20, R21). Clips reads one clipboard counter twice a second and the clipboard's kinds only when it changed; it reads contents only after the secret checks pass, so secrets never reach memory (R22). History stays in its `data/` (R23). ⌘V is pressed only right after a pick, into the app that was in front (R24). Shared Swift code lives in `extensions/kit` (SamoKit); a URL is made from text only in a file named `Links.swift`.
- Claude Sessions: every change to `~/.claude/settings.json` is declared, backed up and exactly undone (R7); the hook prints nothing, exits 0 and finishes within 500 ms (R9); unknown is never `0` and a dead session is never shown working (R10).
- Windows (ADR-006, in progress, #13):
  - **The Mac doesn't change (R25).**
    - Windows code sits behind `cfg(target_os = "windows")`, or is platform-neutral and tested on both.
    - A rule that differs by OS takes it as a parameter (`is_protected_path_on`, `scan_root_on`).
  - **One path shape (R26).**
    - Windows paths are kept as `C:/Users/kj` (`samo_domain::normalize_path`).
    - On Windows only a drive path is a scan root or trashable. On the Mac drive paths fail closed, as before.
  - **R2 on Windows (R27).**
    - Protects Windows' system paths in any case.
    - Also protects names Windows resolves elsewhere: `.`, `..`, trailing dot or space, `:stream`, `~1` short names.
    - Anything not in the one shape fails closed.
  - **Said, never faked (R28).** What Windows can't report is `Unsupported`, and a feature not built for Windows yet says so.
  - **No `unsafe` (R29).** Windows APIs go only through safe crates; a need with no safe wrapper goes to KJ.
  - **Gates on Windows (R30).**
    - Before finishing on Windows, run `scripts/ci-windows.sh` (with `RUSTUP_TOOLCHAIN=1.99.0-x86_64-pc-windows-msvc` if rustup's default host is GNU).
    - It runs fmt, clippy, the static gates and the tests of the whole workspace.
    - The macOS jobs still run everything, Swift included.
  - **The app on Windows (`samo-gpui`).**
    - A window that is never shown keeps Samo in the tray (GPUI quits on the last window close), and `instance.rs` keeps it to one Samo.
    - What each OS calls things lives in `words.rs`, and where things are lives in `places.rs`. Keep the Mac's strings byte-identical.
    - Roots go through `places::scan_root`, so a scan matches its root.
    - Shortcuts are `secondary-…` (⌘ on the Mac, Ctrl on Windows).
  - **Extensions on Windows (#9).**
    - A helper is `<helper>.exe` (`helper_file_on`) in `%LOCALAPPDATA%\Samo\Extensions`, and `current` is a junction.
    - Its family is its Job Object (`family.rs`); the parent walk is only the fallback without a job, and the Mac's
      walk is unchanged. R15 counts visible top-level windows (`winsafe`).
    - The host's tests use the `samo-fake-helper` stand-in on both OSes; never a `#!/bin/sh` script.
    - The store runs on Windows (`STORE_ON_THIS_OS`): a listing installs its `platforms["windows-x86_64"]` package, or
      is *Mac only*. Every listing keeps its Mac `package` (Samo 0.4.1 reads only that; `catalog_compat` checks it).
      Samo's own updates on Windows come with its zip (`SAMO_UPDATES_ON_THIS_OS`), and until then say so.
    - Claude Sessions writes its Windows entries with `settings::entries_on(true, …)`: hooks in Claude Code's exec
      form (`command` + `args`, no shell), and a status line both Git Bash and PowerShell run. The Mac's entries are
      byte-identical; Samo's entries are recognised by `OWN_MARK` or the helper's own path (`HELPER_MARK`).
  - **Clips on Windows (#11)** is Rust (`extensions/clips-windows`, same id `clips`), its core pure and tested on both
    OSes. Windows tells it of each copy (`clipboard-win`'s listener); a look opens the clipboard once for names,
    markers and contents, and a marker it can't read counts as secret (R22). Its list holds its shortcut with
    `RegisterHotKey` (refused, never forced: R18) and pastes with `SendInput` only after a pick (R24).
  - **A platform's own capabilities.** A listing's `overrides[platform]` holds what that platform's build declares
    where it differs from the Mac's; the store client hands out `Listing::on(this_platform())`, so the store shows
    and checks those. `ext-publish` writes them from the Windows manifest.
