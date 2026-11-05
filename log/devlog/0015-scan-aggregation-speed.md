# Devlog 0015 — Samo 0.5.1: a home folder's scan finishes

- **Date:** 2026-11-05
- **Asked by:** KJ, from Samo 0.5.0 on Windows: "why this take measuring around one h and not finish?" Then "release
  0.5.1".
- **Branches:** `fix/scan-aggregation-speed` (#28), merged into `main`; this release from `main`.

## What was wrong
Measuring KJ's home folder sat at "Samo is measuring" for over an hour. Disk free and Reclaimable showed "--".

The walk was not the slow part:
- 1.79 M files took about 7 minutes.
- After the walk, Samo ran at about one core (116 % CPU) and read nothing from disk.

The slow part was working out the projects, in `samo_domain::aggregate_scan_entries`, on both OSes:
- Each project marker was compared with every other marker, to keep only the outermost ones.
- Each file was compared with every outermost project, to find its owner.
- Every comparison allocated a `String`.

A developer's home holds about 10,000 markers, since every package in a `node_modules` has a `package.json`.
At that size the two loops take about an hour. A probe measured ×4 time per doubling: 1.3 s, 5.3 s, 21.4 s.

## Changes
- **Lookups instead of comparisons.**
  - `enclosing(path)` lists a path's own ancestors, without allocating.
  - Both loops look those up in the project map.
  - They list exactly the ancestors the old `is_subpath_of` accepted. A test checks the two agree on every pair of 17
    paths: `p1` and `p10`, `C:/` and `/`, a trailing `/`, `//`, and the empty path.
- **Same results.** Outermost projects never nest, so a file still has at most one owner. Artifact attribution
  (`target`, `node_modules`, …) is unchanged.

## Checked
- **Tests:**
  - A file belongs to its project, not to one sharing a prefix (`p1` and `p10`).
  - A project at a drive's root owns what is under it.
  - `aggregation_grows_with_files_not_files_times_projects`: 1,500 projects and 60,000 files in under 5 s. It took
    18.1 s before the fix.
- **At the real size:** a probe of 1.8 M entries and 10,300 projects, each with a nested marker, aggregates in 4.5 s in
  release.
- **KJ's own home folder:** `samo-cli scan` with the fix measures 1.71 M files (178 GB) and lists its projects and
  their reclaimable space in 356 s, start to finish, the walk included. Samo 0.5.0 hadn't finished after an hour.
- **On Windows 11 (MSVC):** `scripts/ci-windows.sh` passes on the release branch (fmt, clippy `-D warnings`, the static
  gates and the workspace's 499 tests).
- **GitHub CI:** #28 passed on Windows. Its Mac job failed once, on a Shots timing test unrelated to the change. `main`
  passed on both OSes after the merge.

## Found on the way
Two Mac-only tests failed once each on GitHub and passed on the next run:
- `shots_phase3_demo` sometimes still counts one Shots process right after Shots is turned off.
- The Swift test `testStitchingIsQuickEnough` took 1.11 s against its 1 s limit for debug builds on the Mac VM.

They're left for the Mac.
