# Devlog 0021 — Samo 0.6.3: Windows' Recycle Bin room and End task

- **Date:** 2026-11-22
- **Commit:** `7fc59a1` (`chore: improve Samo Windows trash and quit`) on `main`; this release is cut from `main`.
- **The Mac is unchanged (R25).** Every change is behind `cfg(windows)`, or uses `words::FORCE_QUIT` with `"Force quit"` preserved on the Mac.

## What was wrong
- On Windows, moving an item larger than a drive's Recycle Bin `MaxCapacity` (or on a drive with `NukeOnDelete` on) failed after clicking Move to Recycle Bin, rather than explaining and offering a safe fix up front.
- `taskkill` without `/F` exits 0 even when an app ignores the close request, so Samo reported "Asked … to close" and left no direct way to force-end it if it stayed open.

## Changes
- **Recycle Bin plan & make-room dialog (`crates/samo-windows/src/cleanup.rs`, `apps/samo-gpui/src/main.rs`, `apps/samo-gpui/src/shell.rs`, `docs/decisions/ADR-006-samo-on-windows.md`):**
  - `recycle_plan` checks the drive before the confirmation dialog opens (`NukeOnDelete`, `MaxCapacity`, and current bin usage via `bin_used_bytes`).
  - When the item won't fit or the bin is turned off, the dialog shows the item size, what the bin already holds (or falls back safely when the bin cannot be listed), and the limit Samo would set: `item + used`, rounded up to whole MB, plus 5% headroom (at least 64 MB).
  - Only after confirmation does `make_room` write `MaxCapacity` and clear `NukeOnDelete`, so Undo keeps working and nothing already in the bin is pushed out for good.
  - Unseen, removable, non-NTFS, or unreadable-GUID drives are refused directly in the dialog before the click.
- **Quit check & End task fallback (`crates/samo-windows/src/process.rs`, `apps/samo-gpui/src/main.rs`, `apps/samo-gpui/src/shell.rs`, `apps/samo-gpui/src/words.rs`):**
  - After a Windows Quit request, Samo checks `is_process_running` 2 seconds later.
  - If the verified process is still alive, the notice bar offers **End task** (`words::FORCE_QUIT`), which runs `taskkill /F` only after re-verifying R4's `(pid, start_time_secs)` identity.
  - Notice bar actions are now typed (`NoticeAction::UndoTrash`, `NoticeAction::ForceQuit`).

## Checked
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `XTASK_SKIP_TESTS=1 cargo run -p xtask -- verify`: all 6 static gates passed (`layers`, `purity`, `rls`, `commands-complete`, `network`, `swift`).
- `cargo test --workspace --no-fail-fast`: 563 passed, 1 failed, 4 ignored across 57 suites.
  - `samo-windows` (25 passed), `samo-gpui` (128 passed), `xtask` (42 passed), and all other suites passed with 0 failures.
  - The sole failure was `samo_extensions::windows::tests::the_window_server_can_be_read` (`SystemWindows::count` returned `None` because `EnumWindows` has no interactive desktop in the non-interactive automation runner; untouched by `0.6.3`).
- `cargo run -p xtask -- samo-package`: built `target/samo-dist/Samo-0.6.3-windows-x86_64.zip`.
- **GitHub CI:** skipped on this Windows-only release commit (`[skip ci]`).

## Not yet checked by hand (KJ)
- Moving an oversized item through the Windows "make room" dialog and restoring it with Undo.
- Quitting an app that ignores `taskkill` and using the **End task** notice action.
- Running `cargo test -p samo-extensions --lib windows::tests::the_window_server_can_be_read` in an interactive Windows desktop session.
