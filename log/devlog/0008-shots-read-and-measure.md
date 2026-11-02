# Devlog 0008 — Shots, Phase 3: read and measure

- **Date:** 2026-11-02
- **Design:** ADR-004 S4 and Phase 3. Plan: `docs/samo/2026-11-02-samo-shots-p3-plan.md`, approved by KJ ("Approved"),
  one commit per task, on `feat/shots-read` from `feat/shots-editor` (Phase 2 and the signing fix).
- **Samo:** unchanged. Shots 0.3.0.

## What Shots 0.3.0 does
- **In the overlay, after ⇧⌘2 and before a drag:**
  - **T copies the text in an area.** It is read on the Mac by Vision's accurate mode: Thai and English first, then
    the Mac's other languages Vision reads. A note shows the first line, or *No text found*. If the first reading
    takes a while, a *Reading text…* note says so after 400 ms.
  - **C copies the colour under a click** as `#RRGGBB`, or `rgb()` with ⇧. The magnifier shows the hex with a swatch.
  - **R measures.** Lines run to the nearest edges, labelled in px, and pt on Retina. A drag measures a box. Each
    measurement is copied, and the ruler stays until Esc.
  - Pressing a key again goes back to capturing. Keys never change the mode mid-drag; S says scrolling comes later.
- **QR codes:** found in a T selection and in the editor. They show on a card on the readout's glass with Copy, plus
  Open for `http` and `https` links only. The card leaves on Esc, on a click anywhere else, or after 8 s, unless the
  pointer is on it.
- **In the editor:**
  - **M** measures: a view, never a mark, so it isn't exported and isn't undone.
  - The footer shows the colour under the pointer; **Tab** copies it.
  - **⇧⌘C** (or *Copy text*) copies the text of the kept part, without marks.
  - A QR code found in the background shows as a chip in the footer.
- **In Samo:** buttons for *Copy text from an area*, *Pick a colour* and *Measure*. Each can have its own shortcut,
  set in Shots' settings window; none is set by default.
- **Nothing saved:** reading, picking and measuring never save a shot or add to Recent.

## Rulings made during the build
1. **The card before the overlay:** the result card (Task 4) was built before the overlay modes (Task 3) that use it;
   each kept its own commit.
2. **The ruler shows pixels and points on Retina** (`32 px · 16 pt`). The ADR says pixels; M17's check is against CSS,
   whose pixels are points.
3. **Colour and ruler show the screen undimmed,** because they need its true colours. Colours are converted from the
   display's space to sRGB: a P3 capture of `#F15B2A` reads `#F15B2A`.
4. **An edge is a change of more than 16 (out of 255) on any channel.** Anti-aliased edges stop the ruler; a
   screenshot's faint noise doesn't.
5. **Small selections are read at twice the size,** which Vision reads better.
6. **The clipboard:** it takes text too (`put(text:)`). The real one moved to `ShotsCore`, beside `FileDisk`.
7. **Shortcuts:** they name letters (⇧⌘T, not "key 17"), and Delete clears one in the recorder, since the new ones
   start empty.
8. **What the editor reads:** its colour is the shot's own pixel, not a mark's, and its text is the kept part,
   without marks. It reads through its hooks, so its tests stand in for Vision.

## Verification
- See Task 8 below.
