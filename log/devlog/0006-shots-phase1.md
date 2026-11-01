# Devlog 0006 — Shots, Phase 1: extensions with windows, capture and pins

- **Date:** 2026-11-01
- **Design:** ADR-004, accepted by KJ ("Approved"), with KJ's answers: Screen Recording belongs to Shots alone;
  Shottr's usual keys (⇧⌘1, ⇧⌘2). Plan: `docs/samo/2026-11-01-samo-shots-p1-plan.md`, one commit per task.
- **Samo's side (0.3.0):**
  - Manifests may declare `windows`, `permissions`, `min_macos` and `[limits]`. The catalog carries them, an install
    refuses a package that differs, and the store says them before Install.
  - Protocol 1 gains row buttons, a `choice` widget, and `capturing`/`captured`. While an extension that declares
    windows captures, Samo hides itself, and never for more than a minute.
  - R15: every 2 s, the window server is asked, in safe Rust through Foundation's checked casts, whether the helper's
    process tree owns any window. An undeclared one stops the helper. Helpers run in their own process group, and the
    tree is ended when the helper stops.
  - `xtask` packages Swift extensions, scans Swift for network use, and rejects `try!`, `as!`, `fatalError` and
    `precondition` (gate 6 of 7). The tests gate builds and tests every Swift extension.
- **Shots (0.1.0):**
  - A Swift package: `ShotsCore`, everything testable without a screen, and `samo-shots`.
  - `samo-shots serve` starts the same binary as the worker, using macOS's responsibility-disclaim spawn attribute
    and its own built-in Info.plist (`dev.talvyn.samo.shots`), so Screen Recording is asked for Shots. Without the
    attribute it says so.
  - Capture freezes every display through ScreenCaptureKit. An overlay on each shows the frozen image with a
    crosshair, an 8× magnifier and the size in pixels; window capture is live, with its shadow.
  - Copy (this Mac only) and save happen before any window opens. Pins float above every window and Space.
  - Unsaved shots are saved as `… (unsaved).png` when Shots stops.
  - The page in Samo shows permission, capture, recent shots, shortcuts and settings; its own window chooses the
    folder and records shortcuts.
- **Hello 0.3.0** shows a choice and a row button; its `min_samo` is 0.3.0.
- **Verification:** `scripts/ci-local.sh`: Rust and Swift tests, all seven gates. Checks by hand (M1–M7) need KJ to
  allow Screen Recording.
