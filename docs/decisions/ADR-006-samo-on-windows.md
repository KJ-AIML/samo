# ADR-006: Samo on Windows

**Date:** 2026-11-03
**Status:** Proposed. KJ asked on 2026-11-03 for the whole app on Windows ("All of APP … I want it all"). The
`[call]`s are the builder's and are confirmed or changed in review of the first PR.
**Product:** Samo (`samo`)
**Builds on:** ADR-001 to ADR-005. Their rules R1–R24 hold on Windows as written, unless §4 says how Windows meets
them. Tracking issue: #13. Phases: #1, #9, #10, #11, #12.

---

## 1. Intent

**[KJ]** Samo works well on KJ's Mac every day, and KJ wants the same instrument on a Windows PC: all of it, the app
and the extensions. It is for KJ's own PC first.

**What it is.** The same Samo, built from the same repo, running natively on Windows 10/11 (x86_64):
- the app, `samo-cli` and the web console
- Claude Sessions, Clips and Shots
- the store and updates

The Mac build does not change.

**Checked before writing:**
- **GPUI.** GPUI 0.2.2, which draws Samo, has a full Windows backend (DirectX renderer, DirectWrite text).
- **Pure crates.** On this PC (MSVC toolchain) `samo-primitives`, `samo-blueprint`, `samo-ext-protocol`, `samo-domain` and `samo-store` already build, and all 42 of their tests pass.
- **What doesn't build yet:**
  - `samo-macos`, `samo-extensions` and `samo-gpui` (unix and AppKit calls)
  - `samo-testkit` (unix file modes)
  - `xtask` (the macOS Keychain)

---

## 2. Goals, Non-Goals, Anti-Goals

### Goals
1. **[KJ]** Every page and extension Samo has on the Mac, on Windows.
2. **[call]** Native, not a lookalike. Windows conventions where they differ:
   - the tray, not a menu bar
   - the Recycle Bin, not the Trash
   - Explorer, not Finder
   - Ctrl, not ⌘
   - the native titlebar
3. **[call]** One codebase. Windows code sits beside the Mac code, chosen by `cfg(target_os)`; pure rules are shared.

