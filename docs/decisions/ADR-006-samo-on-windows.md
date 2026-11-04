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
| A helper in its own process group, its tree found by parent pid (R15, R17) | A Job Object closed with kill-on-close (`win32job`): the family is the job's own list, and it ends with Samo |
| The window server's window list (R15) | The visible top-level windows and their owners (`EnumWindows`, through `winsafe`) |
| `current`, a symlink to the version in use | `current`, an NTFS junction (`junction`; no Developer Mode) |
| A helper `bin/<name>` | `bin/<name>.exe`; manifests name it once, without `.exe` |
| Swift extensions (Shots, Clips) | Rust rewrites (#11, #12) |

---

## 4. Rules

These continue R1–R24.

| Rule | Protects | Enforcement | Required test |
| --- | --- | --- | --- |
| **R25 — The Mac is unchanged** | Anti-goal 1 | Windows code is behind `cfg(target_os = "windows")`, or is platform-neutral and tested on both. Rules that differ take the OS as a parameter (`is_protected_path_on`, `scan_root_on`), so both branches are tested on every CI. The macOS CI jobs run as before. One deliberate change on the Mac: a `//`-prefixed path is now protected from cleanup, since the Mac resolves `//System` to `/System`. | macOS CI (`check`, `db`); `on_the_mac_drive_paths_stay_protected`, `scan_roots_follow_the_os` |
| **R26 — One shape for Windows paths** | Anti-goal 2 | **Normalising.** `samo_domain::paths::normalize_path` gives Windows paths `/` separators, an upper-case drive, and no trailing `/` except on the drive root (`C:/`). A `/`-rooted path is never changed, since a Mac name may contain `\`. **On Windows,** only a drive path in that shape is a scan root or can be trashed. A `/`-rooted path there is on the current drive, so it fails closed, as do relative, UNC and `C:foo` paths. **On the Mac,** a drive path is a relative name and fails closed, as it always did. | `normalize_keeps_mac_paths_and_shapes_windows_ones`, `scan_roots_follow_the_os` |
| **R27 — R2 on Windows** | Anti-goal 2 | **Protected, in any case:** drive roots; `Windows`, `Program Files`, `Program Files (x86)` and `ProgramData`, with everything under them; `Users` and `Documents and Settings`, and each `<name>` under them; `$Recycle.Bin` anywhere; `System Volume Information`, `Recovery`, `Boot`, `bootmgr`, `pagefile.sys`, `hiberfil.sys` and `swapfile.sys`. **Also protected:** a name Windows resolves to somewhere else, so it can't be judged by how it reads. That means `.` and `..`, a trailing dot or space, a stream (`name:stream`), and an 8.3 short name (`PROGRA~1`). **Unchanged:** the identity check. | `windows_system_paths_are_protected_in_any_case`, `windows_lookalike_forms_fail_closed` |
| **R28 — Said, never faked** | Anti-goals 2, 4 | A reading Windows can't give is `Unsupported` (R3). A feature whose Windows mechanism isn't built yet stays off on Windows, and its page says so. | Per phase |
| **R29 — No `unsafe` for Windows** | Anti-goal 3 | `unsafe_code = "forbid"` stays for every crate. Windows APIs are reached only through safe crates (`sysinfo`, `trash`, `winapi-util`, `filesize`, `windows-registry`, `tray-icon`, GPUI; for extensions `win32job`, `winsafe` and `junction`) and Windows' own tools run by full path (`taskkill`, `mountvol`). A need with no safe wrapper comes back to KJ as a decision. | The compiler |
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
- **[call] Extensions run on Windows from #9; the store and updates stay off until #10.** Their pages say so,
  and nothing is fetched (R28).
- **[call] On Windows a helper's family is its Job Object.** Windows keeps an orphan's old parent pid and reuses
  pids quickly, so walking parent pids could take another process for the helper's child. The walk, with the rule
  that a child never started before its parent, is only the fallback for a helper that can't be put in a job. The
  Mac's walk is unchanged.
- **[call] R15 on Windows counts visible windows.** Every console helper has a hidden console host (`conhost.exe`)
  in its family; a hidden window shown later is seen at the next check, 2 s on.

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
  - **Built.** R2 is checked on the canonical path: the folder is resolved and the item keeps its own name.
  - **The Recycle Bin** goes through the `trash` crate, on a thread of its own.
  - **What Windows would delete for good is refused before anything moves.**
    - Cleanup runs only on a fixed NTFS volume that Samo can see, and only where its Recycle Bin is on (`NukeOnDelete`)
      and holds the item (`MaxCapacity`). Samo reads both from the volume's own settings.
    - Anything Samo can't tell is a no: other file systems (FAT has no stable file index, ReFS no unique 64-bit
      one), removable or unlisted drives, and settings it can't read.
  - **The moved item is found again by its identity**, not by its shown name (Windows hides known extensions), so
    Undo brings back that item under its real name.
  - **Quit** ends only the verified process, never its tree.
- **PR 3, the app:**
  - the tray and quick panel
  - keeps running after its window closes, as one instance
  - the native titlebar
  - Ctrl shortcuts, fonts, Windows wording
  - the DPI manifest
  - **Built:**
    - A window that is never shown keeps Samo running in the tray, because GPUI quits on the last window close.
    - A lock file and a show request keep it to one Samo.
    - Roots take the scanner's shape, so scans finish.
    - Extensions, the store and updates say they come later (R28).
    - The whole workspace builds and tests on Windows.
  - The window and taskbar icon moves to the installer (#10). The tray already shows Samo's own probe.
- **Done when** on this PC:
  1. `cargo build --release -p samo-gpui` builds.
  2. Samo opens, scans a folder and shows live telemetry.
  3. It moves a folder to the Recycle Bin and Undo brings it back.
  4. It quits a process only after checking it.
  5. It stays in the tray after its window closes.
  6. `cargo test --workspace` passes on Windows, and the macOS jobs pass unchanged.

### Phase 2 — Extensions on Windows (#9)
- **PR A, the extension host. Built:**
  - Extensions live in `%LOCALAPPDATA%\Samo\Extensions`. A helper is `<helper>.exe`, started with no console
    window and Windows' own environment (its folders, shell and temporary folder) and nothing else of Samo's.
  - `current` is a junction, swapped so it never points at a removed version. A reinstall keeps the old copy
    until the new one is in place, and a hard-linked file is refused (R5).
  - A helper's family is its Job Object: it ends when the helper stops, and when Samo ends, however Samo ends.
  - R8's limits, the crash backoff and R15 work as on the Mac. The host's tests use one Rust fake helper, so they
    run on both.
  - The app starts the host. The Extensions page lists what runs from `SAMO_EXT_DEV_DIR` and says the store comes
    later.
  - `cargo run -p xtask -- ext-dev <folder>` lays out an extension on either OS.
- **PR B, Claude Sessions. Built:**
  - It reads `%USERPROFILE%\.claude` and keeps its data in `%LOCALAPPDATA%`.
  - **Hooks** use Claude Code's exec form (`command` + `args`, CLI 2.1.139+): Claude Code runs Samo's helper with no
    shell, so it makes no difference whether Git Bash is installed.
  - **The status line** has no exec form; Claude Code runs it through Git Bash, or PowerShell without Git Bash. Its
    command is the helper's path with forward slashes and no quotes, which both shells run, if the path holds only
    letters, digits and `. _ - / : ~ +`; else `~/…` from the home folder if that does. A space, `'`, `&`, `(` or
    `$` would break one shell or the other, so otherwise Plan limits says why and writes nothing.
  - Samo's entries are recognised by the helper's own path as well, so Disconnect finds them wherever
    `SAMO_EXT_ROOT` keeps extensions (R7).
  - **One guarantee is weaker than on the Mac.** The Mac's hook command checks that the helper is there, so a hook
    left behind does nothing. An exec-form hook has no such check: if a dev build is uninstalled "anyway" (it can't
    run to undo its change) or its folder is deleted by hand, Claude Code reports a hook error on each event until
    the hooks are taken out. The uninstall message says so on Windows.
  - A hook finds Claude Code above Git Bash's two `bash.exe`, `pwsh.exe`, `powershell.exe` or `cmd.exe`. On
    Windows a "parent" that started after its child is a reused pid, and is skipped; the Mac's chain is as before.
  - **Checked on this PC:**
    - The release app runs Claude Sessions and Hello from `SAMO_EXT_DEV_DIR`, as children of Samo, and lists them
      in the sidebar. Force-killing Samo ended both and their console hosts.
    - The installed hook, fed a real UserPromptSubmit event, recorded the session as working and found the
      `claude.exe` running it. It exits 0, silently, in about 30 ms (R9).
    - Connect and Disconnect bring a copy of the real `settings.json` (with other tools' hooks and status line)
      back byte for byte, and Plan limits is refused there (an ignored test, `real_settings`).
  - **Left for KJ's hand check**, as it needs clicks and his live Claude Code settings: the Claude Sessions page
    showing sessions working, waiting and done with today's tokens, and Connect then Disconnect on the live file.

### Phases 3–5
Store and updates (#10), Clips (#11), Shots (#12). Each issue has its own plan and "done when".

---

## 7. Open Questions

1. **Thai OCR (#12).** Windows' own OCR may not read Thai. The fallback is bundling Tesseract (`tha+eng`).
2. **Calls with no safe wrapper (#11, #12).** Some Windows calls may have no safe Rust wrapper: overlay windows that don't take focus, keeping Shots' own windows out of captures, global mouse hooks. R29 says these come back to KJ.
3. **Code signing (#10).** Authenticode, or unsigned with one SmartScreen warning.
4. **Default shortcuts (#11, #12).** Ctrl+Shift+C opens browsers' inspector, and Win+V and Win+Shift+S belong to Windows. New defaults are picked in those phases.
