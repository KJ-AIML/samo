# Devlog 0018 — Processes: apps, icons, search and bars

- **Date:** 2026-11-07
- **Asked by:** KJ, with a screenshot of another app's Processes page: "the process display, I want it more detailed, or
  to show the app's icon, or easier to read".
- **KJ chose:** "icons + app groups + search + bars" (not the smaller "icons and bars", not the bigger one with a detail
  panel), and icons in Overview's Top processes too.
- **Branch:** `feat/appearance-themes` (the same one, after the Appearance work).

## What the page was
A flat table of the busiest 150 of ~740 processes: name, PID, CPU, memory, disk, energy, Quit on hover. No icons, no
search, no grouping; eleven rows called "Grok Bot Helper (Renderer)" and three "com.apple.WebKit.…" that you couldn't
tell apart.

## What it is
- **Apps.** A process's app is the outermost `.app` folder in its path (`samo-domain` `process_groups.rs`, pure, 9 tests),
  so a helper nested inside an app counts for the app. One row per app with the totals of all its processes; click to
  open it. A process in no app (a daemon, an XPC service) stays on its own.
- **Applications and Background.** The OS says which processes are Dock applications (`NSRunningApplication`, Regular
  policy), looked at every 2 s. Where it hasn't said (not yet, or Windows) there is one list and no headings.
- **Icons** (macOS) from `NSWorkspace` on a worker thread, never the UI thread; a gear for what is in no app and for an
  app the OS has no icon for; nothing on Windows.
- **Search** by name, path or PID. Typing opens every app so a match inside one is seen. No user search: the collector's
  `user` is a bare uid (`501`), which says nothing.
- **Bars** under CPU and Memory (a thin line, not a fill: a fill behind a right-aligned figure floated beside it).
  Scaled to the biggest on screen, with a floor (50 % of a core, 1 GiB) so a quiet Mac isn't all full bars.
- Overview's Top processes rows carry the same icons (a helper wears its app's).

## What went wrong on the way (and the fix)
- **Icons came out blank**, all but one. AppKit's `TIFFRepresentation` is a 16-bit *half-float* TIFF with premultiplied
  alpha for most icons ("sample format 3 for 16 bits"); `image` and gpui reject it, and gpui drew nothing without an
  error. Warp's, an 8-bit one, was the only icon that showed. Found by dumping the TIFFs and reading their tags.
- **The PNG route is `unsafe`.** `representationUsingType_properties` takes an untyped dictionary, so the bindings mark
  it unsafe (R29). Rather than work around that, `app_icons.rs` reads the one TIFF layout AppKit writes (uncompressed,
  4 samples, 8-bit or half-float, either byte order), un-premultiplies, clamps extended-range values, and writes an
  8-bit PNG with `image` (png feature only, already built by gpui). I prototyped it in Python first and looked at the
  Finder icon before writing the Rust.
- **The first bars were wrong:** drawn from the left of a cell whose figure is right-aligned, they read as stray blocks.
- **The collector kept 150 rows**, which would make an app's total wrong; it now carries every process (4096 cap).

## Rulings (mine, to be checked)
- **Quit is per process, never per app.** An app row has no Quit; open it and quit a process. One click can't end every
  helper of an app. (An app of one process shows Quit on its row.)
- **A gear, not a guess,** for background processes: the same glyph for every daemon. Windows shows no icon column.
- **Overview's card shows processes, not apps** (each helper on its own row with its app's icon). Grouping it by app
  would change what "Top processes" means; not asked for.
- **Not built:** the detail panel (identity, lifetime, memory, disk, network for the selected process) and the
  parent/child tree from the reference; thread count, page faults and GPU/NPU need collectors Samo doesn't have.

## Checked
- 123 `samo-gpui` tests (line layout, the TIFF reader on hand-built files, half floats) and 57 `samo-domain` ones; the
  real-AppKit icon test (`--ignored`) passes. Workspace clippy `-D warnings` clean.
- Looked at the page in dark Cobalt and light Graphite, and at Overview's card, from a debug build with an isolated
  `HOME`. Seven real apps showed their real icons.

## Not checked
- Typing in the search box, opening an app, and Quit in the real window (the logic is tested; the clicks were not).
- Light and dark with many apps; narrow windows (columns step away as before); hundreds of icons (the cache stops at 512).
- Windows: the page compiles to no icons and a flat list, but it can't be built on the Mac (`scripts/ci-windows.sh`).
- A throwaway test window was clicked into Light + Graphite by someone while I was capturing (the file I seeded
  changed 10 s into the run), which suggests the Mode and Style controls save and apply, but I did not do it.
