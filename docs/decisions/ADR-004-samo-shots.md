# ADR-004: Shots, a screenshot extension, and extensions with their own windows

**Date:** 2026-11-01
**Status:** Accepted by KJ on 2026-11-01 ("Approved"), with the [call]s as written. The builder check is folded in, and KJ answered its two questions.
**Product:** Samo (`samo`)
**Builds on:** ADR-002 (extensions) and ADR-003 (the store). Their rules R5–R14 still hold. This ADR adds a kind of
extension that opens its own windows, and the first one: **Shots**, a screenshot tool with Shottr's features.

---

## 1. Intent

**[KJ]** KJ uses Shottr and wants all of its features in Samo, as an extension: installed from the store by whoever
wants it, not built into Samo for everyone.

**[KJ]** Decisions made on 2026-11-01:
- **Where:** a store extension. Building it into Samo was not chosen.
- **Language:** the helper is written in Swift. Rust was not chosen: Apple's capture, text-recognition and drawing
  APIs are reachable from Rust only through `unsafe`, which Samo forbids.
- **Uploading:** left out. Shots never puts a screenshot online. Uploading on a click was not chosen.
- **Scope:** everything: capture and pin, the annotation editor, text recognition with colour picker and ruler, and
  scrolling capture.
- **Screen Recording belongs to Shots alone**, so macOS asks for Shots and no other extension gains screen access.
  Letting Samo hold it, which would cover every extension, was not chosen.
- **Shortcuts: Shottr's usual keys,** ⌘⇧1 for the whole screen and ⌘⇧2 for an area. KJ will quit Shottr. Keys
  Shottr doesn't use were not chosen.

**Why it's new rather than a reuse.**
- Shottr is closed source, like Glance: its features are ideas anyone may build, but none of its code, name, icon or
  look is used.
- macOS's own ⌘⇧5 has no pinning, text recognition, colour picker, ruler or scrolling capture.
- Samo is already in the menu bar, and the store already installs and updates signed extensions.

**What it is.** A small native helper that Samo starts like any extension. It listens for its shortcuts, freezes the
screen, lets you select, and then copies, saves, pins or opens the shot in its editor. Samo shows its page: capture
buttons, recent shots, settings and whether macOS has granted Screen Recording.

**Who it's for.** KJ and teammates: developers who take many screenshots for issues, docs and chat.

---

## 2. Goals, Non-Goals, Anti-Goals

### Goals
1. **[KJ]** Shottr's features: area, window, full-screen and scrolling capture; pinning; an annotation editor; text
   recognition; a colour picker; a pixel ruler.
2. **[call]** Capture feels instant. After the shortcut, the frozen screen and crosshair are up within 150 ms.
3. **[call]** It reads Thai. Text recognition uses macOS's accurate mode, which reads Thai (`th-TH`) and 32 other
   languages on this Mac.
4. **[call]** Samo's look in its windows: the same palette, fonts and controls as `apps/samo-gpui/DESIGN.md`.

### Non-Goals (v1)
- **[KJ]** Uploading, share links, cloud history.
- **[call]** Screen video or GIF recording.
- **[call]** Automatic scrolling for scrolling capture. Scrolling the page for you needs Accessibility permission;
  you scroll yourself instead.
