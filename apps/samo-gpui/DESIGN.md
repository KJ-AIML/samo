# Samo — design

Samo is a Talvyn product, so it belongs to the family described in
`talvyn-ws/company/brand/product-ui-mesh-cells.md`: the nine colours, the faces, a calm frame around a stage, one
Ink action per view, Signal orange only for what is wrong or can't be undone. Within that family Samo has its own
identity, because it does a different job: **Samo is an instrument that measures your Mac.**

## 1. The idea

A calm frame with an instrument's face.

- **The readout is the face.** Pinned at the top of every page is a dark instrument window with phosphor
  dot-matrix figures: CPU, memory, disk free, and what Samo suggests reclaiming. It follows the theme: near-black
  glass with pale-cobalt figures in dark, a light recessed panel with Cobalt figures in light (KJ, 2026-11-07: a black
  slab in a light window looked wrong). It is the same on every page. A wrong figure is Signal (`glass_signal`).
- **The ring is the shape.** Disks are platters and gauges are dials, so Samo's shapes are rings: the gauge mark,
  the dial each page's icon sits on, the disk ring on Overview, the circle map on Storage. (Mesh uses hexagons.)
- **Samo measures; you decide.** What Samo *suggests* (build artifacts it thinks are safe to clear) is drawn in
  dither — a checkerboard of Cobalt pixels. What *you* decide is solid. Nothing moves until you say so.
- **The probe tells the truth.** Samo is a small round probe whose face shows only what its scanner is really doing.

## 2. The rule

| Is | Drawn as | Where |
| --- | --- | --- |
| A measured fact | Solid (Ink, grey, Cobalt for "this folder") | Sizes, rings, meters, tables |
| Samo's suggestion | Dither (2 px Cobalt checker) — on a mark, never as a card's border | The swatch by "Samo suggests", artifact chips, the suggested arc of the disk ring, a suggested circle on the map, the reclaimable lamp in the readout |
| Your decision | Solid, and yours (your cell beside the question) | The Trash dialog |
| Wrong / can't be undone | Signal orange, never filled | Errors, "Quit" a process, a disk ≥ 95% full, critical memory pressure |

Moving to the Trash can be undone, so it is not Signal: the dialog's "Move to Trash" is the view's Ink action.

## 3. The characters (`src/art.rs`, 16 x 18 pixel grid)

