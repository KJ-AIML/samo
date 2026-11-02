# Devlog 0012 — Samo 0.4.1 and Claude Sessions 0.2.1: true CPU, and limits that hold

- **Date:** 2026-11-02
- **Asked by:** KJ, with two screenshots at 22:13. Claude Sessions was stopped ("It used 20% of a CPU core for a
  minute (the limit is 10%)") and needed Restart, often. After a restart the 5-hour limit said it had reset at 19:40
  and was waiting, and the weekly one said "47%, resets 22:00 · as of 17:30". KJ asked that it never need a restart.
- **Branch:** `fix/ext-cpu` from `main` (Samo 0.4.0).

## What was wrong
1. **Samo measured a helper's CPU wrongly (since 0.3.0).** KJ's `helper.log` had 13 stops that day, from 12:47 (after
   0.3.0 was installed) on, for 20% to 105%. `ps` showed the helper used 3.65 s of CPU in its first minute: about 6%.
   sysinfo works a process's CPU use out from the time since its `System` last refreshed *any* process. The tree
   check (R15, ADR-004) refreshes every process every 2 s on the supervisor's `System`, so the CPU check every 10 s
   divided 10 s of CPU time by the under 2 s since, which is 5 to 16 times too much.
2. **A stop for going over a limit was for good.** Even a real one-off spike needed a click on Restart.
3. **The weekly limit disappeared.** At 22:18 Claude Code's status line reported the 5-hour window without the weekly
   one. `limits.json` was replaced as a whole, so the page showed the weekly limit as not reported.
4. **"resets 22:00" didn't say which day.** The weekly limit resets on another day, but at 22:13 the bare time read
   as if it had already passed.

## Changes
- **Samo 0.4.1** (`samo-extensions`):
  - `meter::Meter` measures a helper's CPU and memory on a `sysinfo::System` nothing else refreshes, primed when the
    helper starts.
  - A helper over its CPU or memory limit is stopped, and its reason says when Samo starts it again: "It used 20% of
    a CPU core for a minute (the limit is 10%). Samo starts it again at 22:31." It starts again after 1 minute, then
    2. The third time within an hour it stays stopped: "It went over its limits 3 times in 60 minutes. Last time it
    used …". Restart still starts it at once. ADR-002's R8 and point 3 say so.
- **Claude Sessions 0.2.1:**
  - The status line keeps a window that a report leaves out, from the last `limits.json`, until it resets. A kept
    window has its own `observed_at_ms`, so it says "as of" when it was reported. Old `limits.json` files read as
    before.
  - Times on the page say the day when it isn't today: "resets Thu 22:00", "as of Sun 23:18", "9 Nov 22:00".
  - A transcript line without a `"usage"` key isn't parsed. Before, it was parsed only to find no usage, so the
    count is the same.

## Checked
- **Host tests:**
  - `cpu_is_measured_truly_beside_tree_checks` runs a helper at about 20% with a 50% limit and the tree checked on
    every pass. It fails on the old code, which read 54% and stopped the helper.
  - `over_its_limit_it_rests_and_starts_again_then_stays_stopped` covers the rest and the restart, a dead helper
    while it rests, and the third stop.
  - The `meter` tests: `yes` reads about 100% beside a tree walk every 50 ms, and `sleep` about 0%.
  - `over_limit_rests_then_stops` covers the policy.
- **Claude Sessions tests:** a report without the weekly window keeps it (unit and status line); the day appears
  only when it isn't today.
- **KJ's real transcripts** (1.16 billion tokens on 2026-11-02, 34 files touched, 1.4 GB), read-only through a
  temporary home. The store's 0.2.0 helper used 4.26 s of CPU in 150 s, and 0.2.1 used 1.97 s. Both reached the
  same total, 1,162,322,586 tokens. Once caught up, the helper uses about 0.2% of a core.

## Found on the way
- The store's `samo.toml` still carried 0.2.0's notes, so Settings → Updates offered 0.3.0 and 0.4.0 with them.
  0.4.1 has its own notes.