- **[call]** Opening image files that Shots didn't capture.
- **[call]** Pins that survive quitting Samo.
- **[call]** macOS before 14: the capture API Shots uses (ScreenCaptureKit's screenshot manager) starts there.

### Anti-Goals
These are **[call]**s, extending ADR-002's and ADR-003's.
1. **A screenshot leaves the Mac.** Anything in Shots goes online, ever.
2. **Capture is slow, or wrong.** The overlay lags behind the shortcut, or the frozen image isn't what was on screen.
3. **Shots makes Samo heavy or fragile.** Memory grows with each shot, idle CPU isn't zero, or a crash in the editor
   reaches Samo.
4. **It changes the Mac without saying.** A shortcut quietly taken from another app, a permission prompt with no
   explanation, or files left behind that weren't named.
5. **"Own windows" becomes a loophole.** Any extension can draw over the screen or look like Samo without having
   declared it, and without the store showing it before install.
6. **A shot is lost.** An annotated shot disappears because a window closed, or a copy silently failed.

---

## 3. Core Features

| # | Feature | Covers (v1) | Does not cover (v1) |
| --- | --- | --- | --- |
| **S1** | **Capture** | Area (crosshair, magnifier, live pixel size, arrow keys nudge by 1 px), a window (hover to highlight, click), the whole screen (each display), repeat the last area, a 3/5/10 s delay. The screen is frozen at the shortcut, so menus and hover states are captured as they were. After capture, by setting: copy, save, open in the editor, or pin. | Video, a series of shots on a timer. |
| **S2** | **Pin** | A shot floats above every window and Space. Drag to move, pinch or corner to resize, scroll to change opacity, double-click for the editor, Esc or × to close, ⌘C to copy. Many at once. | Pins after quitting. |
| **S3** | **Editor** | Arrow, line, rectangle, ellipse, text, highlighter, numbered steps, blur, pixelate, spotlight, crop; colour and width; undo and redo; copy, save, drag out as a file. Asks before discarding changes. | Layers, pressure, opening other images. |
| **S4** | **Read and measure** | Copy the text in a selection or a whole shot (Thai, English and every language macOS reads). Read a QR code. A colour picker with a magnifier that copies hex. A ruler: drag to measure in pixels, or hover to measure the gap between the edges around the pointer. | Translating text. |
| **S5** | **Scrolling capture** | Select an area, then scroll it yourself; Shots stitches one tall image by matching what overlaps, up to 30,000 px tall. Esc or a click ends it. | Sideways scrolling, scrolling for you. |
| **S6** | **In Samo** | Its page: capture buttons, *Screen Recording: allowed / not yet* with a button to System Settings, its shortcuts and whether each is working, the 20 most recent shots (Copy, Reveal, Pin, Edit), and its switches. The quick panel's line: *Shots · 12 today*. | Thumbnails in Samo's page (the protocol has no image widget yet). |

### The Platform Underneath

**[call] Extensions that open windows.** Until now an extension draws only inside its Samo page, with Samo's widgets.
A screenshot tool can't: it needs a full-screen overlay, an editor and pins. So a manifest may now declare:

```toml
windows = "Opens its own windows: the capture overlay, the editor and pinned shots"
min_macos = "14.0"
permissions = ["screen-recording"]

[limits]
memory_mb = 800
cpu_pct = 60
```

- Each line is shown on the store's detail page and on the Extensions page, beside *What it changes on your Mac*,
  before Install. `ext-publish` copies them into the catalog, and they must match the package (R11).
- `memory_mb` raises R8's 150 MB limit for this extension, up to a ceiling of 1024 MB. `cpu_pct` raises R8's 10 % of
  a core, averaged over 60 s, up to a ceiling of 100 %: a minute of scrolling capture, the ruler or text recognition
  is real work, and R8 kills a helper over its limit at once.
- `min_macos`: the store shows *Needs macOS 14* and disables Install below it; the host won't start it either.
- An extension that opens windows without declaring them is stopped (R15).

**[call] The helper.** `extensions/shots` is a Swift package with one product, `samo-shots`, built for arm64 and
macOS 14.
- It runs as an `NSApplication` with the `.accessory` policy: no Dock icon and no menu bar of its own.
- It speaks protocol v1 on stdin and stdout on a background thread, and does everything visible on the main thread.
- It quits when its stdin closes, so a helper left behind by a Samo crash never keeps its shortcuts. Samo now starts
  every helper in its own process group, and stops the whole group.
- On `shutdown` (off, update, uninstall, quitting Samo) it saves anything not yet saved, an edited shot or a pin that
  was only on the clipboard, into the folder as `… (unsaved).png`, within the 2 s grace, and the page says so.
- It is packaged like any extension (`bin/samo-shots` and `extension.toml`). `ext-package` learns Swift: a folder
  with `Package.swift` is built with `swift build -c release`, and its `share/` folder (the fonts) is packaged too.
- It uses no SwiftPM resources (`Bundle.module` points at the build Mac). Fonts load from `share/fonts` beside the
  executable, falling back to the system's.

**[call] Capture.**
- ScreenCaptureKit's `SCScreenshotManager` captures every display at the shortcut, at the display's full resolution.
  The overlay then shows that frozen image, and selection happens on it.
- Window capture uses `SCShareableContent` for the windows on screen and captures the chosen one alone, with its
  shadow (`ignoreShadowsSingleWindow = false`), at the click. So it is live, not frozen: that's how a window comes
  out whole, without what overlaps it. Area and full-screen captures are frozen.
- `SCShareableContent` is kept and refreshed when displays or windows change (workspace notifications), never by
  polling, so the overlay meets 150 ms.
- A shot is held as one `CGImage`. Every window that shows it holds a reference, and it is freed when the last one
  closes.

**[KJ] Screen Recording belongs to Shots alone.**
- **Why it needs doing:** Samo starts helpers with a plain process spawn, so by default macOS would give the
  permission to **Samo**. Every helper Samo runs could then capture the screen: Claude Sessions, any store extension,
  any dev build.
- **[call] How:** `samo-shots` is a thin first process.
  - It starts its working half, `samo-shots --worker`, with `posix_spawn` and the responsibility-disclaim attribute.
    The worker has its own Info.plist built in (bundle id `dev.talvyn.samo.shots`), so macOS treats it as its own app
    and asks for **Shots**.
  - The first process relays stdin and stdout to the worker, and both quit when stdin closes.
  - The attribute is private API, looked up by name at run time. If it's missing, Shots falls back to Samo's grant,
    and its page says that Samo, and so every extension, can now see the screen.
- **[call] Either way:**
  - Shots' page explains why it's needed before macOS asks.
  - After it's allowed, the page offers **Restart Shots**, because macOS applies it only to a new process.
  - `CGPreflightScreenCaptureAccess` tells Shots when the grant is gone. It says so instead of capturing black.
- **Known costs:**
  - Shots is ad-hoc signed, and macOS ties the grant to that exact build. So it will likely need allowing again
    after each Shots update; not after Samo's.
  - macOS 15 and later re-asks about once a month for screen capture that doesn't use the system's picker.

**[call] Shortcuts.**
- Registered with Carbon's `RegisterEventHotKey`, which needs no Accessibility permission.
- **[KJ]** Defaults: ⌘⇧1 for the whole screen, ⌘⇧2 for an area.
- **[call]** No other global defaults. Inside the ⌘⇧2 overlay, one key switches mode, as Space does in macOS's ⌘⇧4:
  - **Space:** window
  - **S:** scrolling capture
  - **T:** copy text
  - **C:** colour picker
  - **R:** ruler

  Every mode also starts from Shots' page, and any can get its own shortcut in Shots' settings window. Shortcuts that
  type characters or format text (⌘⇧7/8/9 are lists and quotes in Slack, Docs and Gmail) are never defaults.
- **[call] Conflicts:**
  - `RegisterEventHotKey` fails only against an exclusive registration. Two apps can hold the same keys, both fire,
    and macOS never says who holds them.
  - So Shots also checks macOS's own screenshot shortcuts, and whether a known screenshot app is running (Shottr,
    CleanShot X, Snagit, Xnapper), and says *may clash with Shottr*.
