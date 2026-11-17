# Devlog 0020 — Samo 0.6.2: Windows' performance and efficiency cores

- **Date:** 2026-11-17
- **Asked by:** KJ, with a screenshot of Performance on Windows: "Why not auto detect for window?" The Cores card said
  "Windows doesn't say which cores are for performance and which for efficiency". KJ chose "Add gdt-cpus", then "release 0.6.2".
- **Branch:** #39, merged into `main`; this release is cut from `main`.
- **The Mac is unchanged (R25).**

## What was wrong
- Windows does report each core's kind, as its `EfficiencyClass` (`GetLogicalProcessorInformationEx`).
- Samo's safe Windows bindings (`winsafe`) don't wrap that call, and R29 allows no `unsafe`. So the card was left empty, and it blamed Windows.
- KJ's i7-1255U is a hybrid chip: 2 performance cores with two threads each, and 8 efficiency cores.

## Changes
- `gdt-cpus` (`=0.2606.1`, Windows target only) reads the core kinds safely.
  - KJ chose it over an `unsafe` exception and over leaving the card empty.
  - Its own dependencies were already in the lockfile, so the lockfile gains one crate.
- When the collector starts, `samo-windows` maps each logical processor to its core and kind.
  - A core's load is the average of its threads' loads, as the Mac shows a core's load.
  - A CPU with one kind of core (most desktops and servers) shows all its cores under Performance.
- Nothing is guessed (R3, R28). In each of these cases the reading is `SensorUnavailable`, and the card says Windows didn't tell:
  - a kind Windows didn't name;
  - a core whose threads disagree;
  - processors that don't line up with `sysinfo`'s CPUs;
  - a thread with no reading.
- Before the first sample the card says "Waiting for this PC…". The Mac's own string is unchanged.

## Checked
- **Before any Samo code:** a throwaway program read KJ's CPU as 2 P-cores (processors 0–3) and 8 E-cores (4–11). That matches the 12 CPUs `sysinfo` measures.
- **Samo's collector on KJ's PC:** P 2, E 8. At the time the P-cores were at 76% and 80%, the E-cores at 91–99%.
- **Tests, each failing first:**
  - the i7-1255U's threads grouped into cores;
  - a core's load is its threads' average, and a missing thread is unknown, never 0;
  - what Windows didn't say isn't guessed;
  - a CPU of one kind has P-cores only;
  - the collector's core counts and loads always agree.
- `scripts/ci-windows.sh` passed on #39 (558 tests, 0 failed) and on this release branch.
- **GitHub CI:** not run for #39 or this release (`[skip ci]`), to save KJ's GitHub Actions minutes. The change is Windows-only, and everything above was checked on Windows.

## Not yet checked by hand (KJ)
- The Cores card in Samo's window. A trial build of #39 was left for KJ in `release/try-cores`.
