# Devlog 0007 — Shots, Phase 2: the editor

- **Date:** 2026-11-02
- **Design:** ADR-004 S3 and Phase 2. KJ's answer on 2026-11-02: every shot opens in the editor, like Shottr. Plan:
  `docs/samo/2026-11-02-samo-shots-p2-plan.md`, approved by KJ ("approved"), one commit per task, on
  `feat/shots-editor` from `feat/shots`.
- **Samo:** unchanged. The editor is Shots alone; Samo stays 0.3.0.

## What Shots 0.2.0 does
- **After a capture:** two choices, *First* (Copy · Save · Both) and *Then* (Edit · Pin · Nothing). The default is
  Both, then Edit. Copy and save still happen before any window opens (R19), and the editor opens even if the save
  failed, so the shot is never only in a failed file.
- **The editor:**
  - Thirteen tools, each with a key: select V, arrow A, line L, rectangle R, ellipse O, text T, highlighter H, pen P,
    numbered step N, blur B, pixelate X, spotlight S, crop C. A backdrop is a switch.
  - Seven swatches and the colour panel; S, M, L widths (`[` `]`).
  - Undo and redo of whole canvases: marks only, never pixels, capped at 500.
  - Copy (⌘C), Save over the shot's own file (⌘S), Save As (⇧⌘S), Pin (⌘P), copy and close (⌘↩).
  - A thumbnail at the foot drags the shot out as a file.
  - Closing with changes not copied or saved asks: Save and close, Discard, Cancel.
- **Rendering:** one renderer draws the canvas and every export, so what is copied is what was on screen.
  - The shot is a layer's contents, so only the marks redraw.
  - Blur and pixelate are made from the shot's own pixels: black-and-white stripes come out flat.
- **The other ways in:** a pin's double-click, and Edit on each recent shot in Samo (its fourth button, the protocol's
  most).
- **Lifetime:**
  - One editor per shot. It keeps its shot alive while open and hands its unsaved edits to shutdown, which saves them
    beside the shot as `… (unsaved).png`, never over it.
  - `data/drag/` is emptied at start and stop.
  - When the last editor closes, the app you came from gets the keyboard back.

## Rulings made during the build
1. **Focus,** checked on this Mac before building on it.
   - Refused: macOS 14's `NSApp.activate()`, when another app is in front.
   - Never became key: a non-activating panel.
   - Works: `activate(ignoringOtherApps:)`, reached through a protocol so its deprecation doesn't warn. Checked live:
     the editor opened in front of Warp, with the keyboard.
2. **Saved shots carry their density** (144 dpi at Retina), as macOS's own screenshots do. Without it, a shot saved at
   1x and reopened would be halved again. Files without one (0.1.0's) are read at the screen's scale.
3. **The highlighter is translucent, not multiplied,** so the canvas and the export are drawn the same way.
4. **Crop:** before there is a crop, a drag inside the shot draws a new box; moving a box the size of the whole shot
   does nothing.
5. **Text marks:** a click while typing only ends the typing. Return or Esc ends it; ⇧↩ or ⌥↩ starts a new line.
6. **A drag that lands somewhere counts as out:** closing afterwards doesn't ask.
7. **The minimum width is 868 pt.** A live check showed the toolbar overlapping at 760, and a test now guards it.
8. **Views don't clip by default since macOS 14,** so the backdrop's gradient filled the whole window until the
   ground and marks views clipped to their bounds.
9. **The worker has an invisible Edit menu,** so ⌘C, ⌘V, ⌘A and ⌘Z work while typing a text mark. It has no Quit.
10. **Settings load field by field,** so one value Shots doesn't know never resets the rest.
11. **A capture macOS never answers ends after 8 s.** This was found while scripting M7. Before, the worker stayed
    busy and ignored every shortcut until it restarted. Now a `Watchdog` gives up with a note, and ignores a late
    answer.

## Found along the way
- **Phase 1:** `~` in the folder came from the account, not `$HOME`, so test runs saved into the real
  `~/Pictures/Shots`. Fixed: `Settings.home`, `expand`, `abbreviate`.
- **Phase 1:** the clipboard and the file each encoded the PNG; now once.
- **Signing, fixed after KJ's sign-off:**
  - The problem: Shots was signed ad hoc, so macOS knew it by its binary hash (`designated => cdhash …`). Every new
    build, and so every store update, was a new app to Screen Recording and asked again.
  - KJ approved the fix in `xtask`, a gate file: `ext-package` now signs a helper that asks for a permission with
    the self-signed "Talvyn Samo Extensions" identity, and refuses the package otherwise. The helper's requirement
    becomes `identifier "dev.talvyn.samo.shots" and certificate leaf = H"…"`.
  - KJ makes the identity once with `ext-codesign-setup`.
  - Checked in a throwaway keychain:
    - two different builds share one requirement;
    - the later build satisfies the earlier one's, which is macOS's check against a stored grant;
    - a packaged Shots verifies and runs.
  - `two_builds_share_one_requirement` repeats this in the tests gate. The first signed Shots needs Screen
    Recording allowed once more; updates after it keep it.

## Verification
- **Local CI:** `scripts/ci-local.sh` passes: fmt, clippy, all seven gates, 241 Rust tests and 66 Swift tests.
  - The Swift tests drive the editor window headless, with real mouse and key events; it is never shown on screen.
- **Phase 2 demo:** `shots_phase2_demo` installs 0.2.0 from a test-key store under a temporary home.
  - Phase 1's settings load as Both, then Edit.
  - The temporary home's shot shows in Recent with Copy, Reveal, Pin and Edit.
  - Edit opens the editor, and R15 leaves it be.
  - Off leaves nothing running and no `data/drag/`.
  - The Phase 1 demo still passes; its choice step now sets `then: pin`.
- **Store:** `ext-package` and `ext-publish --test-key` give a catalog that verifies against the 0.2.0 package
  (416 KB).
- **Live, on KJ's Mac:**
  - The editor opened from Edit, in front, with the keyboard, and drew correctly in the dark theme.
  - This check found the toolbar overlapping at 760 pt.
- **M13, by script** (20 editors on 3024 × 1964 shots, opened over the protocol, each closed by ⌘W sent to Shots alone):
  - Memory: 87 MB after the first editor, 89 MB after twenty (physical footprint 20 → 21 MB). The limit is 50 MB.
  - Speed: each editor was on screen 112–148 ms after the request, decoding included.
- **M7, by script: not done.**
  - A second Shots process, beside the test Samo's, asked for the screen 50 times; ScreenCaptureKit never answered
    the first.
  - That led to rule 11. The test Samo's own Shots is idle at 27 MB after KJ's four shots, down from 60 MB.
- **By KJ on 2026-11-02, with 0.4.0:** capture opening the editor, the tools with undo and redo, ⌘S, dragging out,
  and closing with changes. KJ: "All good". These were the items left:
  - Capture with 0.2.0: M8, M10–M12, and the drawing feel in M9. These need Screen Recording allowed for the new
    build.
  - Phase 1's M3, M5 and M6.