- Changing them happens in Shots' own small settings window (*Change shortcuts…* on its page), because Samo's widgets
  can't record a key.

**[call] Where shots go.**
- By default after a capture: copy to the clipboard and save to `~/Pictures/Shots`, as
  `Shot 2026-11-01 at 21.35.12.png`. Not the Desktop, which iCloud uploads when "Desktop & Documents" is on. If you
  choose a folder inside iCloud Drive, the page says it will be uploaded.
- The clipboard copy is for this Mac only (`.currentHostOnly`), so Universal Clipboard doesn't send it to your other
  devices.
- Settings: the action (copy, save, both, editor, pin), the folder, PNG or JPEG, 1x or Retina, a shutter sound.
- Shots keeps only a list of the last 20 file paths and its settings in its `data/`. Images are never kept anywhere
  but where you saved them.

**[call] Text recognition.** Vision's `VNRecognizeTextRequest`, accurate level, with automatic language detection,
over the system's preferred languages plus Thai and English always. QR codes use `VNDetectBarcodesRequest`. Everything runs on the Mac.

### How Each Feature Behaves

#### S1 — Capture
1. The shortcut is pressed. Within 150 ms every display shows its frozen image, dimmed, with a crosshair and a
   magnifier.
2. Drag selects an area; its size is shown in pixels. Space moves the selection; arrow keys nudge it; Return
   captures; Esc cancels. Holding ⇧ while dragging keeps a square.
