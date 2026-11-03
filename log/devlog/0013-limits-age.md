# Devlog 0013 — Claude Sessions 0.2.2: how old a limit is, and a limit reached anywhere

- **Date:** 2026-11-04
- **Asked by:** KJ, "% of five h not update... or stuck", then "is update, when using terminal only?" KJ chose:
  show the age, and catch limits reached from the transcripts. Asking Anthropic for usage (online, with KJ's login)
  was declined: Claude Sessions keeps its "never goes online" promise.
- **Branch:** `fix/limits-age` from `main` (Samo 0.4.1). Store-only: Samo is unchanged.

## What was wrong
Claude Code reports plan limits only to a status line, which only a terminal runs. On KJ's Mac that night, three
sessions ran in Cursor's Claude Code extension and one as `claude -p`; only this session was in a terminal. From
22:29 to 00:30 the page showed the 5-hour window at 4% with "as of 22:29", while the real figure reached 34%. The
next terminal reply updated it. Nothing was broken, but an old figure looked current.

The transcripts hold no running percentage. Every session, terminal or not, writes a refusal when it reaches a limit:
an assistant line with `apiErrorParams.rate_limit_info` saying `status: rejected`, `rateLimitType` and `resetsAt`.
KJ's transcripts had 96 of them, all for `five_hour`. The one `allowed` record with a percentage was inside a saved
tool output, not a transcript line.

## Changes
- **Age:** past 10 minutes, a limit's note says how old it is: "resets 02:50 · as of Tue 22:29, 2 h ago".
- **Limits reached anywhere:**
  - The transcript reader notes the latest refusal for the 5-hour and weekly windows. Other kinds, like
    `seven_day_opus`, are left alone. The refusals are kept across midnight, since a window can span it.
  - The page and the menu bar's 5-hour bar show a refusal as 100%: "limit reached at 01:12 · resets 02:50". This
    lasts until a later status-line report or the reset.
  - It shows with no status-line report at all, too.
- **The note under the limits** names what doesn't report: Cursor, VS Code, the desktop app and `claude -p`.

## Checked
- **Unit tests:**
  - refusals are noted latest first, and an escaped mention inside a string is ignored;
  - a later report wins over a refusal, and a refusal past its reset is forgotten;
  - a refusal in a transcript reaches the reader and stays across midnight;
  - the page's notes for an old figure and for a limit reached.
  The golden view changed only in the note under the limits.
- **KJ's real data:**
  - A scratch test, not kept, read all 96 refusal lines in KJ's transcripts with the new parser.
  - The release helper on KJ's real data, with KJ's limits backdated 2 h 5 min in a temporary copy, showed
    "resets 02:50 · as of Tue 22:29, 2 h ago" and "resets Mon 21:00 · as of Tue 22:29, 2 h ago".
