# Devlog 0019 — Samo 0.6.1: Windows' Processes, charts and install

- **Date:** 2026-11-10
- **Asked by:** KJ, using Samo 0.6.0 on Windows:
  - A screenshot of "Samo couldn't be installed: … Access is denied. (os error 5)".
  - Then: "Why not show same as macos like icon on processes, and graph is a bit cut off sometimes?"
  - Then "release 0.6.1".
- **Branches:** #35, #36 and #37, merged into `main`; this release from `main`.
- **The Mac is unchanged (R25).** Every change is Windows-only, or takes the OS as a parameter with the Mac's value as it was.

## Install over a read-only Samo (#35)
- **The cause:** double-clicking `Samo.exe` inside its zip runs a copy Explorer extracts to Temp. Explorer extracts only that file, and marks it read-only.
- `install()` copied it with `fs::copy`, which kept the flag. So KJ's 0.5.0 install was read-only, and every install after it failed with "Access is denied".
- **The fix:** `copy_replacing` clears the flag on the old file before copying, and on the new one after.
- Updates were never affected: they move the old folder to the Recycle Bin.

## Charts that went blank on a busy PC (#36)
- A Windows reading (every process, the disks, the network) takes 0.1–0.3 s when the PC is quiet. With every core busy, it took up to 2.1 s, so samples came about 3.3 s apart.
- The ring buffer called anything over 2 s late a stopped sensor and put a gap before it. A chart needs two points in a row to draw, so the lines vanished for 10–25 s at a time.
- Ruled out first:
  - the collector during a real home scan (66 ticks, worst 310 ms, no gaps);
  - the scan holding Samo's shared lock;
  - thermal reading, which is `None` on Windows.
- **The fix:** a late sample is still measured, since `sysinfo` averages over the time since the last one. Windows' buffer now waits 5 s before calling it a gap (`with_gap_after`). The Mac's buffer is as it was.

## Processes on Windows as on the Mac (#37)
- **Groups:** the processes of one `.exe` roll up into it (`app_of_on`, `group_processes_on`).
- **Names:** an app is named by its `.exe`'s description, as Task Manager does.
- **Applications:** the processes with a window on the taskbar. They count when visible, unowned, titled, not a tool window and not cloaked.
- **Icons:** the `.exe`'s own icon, read from its PE resources in safe Rust (`exe_icon.rs`, R29). A Store app such as Arc has none in its `.exe`, so its package's logo is used instead: the `AppxManifest.xml` names it, and the size just big enough, without a plate, is taken.
- Overview's Top processes shows the icons too.
- **Dependencies, Windows only:** `image` gains its `ico` decoder, and `samo-gpui` gains `winsafe`, which the workspace already had. No new crate.

## Checked
- **On Windows 11 (MSVC):**
  - `scripts/ci-windows.sh` passed on each branch: 542, 544 and 552 tests, 0 failed. It passed again on this release branch.
  - Every new test failed before its fix. #35's failed with KJ's own error. #36's failed with gaps between late samples.
- **On KJ's PC:**
  - With all 12 cores busy for 30 s, the old rule made 10 of 12 samples gaps; the new one made none.
  - Of the 73 programs running, 46 have an icon and 53 a name, read in 0.4 s off the UI thread. Arc, Discord, Explorer, Teams, OpenIDEA, Command Prompt and Search come out right.
  - The taskbar check found Arc, Codex, Teams, Edge, Explorer, OpenIDEA, Glance and Samo, and no background program.
- **GitHub CI:** every PR passed on the Mac and on Windows, and so did `main` with #36 and #37 together.

## Not yet checked by hand (KJ)
- The Processes page and the charts in Samo's own window. Tests here don't open windows on KJ's desktop.

## Found on the way
- Two Windows tests timed out on a busy PC and passed when run again: `windows_limits` and `store_install`'s update test. GitHub passed them each time.
- `gh pr merge` now needs a Heli grant, as `git push` does.