| Cell | Silhouette | States (always real) |
| --- | --- | --- |
| **Samo** (Cobalt) | Round probe: dithered crown, a readout face, a probe antenna with a lamp, side knobs, two feet | `Working` — a scan bar sweeps the face, lamp blinks (a scan is running) · `Idle` — two dots · `Blocked` — X eyes in Signal (the scan failed) · `Asleep` — flat line, dimmed (sensors not ready or stopped) |
| **Mascot** (Ink) | The same probe in Ink, nobody's colour | Empty places (asleep), errors (blocked) |
| **Disk** (grey) | A platter with a track and hub; a lamp | Lamp lit; Signal when nearly full; out when not measured |
| **You** (your hue) | A rounded square, solid, upright-bar eyes | Idle. Your hue comes from your user name (Mesh's hash, six equal-lightness hues). |

Cells move in whole-pixel steps, only while Samo is scanning (see §10).

## 4. Tokens (`src/theme.rs`)

Every colour is mixed from the nine Talvyn colours with `color-mix(in srgb, …)` maths, as a light/dark pair; no hex
appears anywhere else.

**Two styles**, each a light/dark pair, chosen in Settings → Appearance with a mode (System, Light, Dark; kept in
`appearance.txt`). **Cobalt** is Samo's own: Ink-tinted slate neutrals. **Graphite** is true greys with more depth: in
light a grey canvas under white cards, in dark a charcoal instead of near-black. A style only moves the greys (a
`Tone` in `theme.rs`: the neutral, the dark neutral, what secondary text leans toward, how far the light stage sits
from white, how much orange stays in dark Signal text). Cobalt, Signal and the characters are the family's in every
style, so Cobalt still means "selected" and "Samo suggests" in both. `SAMO_THEME` and `SAMO_STYLE` pin a mode or style
for screenshots and tests, over Settings. Tests check that Cobalt is byte for byte what shipped, that Graphite is
neutral, and every text pair at 4.5:1 in all four palettes.

Samo adds the readout tokens (`glass`, `glass_line`, `glass_text`, `glass_text_2`,
`phosphor` = Pale cobalt in dark and Cobalt in light, `phosphor_off`, `phosphor_ghost`, `glass_signal`) and runs a step denser and crisper than Mesh:

| | Samo | (Mesh) |
| --- | --- | --- |
| Control height | 36 (30 compact, in rows) | 40 |
| Radii chip / control / card / shell / stage / dialog | 6 / 8 / 12 / 14 / 18 / 20 | 8 / 10 / 16 / 20 / 24 / 28 |
| Type label / meta / body / lead / h2 / h1 | 11 / 12.5 / 14 / 16 / 17 / 28 | 12 / 13 / 15 / 17 / 19 / 32 |
| Sidebar | 240 (rail 72) | 264 |

Faces: Cabinet Grotesk (h1, h2, the name), Switzer (body), IBM Plex Mono (labels, tags, every figure in a column).
Plex Mono is bundled (OFL, `assets/fonts`). Cabinet Grotesk and Switzer are ITF-licensed and not redistributable;
Samo uses them when installed and falls back to the system face otherwise.

## 5. Components

| Component | File | Use it for |
| --- | --- | --- |
| Readout strip | `shell.rs` `readout_strip` | The four live figures, on every page. One row at every width. Memory and Disk free carry a small share after the unit (`67%` used, `18%` free) and their bar is that same share; it steps away with the side labels under 190 px a cell. A figure's unlit dots are a quiet ghost (`phosphor_ghost`), not the meter's empty track |
| Page head | `shell.rs` `page_head` | Dial + breadcrumbs (disk pages) + `h1` + one line + actions |
| Dial chip | `art.rs` `dial_chip` | A page's icon in a ring with twelve ticks |
| Logo mark | `art.rs` `logo_mark` | The gauge on an Ink disc |
| Disk ring | `art.rs` `ring_gauge` | A whole split into parts; dither parts are suggestions |
| Dot-matrix figure | `art.rs` `dot_number` | Big figures only. Unknown is drawn as `--`, never `0`. |
| LED meter / column | `art.rs` `led_meter`, `led_column` | A share of a whole. `None` lights nothing. |
| Split bar | `art.rs` `split_bar` | Source (solid) vs. what Samo suggests clearing (dither) |
| Dither frame / block | `art.rs` `dither_frame`, `dither_block` | A suggested chip or circle; legend and title swatches. Cards keep their ordinary border. |
| Line chart | `charts.rs` `LineChart` | One or more lines on one scale (never two scales); 2 px lines, no fill under them; a crosshair with an 8 px ringed dot per line |
| Segmented switch | `components.rs` `segments`, `segment` | A small set of choices: in a card's head (a time span, a sort) or beside a setting (Mode) |
| Section rule | `components.rs` `section_rule` | Splits a page into what it's about: `THIS MAC · LIVE`, `SPACE` |
| Measuring grid | `art.rs` `measure_grid` | The Storage map's ground. Not the stage: its dots, on every page, blurred "dots = Samo suggests" |
| Card, card head, h1, h2 | `components.rs` | Grouped content. A card is a surface and a hairline ring, with no shadow. A count chip is Cobalt mono. |
| Button | `components.rs` `button` | `Primary` (one per view) · `Default` · `Danger` (Signal text, never filled) · `Link`; `compact` inside rows |
| Busy button | `components.rs` `busy_button` | A primary action that is running: three pixels run along it |
| Tag | `components.rs` `tag` | Mono state labels; `lamp` adds the square lamp |
| Face switch | `components.rs` `face_switch` | Every on/off: its thumb sleeps when off and wakes when on |
| Empty / loading / error line | `components.rs` | Mascot asleep / Samo scanning / mascot blocked + words that say how to recover |
| Truncating line | `art.rs` `clip_text` (`components::line`, `mono_line`) | Any single line that may not fit. Give it `.flex_1()` in a row or `.w_full()` in a column. |
| Pop-up | `popup.rs` | A short note under the menu bar item. A non-activating panel: it never takes the keyboard. The kind in Samo's ring (an Ink `!` when something waits for you, a Cobalt dial with a check when something finished) and a short bar of the same colour inside the left edge; a mono line with where it's from and when (`CLAUDE SESSIONS · FINISHED  08:44`); the title (the project) and one sentence. One at a time; 6 s, held while the pointer is on it (which shows a close button); a click opens the page it is about |
| Extension page | `ext_view.rs`, `extensions_page.rs` | What an extension's helper sends, drawn with the components above: a section rule per section, figures and meters side by side in cells, rows with a lamp (`!` for "needs you"), switches, Default or Link buttons, notes |
| Text field | `text_input.rs` | One line (the store's search): a `CONTROL_H` field on the card colour, Cobalt line when focused, Cobalt caret, Cobalt-wash selection; Escape clears |
| Store list | `extensions_page.rs`, `store_view.rs` | One card of rows: a 40 pt icon, the name with `by author` in mono, the summary clipped to one line, a state tag (*installed*, *update* in Cobalt, *dev*, *off*, *needs a newer Samo*) and a chevron. Segments (All · Installed · Updates) and the search above; a line on where the list came from (*Offline: showing the store as of 14:02*) |
| Store detail | `extensions_page.rs` | A back link, then a card: a 72 pt icon, name, *signed* or *dev*, a mono line of facts (version, author, size, categories), Homepage, and on the right one Primary (Install, Update to x, Install signed version) beside a Danger Uninstall, then Open and the switch. Asks before replacing a dev build or uninstalling (naming the backup it keeps). Screenshots one at a time on the glass with ‹ › and *1 of 3*; About as paragraphs and Cobalt-square bullets; What it changes; What's new |
| Settings | `settings_page.rs` | The gear at the foot of the sidebar, under a gap (no group label). A list, not cards: a group is its `h2` and rows split by hairlines; a narrow window drops a row's control under its name. **Appearance**: Mode as a segmented switch; Style as tiles, each drawing a small Samo in that style in the light or dark in use (the one in use has a Cobalt ring *and* a check). A choice shows at once in every Samo window and is kept; a failed write is said on the page. **Updates**: one row, the headline (*Samo 0.3.0 is available* / *You have the latest Samo*) over a mono status (*Samo 0.2.0 · checked 14:02*), with Check now and the Primary *Install 0.3.0 and restart* (or why it can't, with the release page) beside it; What's new as bullets. On Windows, a **This PC** group first |
| Processes list | `processes.rs`, `app_icons.rs`, `samo-domain` `process_groups.rs` | Apps, not a flat list: helpers are rolled up into their app ("Grok Bot · 9 processes", the totals of all nine), applications (the OS's Dock apps) under one heading and background processes under another, each with its count. An app's icon beside its name on the Mac, a gear for what is in no app, nothing on Windows. Click an app to open it and see each process; a search box (name, path or PID) opens every app so a match inside one is seen. CPU and Memory carry a thin bar under the figure: the biggest on screen is the longest, with a floor so a quiet Mac isn't all full bars. Quit is offered for one process at a time, never for a whole app, and re-checks PID and start time (R4) |

## 6. Folders, the map, and colour

- **Where Samo measures** is a menu in each disk page's header (`shell.rs` `location_picker`, `src/locations.rs`),
  not a fixed list in the sidebar: no two Macs have the same folders. It offers the standard folders that exist on
  this Mac (Home, Desktop, Documents, Downloads, Developer, Applications), any folder you added with
  **Choose folder…** (the system's folder picker; kept in `~/Library/Application Support/Samo/locations.txt`,
  removable from the menu), and a check on the current one.
- **The space map** (`storage.rs`) is navigable: click a folder's circle to go inside it, **Up to …** to come back
  out. The last eight folders measured stay in memory, so going in and out is instant; a recent result says when
  it was measured, and Rescan measures again. A Trash or Undo forgets the recent results it changed.
- **Inside a circle**: name, size, and on big ones the folder icon, share and file count — every line clipped to
  the circle. Point at a circle and the side panel keeps it, with Go inside / Reveal / Move to Trash.
- **Colour means kind of file**, the same everywhere (map, By file type, file rows): Code blue, Documents aqua,
  Archives yellow, Media magenta, System green, Applications violet, Other grey (`theme::category_color`, fixed
  order in `CATEGORY_ORDER`, validated for colour-blind separation in both themes). Orange and red are kept out:
  in Samo they mean "wrong". A legend with sizes is always beside the map, so colour is never the only cue.
- **Classification** (`samo-domain` `classify_file_path`) knows developer files — build output (`.dll`, `.pdb`,
  extensionless binaries in `target/`), source maps, git history, database tables (`.sst`), compressed assets,
  fonts — so little is left as "Other".

## 7. Overview: live, then space

Overview answers two questions, each under its own rule, so it is worth watching while a folder is measured:

- **This Mac · live** (`src/live.rs`): **Activity** — CPU and memory on one 0–100 % scale (both are a share of what
  the Mac has), over 2 min / 10 min / 1 hour — beside **Top processes** (CPU or memory, the same choice as the
  Processes page). Under them four cards: **Network** (in/out), **Disk activity** (read/write), **Memory** (used,
  pressure, compressed, swap) and **Thermal** (macOS's own thermal state, Low Power Mode, uptime).
- **Space**: the disk ring, what Samo suggests, the largest folders and the file types.
- **Charts are history, honestly bucketed.** 120 slots across every span (1 s, 5 s or 30 s each); percentages
  average within a slot, rates keep their peak so a burst shows. A stopped sensor is a gap; a slot nobody sampled
  into is just skipped. Rate charts share one scale per card, rounded up to 1-2-5 of a unit and never under
  100 KB/s, so an idle line lies low instead of magnifying noise.
- **Two lines, two colours, fixed order** (`theme::series_pair`): a lighter Cobalt, then aqua — CPU/memory, in/out,
  read/write. Validated as a pair in both themes. A legend names every line; values are in text colours.
- **Pointing** at a chart shows a crosshair and a note of each line at that moment; it re-renders only when the
  slot under the pointer changes.
- **Equal heights without growing.** Cards in a row are stretched from outside and split into a top and a bottom
  (`live::ends`). Nothing inside a card *grows*: taffy sizes a column with a growing child taller than its content.
  The activity chart grows from a fixed base height, which is safe.

## 8. Layout (`src/layout.rs`)

One set of breakpoints, decided from the window's width every frame:

- **Sidebar**: full at 1100 px and wider; below that, a 72 px icon rail (names on hover).
- **Two columns** (Overview, Storage, Performance) when the content is ≥ 940 px; otherwise one column.
- **Live cards**: four across from 1060 px of content, two from 560 px, then one.
- **Readout**: always one row; figures scale and the side labels step away under 190 px a cell.
- **Process table**: PID from 560 px, disk I/O from 660 px, energy from 780 px.
- **Content** never runs wider than 1320 px; the window's minimum is 640 x 520.

## 9. The menu bar

Samo lives in the menu bar too (`src/menubar.rs`, via `tray-icon`'s safe `NSStatusItem` wrapper):

- **The item** is the probe as a 36 x 44 px template image (22 pt, pixel-exact on Retina; macOS tints it for the
  bar), with the CPU load beside it, padded with figure spaces so it never shuffles. Its eyes are the real state:
  dots when idle, a sweeping bar while measuring, X when a scan failed, a line before the sensors are ready.
- **A click** opens the quick panel under the item: the same readout as the app (a click on a figure opens that
  page), the busiest process across all processes, memory, the network, the folder, and Open Samo / Rescan / Quit.
  Clicking anywhere else closes it.
- **Extensions** (ADR-002): while they have something waiting for you, the count follows the CPU figure,
  `23% !1`. The probe's eyes stay Samo's own state. The quick panel adds a line per extension that is on (at most
  three) and grows to fit; a line can end in a small LED load bar with its figure (`5h 36%`, or `5h --` when
  unknown). A pop-up can appear under the item, without taking the keyboard.
- **Closing the window** leaves Samo running in the menu bar; the Dock icon, Show Samo (⌘0) or Open Samo brings
  the window back. ⌘Q quits.

## 10. Motion and performance

- A page arrives with a 180 ms fade, once. A pop-up arrives with a 220 ms drop and fade and leaves with a 160 ms
  fade. Nothing else animates by itself.
- Samo's probe steps every 150 ms **only while a scan runs**; an idle Samo draws nothing new.
- Sensors are read on their own thread (`src/hub.rs`) and the UI is woken by a channel only when there is news —
  never on the UI thread (a sensor read takes ~10 ms, which used to drop a frame every 1.2 s).
- Scans build what the pages need (`DiskView`: rankings, the suggestion set) on the scan thread, once.
- Store images are decoded by gpui off the UI thread, only after their size was read from their header; leaving a
  detail page drops its screenshots from gpui's textures and caches (memory stays flat after 20 pages).
- App icons, and which processes are Dock applications, are asked of AppKit (safe bindings only, R29) on a worker thread of their own
  (`app_icons.rs`), never on the UI thread; the page asks for an icon on every frame, gets what is ready or a queued load,
  and the worker wakes the UI when one arrives. An icon is the smallest of the OS's pictures that is at least 64 px, as an
  8-bit PNG (16 KB of texture), cached up to 512 apps. AppKit hands over half-float TIFFs that neither `image` nor gpui can read,
  so `app_icons.rs` reads that one layout itself. The Dock's applications are looked at every 2 s and wake the UI only if they changed.
- The process table is virtualised: only the rows on screen are built. While the pointer is over it, its rows hold
  still so a row can't move under a click; a tag says when updates are being held.
- The thermal state is one `NSProcessInfo` call on the sampler thread (no sensors, no privileges).
- Run the release build (`run-samo.sh` does). Idle, release: ~2 % CPU on Overview, ~4 % on Processes.

## 11. Rules

1. One Ink action per view. Signal only for wrong or irreversible, and never filled.
2. Dither only for what Samo suggests. Never as decoration. (One exception, from ADR-003: an image that hasn't
   arrived yet is a dithered placeholder, never a broken image.)
3. A cell's state is always something true. Idle cells hold still.
4. Unknown is never zero: dashes, empty meters, "not sampled yet".
5. Every colour is a token in `theme.rs`.
6. A single line that might not fit uses `clip_text`, not gpui's `truncate()` (it can paint a stale measurement in flex rows).
7. Nothing slow on the UI thread: sensors, scans and their derived data live on their own threads.
8. Nothing inside a card grows to fill it (no `flex_1`/`flex_grow` spacers): split it into a top and a bottom.
9. An extension draws only with Samo's widgets: no dither (that is Samo's suggestion), never the Primary action,
   Signal only for its errors. Waiting for you is Ink with `!`, not Signal: it isn't wrong.
10. An extension that declares windows (ADR-004) draws them with Samo's tokens, ported (Shots' `Theme.swift`): the
    readout's glass for anything over the screen, the mono face for figures, Cobalt for what is chosen. Its windows
    never take a click they weren't asked for, and a note near the pointer fades by itself. Clips' list at ⇧⌘C sits on
    the same glass: the selected row in a Pale cobalt wash, ⌘1–⌘9 and figures in mono, a pin in Pale cobalt.
11. Only what floats casts a shadow (a menu, a dialog, a tooltip, a chart's note). A card's ring lifts it; no blurred
    shadows under cards or the readout, and no dotted decoration across the stage.
12. Say a thing once per screen: a label doesn't repeat the heading beside it, and a number (a count, free space) is
    drawn in one place.
13. A new style is a `Tone`, not new colours. It must pass the palette tests: neutral where it says it is, and every
    text pair at 4.5:1.
14. The readout follows the theme, and is always a little darker than the stage it sits in so it reads as recessed.
    A wrong figure on it uses `glass_signal`, never `signal` directly: plain orange is too faint on the light panel.
15. What only the OS knows (an app's icon, which processes are in the Dock) is asked on its own thread through safe bindings, never
    on the UI thread. An icon that isn't there yet is an empty place and one the OS doesn't have is a gear: never a stand-in
    icon, and on an OS that can't supply them, none.
16. A readout cell says one thing: its figure, its small share and its bar are of the same quantity (Memory: used; Disk free: free).

## Decisions

| Decision | Why | Hard to undo? |
| --- | --- | --- |
| The ring instead of Mesh's hexagon | Samo measures disks and gauges; the shape says what it is | No: `dial_chip`, `logo_mark`, `ring_gauge` |
| A dark readout on every page | An instrument's face; the live figures are always one glance away | No: `readout_strip` |
| Denser scale than Mesh | A data instrument; more rows and figures per screen | No: constants in `theme.rs` |
| Suggestion = dither | Samo never deletes on its own (design doc anti-goal 2); the picture says whose idea it is | Yes: it is the product's grammar |
| Sampler thread + wake channel | Steady frames; launch doesn't wait for sensors | No |
| Held rows while pointing | A process row must not move under a "Quit" click | No |
| Live section on Overview | Watching the Mac shouldn't wait on a disk scan; one page answers "how is it doing" and "where did the space go" | No: `live.rs` |
| No dither border on cards | A dotted frame around a whole card read as noise and broke the card family; the dither stays on marks | No |
| No card or readout shadows, no stage grid, no stage ring | Three layers did one job (lift a block off the page); the ring alone does it. The grid's dots blurred "dots = Samo suggests" | No: `card()`, `glass_panel()`, `render_stage` |
| Graphite as a second style, moving only the greys | KJ asked for another style; Cobalt, Signal and the characters keep their meaning in both. A trace of grey was invisible, so it has real depth | No: `Tone` in `theme.rs` |
| Mode and style in `appearance.txt` | Like `locations.txt`: a plain file, nothing online, and a bad file is just the defaults | No |
| The sidebar keeps its Disk row, without the bar | The Disk is one of the four characters (§3); the bar repeated the readout's | No |
| The readout follows the theme (it was dark in both until 2026-11-07) | KJ: a black slab in a light window looked wrong and "should change with the theme". Its figures and its wrong-figure colour have a light and a dark form | No: `light()` and `dark()` in `theme.rs` |
| Helpers roll up into their app on the Processes page | Nine rows named "Grok Bot Helper" say nothing; "Grok Bot, 9 processes, 1.4 GB" does. Quit stays per process so one click can't end an app's every helper | No: `process_groups.rs`, `layout_lines` |
| The collector returns every process, not the busiest 150 | An app's total over only some of its helpers would be wrong | No: `PROCESS_ROWS_MAX` |
| Icons through AppKit, read from its TIFF by our own small reader | AppKit's PNG call is `unsafe` in the bindings (R29), and `image` and gpui can't read its half-float TIFFs (icons came out blank) | No: `app_icons.rs` |
| Disk free's bar shows the free share, and a `%` follows Memory and Disk | The figure is free GB, so a bar of the *used* share beside "18%" contradicted it; KJ asked for the % | No: `readouts()` in `shell.rs` |
| The unlit dots of a figure are their own quiet colour | On the light panel they were as visible as the lit ones (1.26:1 against the glass) and "97.3" read as "97.8"; a test now keeps them under 1.2:1 | No: `phosphor_ghost` |
