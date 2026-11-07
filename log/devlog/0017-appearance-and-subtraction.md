# Devlog 0017 — Appearance (mode and style) and a pass of subtraction

- **Date:** 2026-11-07
- **Asked by:** KJ, "I just want to update new setting to my app for like dark/light theme, for a setting pages, or
  other theme style of app is that possible ? or improve cause thise color is good but there still look like ai slop".
- **KJ chose:** "Appearance + a second style" (not accent colours), and "Subtract first" (not surgical only, not a
  re-layout) from the options I gave after auditing the running app.
- **Branch:** `feat/appearance-themes` from `main` (`dc9003c`). Not pushed.

## What I saw
I looked at the installed 0.5.1 (dark Overview, real data) and at a throwaway copy in an isolated `HOME` (light
Settings; later all four palettes), capturing only Samo's own window. The colours were not the problem. These were:

| Pattern | Where |
| --- | --- |
| The same bordered card for everything, inside a bordered stage | Overview; Settings was one card holding a headline and "Samo 0.5.1" |
| Three layers doing one job (lift a block off the page) | blurred shadow under every card and the readout, a dotted grid across the stage, and a hairline ring |
| A label restating its neighbour | "UPDATES" above a card titled "Samo hasn't checked for updates yet"; a "SAMO" group label over one item |
| The same fact drawn twice or more | process count in the nav badge, the probe's line and "All 685 processes"; free disk in the readout and the sidebar (words and a bar) |
| Clipped names beside room that was spent on figures | Top processes: three WebKit rows all read "com.apple.We…" |
| A fill under a line that says nothing the line doesn't | Activity chart |

What is Samo's own and stays: the dot-matrix readout, the probe, the rings, dither = "Samo suggests".

## Changes (4 commits and this one)
- **`appearance.rs`, `theme.rs`.** A mode (System, Light, Dark) and a style (Cobalt, Graphite), kept in
  `appearance.txt` beside `locations.txt` (written beside, then renamed; anything unreadable is the defaults).
  `theme::follow(window.appearance())` replaces three copies of the same expression (window, menu bar panel, pop-up),
  so one place decides: `SAMO_THEME` pin, then your mode, then the system. `SAMO_STYLE` pins a style the same way.
- **Styles are a `Tone`.** `light()` and `dark()` take the neutral they mix from. Cobalt is Ink and Pale cobalt, byte
  for byte what shipped (a test says so). Graphite is only black and white mixed (a test checks equal r, g, b).
- **Settings.** An Appearance group (Mode as a segmented switch; Style as tiles that draw a small Samo in each style,
  in the light or dark in use, with a Cobalt ring and a check on the one chosen), then Updates. Settings is a list
  now: a heading, then rows split by hairlines. Check now moved from the page head into the Updates row.
