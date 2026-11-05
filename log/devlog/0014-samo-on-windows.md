# Devlog 0014 — Samo 0.5.0: Samo on Windows, its extensions, its store and Clips

- **Date:** 2026-11-05
- **Asked by:** KJ: Samo works well on the Mac, so it should run on Windows too, all of it ("All of APP… I want it
  all", then "make all phase done"), planned as GitHub issues and built as PRs. Then "merge and release".
- **Branches:** the stacked PRs #14, #15, #17, #18–#24, merged into `main` in order; this release from `main`.

## What's new
- **Samo on Windows** (#14, #15, #17; ADR-006):
  - `samo-windows` scans, measures and cleans as `samo-macos` does. Cleaning goes to the Recycle Bin, with Undo, and
    R2 protects Windows' own paths (R27).
  - The CLI, the web console and the app (GPUI) run on Windows, with a tray icon and one Samo at a time.
- **Extensions on Windows** (#9: #18, #19):
  - A helper's whole family lives in a Job Object, so it can't outlive Samo, and R15 counts its visible windows.
  - Claude Sessions connects to Claude Code on Windows, with hooks in Claude Code's exec form and a status line both
    Git Bash and PowerShell run.
- **The store and Samo's own updates on Windows** (#10: #20, #21):
  - One catalog, still schema 1. A listing carries Windows' package beside the Mac's, and Samo 0.4.1 still reads it
    (`catalog_compat` runs 0.4.1's own reader).
  - Samo for Windows is a zip (`xtask samo-package`), published beside the Mac's. **Settings → This PC** installs it
    for this user, and **Settings → Updates** updates it the way the Mac does (R14).
- **Clips for Windows** (#11: #22, #23), in Rust (`extensions/clips-windows`, the same id `clips`):
  - Windows tells Clips of each copy, and Clips sleeps between copies. Secrets are never kept (R22): a look opens the
    clipboard once for names, markers and contents, and a marker it can't read counts as secret.
  - The list at Win+Alt+V pastes into the app you were in, only right after a pick, and only once Alt, Shift and
    Win are let go (R24). A shortcut another app holds is refused, never forced (R18).
  - A listing can now carry a platform's own capabilities (`overrides`), so the store shows and checks Windows' own.
- **Shots for Windows has begun** (#12: #24): its core is in, ported from ShotsCore with its tests. It isn't packaged
  or listed in 0.5.0: capture, the overlay and the editor come next.
- **Claude Sessions 0.2.3:** the Mac's is unchanged in use. Its code now writes Windows' entries too.

## Checked
- **On Windows 11 (MSVC):**
  - `scripts/ci-windows.sh` passes on `main`: fmt, clippy `-D warnings`, the static gates and the workspace's tests
    (491 at #24).
  - Clips' real-clipboard tests and its demo under Samo's real host pass with `SAMO_TEST_CLIPBOARD=1`.
- **GitHub CI:** every PR passed fmt, clippy and the tests on the Mac and on Windows before it merged.
- **Reviews:** a fresh review read every PR before it merged. Their important findings were fixed with tests:
  - the update could have recycled a shared folder;
  - a secret marker could fail open;
  - plain paste sent Ctrl+Alt+V;
  - a QR link could show one host and open another.

## Not yet checked by hand (KJ, before publishing)
- Installing Samo for this user, and a real update from one Windows build to the next (#21).
- Claude Sessions' Connect on Windows (#19).
- Clips' list into Notepad and Word, Alt+Enter, Thai in its search, and an administrator window (#22, #23).

## Found on the way
- **GitHub had no macOS arm64 runners** for a while: three `check` jobs failed with "not acquired by Runner" and
  passed on a later run.
- **Each git worktree built its own `target`** (about 5 GB each) and filled drive D:. Worktrees now share one
  `CARGO_TARGET_DIR`.
- **`history.log` lines were written in pieces** (#26). `main`'s Windows CI for #23's merge read a half-written
  `uninstall` line. #27 made the test wait for the line; after 0.5.1, each line also goes out in one write, so
  lines appended at once never interleave.
