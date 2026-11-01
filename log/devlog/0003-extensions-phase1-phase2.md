# Devlog 0003 — Extensions, Phase 1 & Phase 2

- **Date:** 2026-11-01
- **Design:** `docs/decisions/ADR-002-samo-extensions-design.md` (accepted 2026-10-31).
- **Plan:** `docs/samo/2026-11-01-samo-extensions-p1-p2-plan.md` (workspace), all 16 tasks.
- **Summary:**
  - Phase 1, the host: a non-activating pop-up window; `samo-ext-protocol` (manifest, protocol v1, widgets);
    `samo-domain::extensions` (R5 entry checks, R8 backoff and rate gates); `samo-extensions` (registry, history,
    dev install, the supervisor with R8's limits); `extensions/hello`; the Extensions page, extension pages, the
    `!N` beside the menu bar figure and quick-panel lines.
  - Phase 2, Claude Sessions (`extensions/claude-sessions`, binary `samo-claude`): reversible settings edits
    (R7), the silent hook (R9), session states and liveness (R10), today's tokens, plan limits, `serve`,
    `on`/`off`/`uninstall`.
  - The `network` xtask gate (R6), and three `UnsupportedReason`s (`SourceNotFound`, `FormatChanged`,
    `NotReported`).
- **Verification:**
  - `scripts/ci-local.sh` (fmt, clippy -D warnings, xtask verify with the new `network` gate and every test).
  - `phase2_end_to_end_demo`: the real host runs the real helper from a dev folder; two stand-ins for Claude Code
    run the exact hook commands from `settings.json` through `/bin/sh`. Connect, a permission wait shown in under a
    second with its badge and pop-up, a finished pop-up, a killed terminal's session gone, exact tokens, plan
    limits and a refused foreign status line, disconnect and off/on restoring the settings to equal JSON.
  - Check M1: with Finder in front, pop-ups were on screen in 37 of 40 samples and Samo never became frontmost.
  - Phase 1 by hand: Hello's page renders every widget; five kills of its helper leave it Stopped with a Restart.
  - Idle CPU over three 60 s rounds: `main` 2.6–4.7 %, this branch 2.7–4.7 %, with Hello on 2.6–2.9 %.
  - Tokens against real transcripts: equal to an independent count (263,106,505) once caught up.
- **Rulings** (beyond the plan's decisions):
  - `HostMsg::Hello` carries no settings: the helper owns its switches.
  - The quick panel shows a line for every extension that is on, not only those with a badge, so it doesn't
    change height while open.
  - `settings_io`'s fingerprint is the bytes read, not a SHA-256.
  - `samo-claude` is a library plus thin binaries, so its pure modules are tested directly.
  - `HostConfig::home` overrides the `HOME` given to helpers, so tests never touch the real `~/.claude`.
  - `SAMO_START_PAGE` also takes an installed extension's id.
  - `scripts/ext-dev.sh` lays an extension out for `SAMO_EXT_DEV_DIR`; it replaces only the files it writes.
- **Found and fixed on the way:**
  - A turn's start was lost when a session waited for you mid-turn; a turn now runs from the prompt to done.
  - KJ's transcripts touched this morning held 724 MB, mostly old history. New files are binary-searched for
    where today starts, and the figure says "Counting today's transcripts…" while the reader catches up (about
    45 s from a cold start at the 8 MiB/s cap, 0.8 s of CPU).
- **Gate files touched:** `xtask/src/{main,layers,purity,network}.rs` (KJ's under CODEOWNERS).
- **Not done here:** Phase 3 (catalog, signatures, downloads, Remove) and check M2 (real Claude Code sessions).