- **Subtraction.** No shadow under cards or the readout (only what floats keeps one: the folder menu, the dialog,
  tooltips, the chart's note); no stage grid; no stage ring, and none on the sidebar's Samo cell. Top processes
  shows the one figure the switch picked. The process count lives in the probe's line only. The sidebar's disk bar and
  its "SAMO" label are gone. Line charts draw lines only. `shade_2` and `shadow_card()` were deleted with their last user.

## Rulings (mine, to be checked)
- **Graphite first looked like Cobalt.** As "the same lightness with the blue taken out" the four palettes were
  indistinguishable side by side, so picking Graphite would have looked like a broken button. It now has real depth: in
  light a grey canvas (stage and shell at 2.4x Cobalt's distance from white) under white cards; in dark a charcoal.
  The contrast test then failed once (Graphite dark Signal text on its wash, 4.49:1); the fix is a knob on the tone
  (more white in the orange), not a looser test.
- **Warm "Paper" was not built.** No warm hue exists among the nine colours; orange mixed into white goes pink.
- **Accent colour was not built** (KJ didn't pick it): Cobalt is "selected" and "Samo suggests", orange is "wrong".
- **The stage keeps its fill and rounding**; only its ring went. In Cobalt light its edge is faint (white frame,
  #F7F7F8 stage) but legible with the cards' rings beside it.
- **The sidebar keeps its Disk row** (name and "103.1 GB free of 460.4 GB"), without the bar. The Disk is one of
  Samo's four characters (DESIGN.md §3); removing the row retires it. KJ may want that, which is the one repeat left.
- **Kept on purpose:** the page head's breadcrumbs (they navigate: click a parent to measure it), the
  "THIS MAC · LIVE" rule (it divides a long page), and IN / OUT / READ / WRITE (they name the lines' colours).
- **Storage's map keeps its grid** (its floor). I haven't looked at Storage, Projects, Processes or Performance.

## Checked
- `cargo test -p samo-gpui` 105 pass (14 theme tests, 5 appearance). The contrast helper is itself tested (black on
  white is 21:1), so the 4.5:1 check can fail. `cargo clippy -p samo-gpui --all-targets -- -D warnings` clean.
- `scripts/ci-local.sh` passes: fmt, workspace clippy with `-D warnings`, and `xtask verify` (its static gates and
  the whole workspace's tests). No gate file under `xtask/` was touched.
- Looked at: Overview in all four palettes, Settings in light Cobalt and dark Graphite, from a debug build run with
  an isolated `HOME` and a seeded `appearance.txt` (which also exercises the load path).

## Not checked
- **Clicking** a Mode segment or a Style tile in the real window (a synthetic click could land on another app).
  The code path is `choose_appearance`: set the atomics, save, `refresh_windows`, notify.
- The menu bar panel and a pop-up in Graphite or in a mode that differs from the system.
- The Windows branch of Settings (the install row is now a plain row): it can't be built on the Mac.
- Shots and Clips (Swift) windows still follow macOS's appearance, not Samo's choice.
- Pages other than Overview and Settings; narrow windows.
- README's screenshots (`docs/media`) show the old look with shadows and the grid. Not replaced: they are public.

## Follow-up, same day: the readout stays the same
KJ, testing the build: "this main metric should follow me to all pages, and should not changes colors?" (two arrows
at the Memory and Disk free cells).

- **It already follows.** Every page is drawn by one `render_stage`, which puts the readout above the scrolling
  page; the menu bar panel uses the same glass. (One of my own captures happened to be scrolled: the readout stayed
  at the top while the cards moved under it.)
- **It did change colour,** three ways: light vs dark (slate about `#1A1E25` in light, near-black in dark, from
  before this work), Cobalt vs Graphite (my addition), and Signal orange for a wrong figure (kept on purpose).
- **Then one face (superseded below).** `glass_face()` gave every palette the same glass, line, text and phosphor (the old dark
  values), so light mode's readout is darker than it was. A test checks all four palettes are equal and darker than
  their stage; I also sampled the rendered pixels in a capture of each palette: `(6, 8, 9)` at three points in all four.
- The Graphite-is-neutral test no longer lists the readout's glass, on purpose.

## Follow-up 2, same day: no, it should follow the theme
I had read "should not changes colors" as "keep the readout fixed" and made it one near-black in all four palettes.
KJ, looking at it in light mode: "why is it still black, the top panel? doesn't it change with the theme?" That was
the opposite of what I built. `glass_face()` is gone.

- **Dark:** near-black glass, white labels, pale-cobalt figures, Signal for a wrong figure (as before this work).
- **Light:** a recessed panel at the shell's tone (so a little darker than the stage), ink labels, Cobalt
  dot-matrix figures. A wrong figure uses a new token, `glass_signal` (a deeper orange in light), because plain
  Signal is too faint on a light panel; the readout, its lamps and its meters all use it.
- Tests: the readout is lighter in light than in dark in both styles, always darker than its stage, a wrong figure
  is at least 3:1 on it, and its text and figures stay at 4.5:1 (the existing palette test). Graphite's neutrality
  test lists the glass again, because it follows the style again.
- This reverses the design line "the readout is dark in both themes" (DESIGN.md §1, 2026-10-31). It was KJ's call.

## Follow-up 3, same day: the light readout was hard to read, and a % after Memory and Disk
KJ, with arrows on the figures of Memory, Disk free and Reclaimable: the numbers on the light panel are hard to read,
and "a small % after Memory and Disk, what do you think?"

- **Why it was hard.** The unlit dots of the dot-matrix figures used the same colour as a meter's empty track (14 % of
  Cobalt). On near-black they vanish; on the light panel they were 1.26:1 against it, nearly as visible as the lit
  dots, so "97.3" read as "97.8". They now have their own token, `phosphor_ghost` (5 % of deep Cobalt in light; the
  meters' track is unchanged). A test keeps the ghost under 1.2:1 in every palette, and I checked that it fails on the
  old value.
- **The %.** Memory shows the share used (what the Activity chart's Memory says) and Disk free the share free (the
  figure is free GB). Because "18%" beside a bar of 82 % lit would contradict it, Disk's bar now shows the free share
  too: one cell, one quantity (DESIGN.md rule 16). It steps away with the side labels below 190 px a cell, and the
  figures leave room for it. One line to revert if the old bar is wanted.
- Reclaimable has no share (not asked for).
