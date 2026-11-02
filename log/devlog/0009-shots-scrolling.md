# Devlog 0009 — Shots, Phase 4: scrolling capture

- **Date:** 2026-11-02
- **Design:** ADR-004 S5 and Phase 4, the last part of ADR-004's scope, which KJ approved.
  - KJ, after Phase 3: "Done? if not just continue".
  - Plan, written and then built straight through: `docs/samo/2026-11-02-samo-shots-p4-plan.md`.
  - Branch: `feat/shots-scroll`, from `feat/shots-read`. One commit per task.
- **Samo:** unchanged. Shots 0.4.0.

## What it does
- **Start:**
  - **S** in the overlay (before a drag) chooses an area to scroll.
  - A *Scrolling capture* button on Samo's page does the same.
  - An optional shortcut, none by default.
- **While you scroll:**
  - On release, Shots watches just that area with a ScreenCaptureKit stream: up to 20 frames a second, in sRGB, only
    frames that changed, with Shots' own windows left out.
  - You scroll the app underneath yourself. A frame around the area and a preview growing at its side, with the
    height so far, never take a click.
- **Stitching:** `Stitcher` matches each frame against the last frame used, by row fingerprints (32 column sums a
  row), counting only rows with detail. Rows that stay put at the top and bottom (a sticky header or footer) are
  kept once. Small changes in place (a caret, a spinner) and scrolling back up are ignored.
- **How it ends:**
  - Esc, a global shortcut held only while it runs, or a click outside the area.
  - A frame with no overlap (the page jumped): Shots keeps what it had and says so.
  - The cap, 30,000 px or 400 MB of pixels.
  - macOS stopping the stream.
  - The picture is then a shot like any other: copied, saved and opened.

## Rulings
1. **Strips are copied out of their frames.** A cropped image keeps its whole frame alive, so at 20 frames a second
   the frames would be the memory, not the strips.
2. **The cap counts the body.** At the cap the sticky footer is left off, so the shot is exactly the cap tall.
3. **Off or quitting mid-scroll cancels it.** It isn't a shot until it ends, so nothing half-made is saved.
4. **S is the scrolling key,** as reserved since Phase 1; the overlay's hint and Samo's page name it.
5. **Test timing in release builds only.** Built for release, as Samo runs it, a 1,600 × 1,200 frame takes 3 ms to
   fingerprint and 1 ms to match. Unoptimised test builds are about fifty times slower, so the 50 ms budget is
   checked only in release.

## Verification
- **Stitching, in tests:**
  - A synthetic page three screens long stitches back pixel for pixel: unique detail in every row, a blank gap, a
    sticky header and footer, uneven scrolls, a pause and a scroll back.
  - So does one without sticky bands.
  - A jump stops it and keeps what it had; a blank page never repeats; the cap stops it at its height.
- **Local CI:** `scripts/ci-local.sh` passes: 247 Rust tests (all four Shots demos), 99 Swift tests, all seven gates.
- **`shots_phase4_demo`:** installs 0.4.0 from a test-key store under a temporary home.
  - The page has *Scrolling capture*, and its shortcut row says *No shortcut*.
  - Off leaves nothing running.
  - The demos share `shots_common`, which now allows unused helpers: each demo is its own test crate.
- **Store:** 0.4.0 packages signed with a throwaway identity in a throwaway keychain (515 KB), and a test-key catalog
  verifies against it. The throwaway package was deleted.
- **Live:** the stream needs Screen Recording for the new build, so the done-when check (M19) is KJ's: a real page
  three screens long, scrolled by hand.