3. In window mode the window under the pointer is highlighted; a click captures it.
4. The shot goes where the after-capture setting says. A small note near the pointer says what happened (*Copied and
   saved*), and the shot joins the recent list in Samo.

#### S2 — Pin
1. A pinned shot opens where it was taken, at its real size, above all windows and on every Space.
2. Closing it frees its image. Pins don't come back after Samo quits.

#### S3 — Editor
1. A window with the shot, a toolbar of tools, and Copy, Save and Pin. Drawing is vector until exported, so undo
   is exact and the export is sharp.
2. Blur and pixelate are applied to the exported pixels. They can't be undone in the saved file.
3. Closing with changes not copied or saved asks *Discard changes?* (R19).

#### S4 — Read and measure
1. **Copy text:** select an area; its text is recognised and copied, and a note shows the first line. If nothing is
   found, the note says so.
2. **QR:** in the editor or on a selection, a QR code's text is shown with Copy and, for links, Open.
3. **Colour:** a magnifier follows the pointer on the frozen screen. A click copies `#RRGGBB`; ⇧-click copies
   `rgb()`.
4. **Ruler:** drag to measure a box in pixels. Hover to see the distance to the nearest edges up, down, left and
   right, found from changes in colour on the frozen image.

#### S5 — Scrolling capture
1. Select the area to scroll, then scroll it with the trackpad or mouse. Inside the selection the overlay lets the
   pointer through to the app; Shots captures that area as it moves, and adds each new strip below by matching the
   overlap.
2. A preview grows at the side. Esc (a shortcut held only while this runs, since the app has the keyboard), a click
   outside the selection, or the cap ends it: 30,000 px tall or 400 MB of pixels, whichever comes first. Then the shot goes where the after-capture
   setting says.
3. If a strip can't be matched (the page jumped), Shots stops there and keeps what it has.

#### S6 — In Samo
1. Shots' page in Samo is drawn with Samo's widgets: a *Capture* section of buttons, *Recent* rows (name, size,
   when; Copy, Reveal, Pin, Edit), *Shortcuts* as rows with a lamp (working, or may clash), and switches.
2. **[call] Three additions to protocol 1**, all optional so older helpers are unaffected:
   - A row may carry buttons (`actions: [{id, label}]`), sent back as `action` with the row's id.
   - A `choice` widget picks one of a few (after capture: copy, save, both, editor, pin; PNG or JPEG). The folder and
     the shortcuts are set in Shots' own small settings window, since they need a picker and a key recorder.
   - `{"type":"capturing"}` and `{"type":"captured"}`: Samo hides its own windows in between, so a capture started
     from Samo's page doesn't capture Samo.
3. Turning Shots off unregisters its shortcuts and closes its windows. Uninstalling also deletes its `data/`; shots
   you saved stay where they are, and the page says so.

---

## 4. Rules That Protect the Anti-Goals

These continue ADR-002's and ADR-003's R5–R14.

