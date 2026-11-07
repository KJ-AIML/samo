# Devlog 0016 — Claude Sessions 0.2.4: no 5-hour limit, and Ask Anthropic for limits

- **Date:** 2026-11-06
- **Asked by:** KJ, "there is bug again is not show session 5h I open claude or other is still not show". The 5-HOUR
  meter said `--`, "not reported by Claude Code yet", beside WEEKLY 94% "as of 22:43".
- **KJ chose:** "Both" (fix the wording, and add the online fetch as a switch that stays off until turned on). Then,
  told that Anthropic's usage page has no 5-hour figure either, "Still add online fetch". This reverses devlog
  0013's "declined" and ADR-002's [call] against Glance's usage check; ADR-002 is amended (C3 item 6, R6).
- **Branch:** `fix/no-five-hour-limit` from `main` (`ace66a3`). Store-only: Samo is unchanged.

## What was wrong
Nothing in Samo dropped the figure. Anthropic sent none:

- `data/limits.json` after Claude Code's 22:47 report: `"five_hour": null`, weekly 94%. The status line only saves
  what Claude Code gives it.
- Claude Code 2.1.317 fills `rate_limits.five_hour` only from the API's `anthropic-ratelimit-unified-5h-*` response
  headers ("present only while the API reports it").
- One small `claude -p --model haiku` with `ANTHROPIC_LOG=debug`: the answer had `7d-utilization: 0.95`,
  `7d-status: allowed_warning`, `representative-claim: seven_day`, and no `5h-*` header at all.
- `GET /api/oauth/usage` with Claude Code's sign-in: `"five_hour": null`; its `limits` list held only weekly rows
  (all models 95%, Fable 15%).

Near the weekly limit, Anthropic applies only the weekly one. The page said "not reported by Claude Code yet",
which was wrong: Claude Code had just reported.

## Changes
- **No 5-hour limit, said so.** A report that leaves a window out means Anthropic applies none right now, so the
  meter says "no 5-hour limit right now · as of 22:47" (or "no weekly limit"). Only a limit reached in a transcript,
  with no report, still says "not reported by Claude Code yet". The menu bar's bar shows the weekly ("wk 94%")
  while there is no 5-hour window.
- **Ask Anthropic for limits** (`online.rs`), a new switch, off by default and declared in the manifest:
  - Every 5 minutes (15 after a failure) `serve` asks `https://api.anthropic.com/api/oauth/usage`, on its own
    thread, and saves the answer to `limits.json` as the status line does.
  - The sign-in is read as Claude Code reads it (`security find-generic-password -a $USER -w -s "Claude
    Code-credentials"`, else `~/.claude/.credentials.json`). It is never refreshed, written or kept. A token that
    expires within a minute isn't sent: refreshing it ourselves would rotate Claude Code's refresh token and could
    sign it out. A sign-in Anthropic refused isn't sent again until Claude Code has a new one.
  - HTTPS only, no redirects, 15 s and 256 KiB at most.
  - A `null` window is "none right now". A missing or unreadable window sets the whole answer aside, so nothing is
    made `0` or *none* (R10).
  - The page says when it last asked and what went wrong: no sign-in, expired, refused, or unreachable, with when it
    tries again.
- **Gate:** `samo-claude` may list `ureq`. In Claude Sessions, `ureq` may appear only in `src/online.rs`, and that
  file's shipped code may name no other `http(s)://` address. `std::net` stays banned.
- `prefs.json` gains `ask_anthropic` (`serde(default)`, so older files still read). The status line's writer is now
  `statusline::save`, shared with `serve`.
- Docs: ADR-002 amended, plus README "Safety and privacy", the README's Claude Sessions list and AGENTS.md.

## Checked
- **Unit tests:**
  - a report without a window says there is none, and a limit reached with no report doesn't;
  - the menu bar falls back to the weekly;
  - the notes for asking, answered, expired and failed;
  - KJ's real usage answer (trimmed) is read, and an odd one is set aside;
  - the sign-in is read, an expired one is refused, and the token never shows in a debug print;
  - old `prefs.json` files still read.
  The golden view changed in the weekly's note ("no weekly limit right now") and the new switch.
- **Gate tests:** `ureq` is allowed in `samo-claude` but not in `serve.rs`, there is no other address in
  `online.rs`, and its tests don't ship.
- **Live, on KJ's Mac:** the debug `samo-claude serve` with the switch on, in a scratch data folder. With no
  Keychain prompt it saved `{"five_hour":null,"seven_day":{"used_percentage":96.0,…}}`. The page showed
  "no 5-hour limit right now · as of 23:26" and "resets Sun 22:00 · as of 23:26", and the badge showed `wk 96%`.
- `cargo run -p xtask -- verify` passes.