### Non-Goals (v1)
- **[call]** Windows on ARM, Linux, and Windows older than 10.
- **[call]** An MSI or MSIX installer. A zip, installed per user without admin, is enough (#10).
- **[call]** Pinning Shots' pins to every virtual desktop. Windows has no supported way.

### Anti-Goals
1. **The Mac gets worse.** Any change in how Samo behaves on a Mac.
2. **Windows is quietly weaker.** A rule that holds on the Mac silently doesn't on Windows:
   - a permanent delete instead of the Recycle Bin
   - a made-up `0`
   - a helper left running
3. **`unsafe` creeps in** because Windows APIs are easier that way.
4. **A port that pretends.** Something that can't work on Windows yet looks as if it does.

---

## 3. What Windows Has Instead

| On the Mac | On Windows |
| --- | --- |
| `~/.Trash`, with Undo | The Recycle Bin, with Undo (restore the same item) |
| `(device_id, inode)` (R2) | `(volume serial, file index)`, read from a file handle |
| `/bin/kill -TERM` / `-KILL` (R4) | `taskkill` / `taskkill /F`, after the same PID + start-time check |
| `sysctl`, `vm_stat`, `ipconfig` | `sysinfo`, and Windows' own counters where a safe crate reads them; otherwise `Unsupported` |
| Thermal state, Low Power Mode, P/E cores | `Unsupported` unless Windows reports them (R3) |
| The menu bar item and its title | The tray icon. The CPU figure goes in its tooltip, since Windows trays have no title |
| Closing the window keeps Samo in the menu bar | The same, by catching the close (GPUI quits a Windows app when its last window closes), plus one instance |
| `/usr/bin/open -R` (Reveal in Finder) | Show in Explorer |
| `~/Library/Application Support/Samo` | `%APPDATA%\Samo`; caches and extensions in `%LOCALAPPDATA%\Samo` |
| ⌘ shortcuts | Ctrl (`secondary-` in GPUI keybindings) |
| `/`-rooted paths | `C:/Users/kj/…` (§4, R26) |
| Swift extensions (Shots, Clips) | Rust rewrites (#11, #12) |

---

## 4. Rules

These continue R1–R24.

| Rule | Protects | Enforcement | Required test |
| --- | --- | --- | --- |
| **R25 — The Mac is unchanged** | Anti-goal 1 | Windows code is behind `cfg(target_os = "windows")`, or is platform-neutral and tested on both. The macOS CI jobs run as before. | macOS CI (`check`, `db`) |
| **R26 — One shape for Windows paths** | Anti-goal 2 | Windows paths are normalised by `samo_domain::paths::normalize_path`: `/` separators, upper-case drive, no trailing `/` except the drive root (`C:/`). Only `/…` and `X:/…` are absolute. Anything else (relative, UNC, `C:foo`) is refused as a scan root and protected from cleanup. A `/`-rooted path is never changed, since a Mac name may contain `\`. | `normalize_keeps_mac_paths_and_shapes_windows_ones`, `only_rooted_and_drive_paths_are_absolute` |
| **R27 — R2 on Windows** | Anti-goal 2 | `is_protected_system_path` also protects, in any case: drive roots, `Windows`, `Program Files`, `Program Files (x86)`, `ProgramData` (and everything under these four), `Users` and each `Users/<name>`, `$Recycle.Bin` anywhere, `System Volume Information`, `Recovery`, `pagefile.sys`, `hiberfil.sys` and `swapfile.sys`. The identity check is unchanged. | `windows_system_paths_are_protected_in_any_case` |
| **R28 — Said, never faked** | Anti-goals 2, 4 | A reading Windows can't give is `Unsupported` (R3). A feature whose Windows mechanism isn't built yet stays off on Windows, and its page says so. | Per phase |
| **R29 — No `unsafe` for Windows** | Anti-goal 3 | `unsafe_code = "forbid"` stays for every crate. Windows APIs are reached only through safe crates (`sysinfo`, `trash`, `winapi-util`, `tray-icon`, GPUI). A need with no safe wrapper comes back to KJ as a decision. | The compiler |
| **R30 — The gates run on Windows** | Anti-goals 1, 2 | `xtask` builds on Windows. Gates 1–6 run there. Gate 7's Swift build and tests run only on macOS, where the Swift extensions are. Files are LF in every checkout (`.gitattributes`), and the gates read CRLF too. CI has a Windows job (`scripts/ci-windows.sh`). | `crlf_test_module_is_not_shipped`; CI `windows` |

---

## 5. How It Gets Made

- **[call] A `samo-windows` crate (L5) beside `samo-macos`,** with the same functions:
  - `scan_directory`, `inspect_node_identity`
  - `move_to_trash`, `restore_from_trash`
  - `inspect_process_identity`, `terminate_verified_process`
  - `SystemCollector`

  Each app chooses one with `cfg(target_os)`. `samo-macos` is not edited for Windows. This beat renaming it to `samo-platform` with two modules, which would touch every Mac path and gate for no gain to the Mac.
- **[call] The MSVC toolchain** (`x86_64-pc-windows-msvc`). The repo pins `1.99.0`; on a PC whose rustup default host is GNU, use `RUSTUP_TOOLCHAIN=1.99.0-x86_64-pc-windows-msvc`.
- **[call] Until #1 is done, Windows checks the crates that build there.** `scripts/ci-windows.sh` lists them, and the list grows with each PR until it is the whole workspace.
- **[call] Extensions and the store stay off on Windows until #9 and #10.** Their pages say so (R28).

## 6. Build Order

### Phase 1 — The app on Windows (#1)
- **PR 1, the foundation:**
  - this ADR
  - LF line endings
  - `xtask` on Windows
  - `samo-testkit` on Windows
  - Windows paths and protected paths in `samo-domain`
  - the engine accepting `C:/…` scan roots
  - a Windows CI job
- **PR 2:**
  - `samo-windows`: scanner and identity, Recycle Bin with Undo, Quit, telemetry
  - `samo-cli` and `samo-server` on Windows
- **PR 3, the app:**
  - the tray and quick panel
  - keeps running after its window closes, as one instance
  - the native titlebar
  - Ctrl shortcuts, fonts, Windows wording
  - the icon and DPI manifest
- **Done when** on this PC:
  1. `cargo build --release -p samo-gpui` builds.
  2. Samo opens, scans a folder and shows live telemetry.
  3. It moves a folder to the Recycle Bin and Undo brings it back.
  4. It quits a process only after checking it.
  5. It stays in the tray after its window closes.
  6. `cargo test --workspace` passes on Windows, and the macOS jobs pass unchanged.

### Phases 2–5
Extensions on Windows (#9), store and updates (#10), Clips (#11), Shots (#12). Each issue has its own plan and
"done when".

---

## 7. Open Questions

1. **Thai OCR (#12).** Windows' own OCR may not read Thai. The fallback is bundling Tesseract (`tha+eng`).
2. **Calls with no safe wrapper (#11, #12).** Some Windows calls may have no safe Rust wrapper: overlay windows that don't take focus, keeping Shots' own windows out of captures, global mouse hooks. R29 says these come back to KJ.
3. **Code signing (#10).** Authenticode, or unsigned with one SmartScreen warning.
4. **Default shortcuts (#11, #12).** Ctrl+Shift+C opens browsers' inspector, and Win+V and Win+Shift+S belong to Windows. New defaults are picked in those phases.