| Rule | Protects | Enforcement | Required test |
| --- | --- | --- | --- |
| **R15 — Windows only when declared** | Anti-goal 5 | A manifest without `windows` may not open windows. Every 2 s the host asks the window server for every window, on screen or not, on any Space (`kCGWindowListOptionAll`, safe in `objc2-core-graphics`, already in the lock file), owned by the helper or any process it started; any for an undeclared extension stop its whole process group, *Opened windows it didn't declare*. `windows`, `permissions`, `min_macos` and `[limits]` are shown before install and must match the catalog (R11). | `undeclared_windows_stop_the_helper` (the window list is a parameter, so the test feeds it); `manifest_capabilities_must_match_listing` |
| **R16 — Screenshots stay on the Mac** | Anti-goal 1 | Shots' Swift sources may not use `URLSession`, `URLRequest`, `NWConnection`, `import Network`, `CFNetwork`, `CFStream`, sockets, `Process` or `NSTask`; `posix_spawn` appears only in the file that starts the worker, and `URL(string:` only in the one that hands a QR code's link to the browser (the `network` gate scans `extensions/shots`). The clipboard is this Mac only. Copy-text and QR run on the Mac. | xtask `network` gate; `rejects_network_in_shots` |
| **R17 — A declared budget, and nothing kept** | Anti-goal 3 | `memory_mb` at most 1024 and `cpu_pct` at most 100, enforced like R8. A shot's image is freed when the last window showing it closes. Scrolling capture stops at 400 MB of pixels. Idle, the helper does nothing: no timers, no polling. "Back to idle" means within 50 MB of its memory after the first capture (AppKit and Vision keep what they load). | `declared_limits_enforced`; Swift `ShotReleasedWhenWindowsClose`; manual check: 50 captures, then back to idle |
| **R18 — Nothing taken silently** | Anti-goal 4 | A shortcut that fails to register is reported on the page, never forced. The permission is explained before macOS asks. Off and uninstall unregister shortcuts and close windows. Uninstall names what it leaves: the shots you saved. | Swift `ShortcutConflictIsReported`; `uninstall_closes_windows_and_keeps_saved_shots` |
| **R19 — No lost shot** | Anti-goal 6 | The after-capture action happens before any window opens, and a failed copy or save is shown, not swallowed. The editor asks before discarding changes. On `shutdown`, anything unsaved is saved as `… (unsaved).png`. | Swift `CaptureActionRunsBeforeWindows`, `EditorAsksBeforeDiscarding`, `ShutdownSavesUnsaved` |

And for speed (anti-goal 2): the frozen overlay is up within 150 ms of the shortcut (p95, M1), checked by hand with a
timing log, since CI can't grant Screen Recording.

---

## 5. How It Gets Made

### KJ's Decisions
- **[KJ]** A store extension, in Swift, with no uploading, and all four groups of features.
- **[KJ]** Screen Recording belongs to Shots alone.
- **[KJ]** Shottr's usual keys: ⌘⇧1 and ⌘⇧2.

### Calls
- **[call]** The name **Shots** (`id = "shots"`, helper `samo-shots`). Shottr's name and look are not used.
- **[call]** Protocol 1 stays, with optional additions: the manifest's `windows`, `permissions`, `min_macos` and
  `[limits]`; row buttons; the `choice` widget; `capturing`/`captured`. Older Samos can't install Shots anyway,
  because its `min_samo` is the Samo that supports them (0.3.0).
- **[call]** Swift's equivalent of Samo's never-crash lints: no `try!`, `as!`, `fatalError` or `precondition` in
  Shots, checked by a new `swift` gate in xtask; force unwraps are avoided by review, since no grep finds them
  reliably.
- **[call]** Shots draws its windows in AppKit, with Samo's tokens ported to a small `Theme.swift` (colours for light
  and dark, Cabinet Grotesk, Switzer and the mono face, radii, the dither). The package bundles the same font files.

### Architecture
- **Rust, Samo's side:**
  - `samo-ext-protocol`: the new manifest fields.
  - `samo-domain`: their checks (the memory ceiling, macOS versions) and the catalog's copies of them.
  - `samo-extensions`: the per-extension limits, helpers in their own process group, the R15 window check (the
    window list comes in through a trait, so it can be tested), and `min_macos` at start and install.
  - `samo-gpui`: capabilities on the store's detail page and the Extensions page; *Needs macOS 14*; row buttons and
    `choice` in `ext_view.rs`; hiding Samo's windows between `capturing` and `captured`.
  - `xtask`: `ext-package` for Swift, the `network` gate for Swift sources, and the new `swift` gate.
- **Swift, Shots:** `extensions/shots/`
  - `Package.swift` and `Sources/SamoShots/`, one folder per feature:
    - `App/`: the entry point, the worker spawn (R16's one `posix_spawn`), `NSApplication`, the protocol over
      stdin/stdout
    - `Capture/`, `Overlay/`, `Pin/`, `Editor/`, `Recognize/` (text and QR), `Measure/` (colour and ruler), `Scroll/`
    - `Settings/`, `Theme/`
  - `Tests/SamoShotsTests/` (XCTest): everything that needs no screen, including protocol messages, stitching on
    synthetic strips, ruler edges on synthetic images, image release, settings and the editor's undo.
- **The baseline's lints don't reach Swift.** This is the first non-Rust code in the workspace. The `swift` gate
  and XCTest in CI (`swift test`, on the macOS runner) are how its rules are kept.

---

## 6. Build Order

### Phase 1 — Windows for extensions, and capture. The end-to-end demo.
- **Builds:**
  - Samo's side: the manifest fields, limits, R15, `min_macos`, and capabilities shown in the store.
  - `ext-package` for Swift, and the gates.
  - The Shots helper: protocol, permission flow, shortcuts, area, window and full-screen capture with the frozen
    overlay, after-capture actions, pins.
  - Shots' page in Samo, and its listing in the store.
- **Done when** KJ, with Shots installed from the store:
  1. Sees *Opens its own windows*, *Screen Recording* and *Up to 800 MB* on its page before installing.
  2. Allows Screen Recording for **Shots** when macOS asks, and restarts it from its page.
  3. Presses ⌘⇧2 and drags. The shot is on the clipboard and in `~/Pictures/Shots` within a second.
  4. Pins it and moves it over another app on another Space.
  5. Sees it in Shots' recent list in Samo and copies it again from there.
  6. Turns Shots off: ⌘⇧2 does nothing, and the pin is gone.
  7. Takes 50 shots: memory returns to idle. Idle CPU is unchanged.

### Phase 2 — The editor
- **Done when** a shot is annotated with every tool, undone and redone, saved and dragged into another app, and
  closing it unsaved asks first.

### Phase 3 — Read and measure
- **Done when** text from a Thai web page is copied correctly, a QR code's link opens, a colour is copied as hex,
  and the ruler measures a button's padding.

### Phase 4 — Scrolling capture
- **Done when** a page three screens long is captured into one image without seams or repeats.

---

## 7. What Carries Over

| Concept | What it becomes | Source |
| --- | --- | --- |
| The extension host, R5–R10 | Shots runs under it, with a declared memory limit | `crates/samo-extensions` |
| The store, R11–R14 | Shots is signed, listed and updated like Claude Sessions | ADR-003 |
| R7: declare, back up, undo | R18: declare, report conflicts, unregister on off | `extensions/claude-sessions` |
| DESIGN.md tokens | `Theme.swift` | `apps/samo-gpui/src/theme.rs` |
| Shottr | Feature ideas only: no code, name, icon or look | — |

---

## 8. Customisation per Machine
- The after-capture action, the folder, PNG or JPEG, 1x or Retina, the sound: switches on Shots' page and its
  settings window.
- Shortcuts: its settings window.

---

## 9. Left to the Builder
- The exact look of the overlay, magnifier, toolbars and notes, within DESIGN.md.
- The stitching method, as long as Phase 4's check passes.
- How the ruler finds edges, as long as Phase 3's check passes.

---

## 10. Open Questions
None. KJ answered the builder check's two questions on 2026-11-01 (§1).
