# Devlog 0010 — Clips, Phase 1: watch, list, paste

- **Date:** 2026-11-02
- **Design:** ADR-005, accepted by KJ ("Approved"), with KJ's answers: paste for you (Accessibility), Maccy's
  ⇧⌘C, history on disk, and our own code with Maccy for ideas. Plan: `docs/samo/2026-11-02-samo-clips-p1-plan.md`,
  approved, one commit per task, on `feat/clips` from `main` (Samo 0.3.0).

## Samo 0.4.0
- A manifest's `permissions` may also be `paste-from-other-apps` and `accessibility`, and it may say what it
  `watches` while idle. The catalog carries `watches`, an install refuses a package that differs, and the store and
  an extension's card say all of it before install (R20, R21).
- **SamoKit** (`extensions/kit`) is what Swift extensions share:
  - the protocol and its channel, and the worker started as its own app;
  - the session, which now sends the page again whenever Samo shows it;
  - the theme, fonts and notes;
  - bringing a window forward, global shortcuts, and System Settings' privacy panes.

  Shots depends on it and is otherwise unchanged: 95 of its tests stay in Shots, 4 moved to the kit, and its four
  demos pass.
- **Gate change (approved with ADR-005):** a URL may be built from text only in a file named `Links.swift`.

## Clips 0.1.0
- **Watching (R21, R22):** one counter read every 0.5 s. When it changed, Clips reads the clipboard's kinds. Only if
  they pass does it read the contents, so secrets never reach memory:
  - no nspasteboard.org concealed, transient or auto-generated marker, and none of the password managers' own;
  - not copied while 1Password, Bitwarden, Keychain Access or Passwords was in front.

  It reads files, images, rich text with its plain text, or text. It skips its own pastes, and can ignore the next
  copy or pause for 15 minutes.
- **History (R23):** 200 by default (50 or 500 on the page), newest first; a repeat moves up and counts again, and
  pins stay. It lives in `data/history.json` plus `items/` for rich text, images and long text, written atomically.
  Long text is cut between characters in the list, never through a Thai character.
- **The list:**
  - ⇧⌘C opens it at the pointer, on the readout's glass, with search (every word, any order, case and accents
    aside).
  - Return or ⌘1–⌘9 pastes, ⌥Return pastes without formatting, ⌥P pins, ⌥⌫ deletes, Esc or a click elsewhere
    closes it.
  - Rows show the kind or a thumbnail, the first line, a pin and ⌘N.
- **Pasting (R24):** Clips remembers the app in front when the list opens. A pick puts the item on the clipboard as
  Clips' own copy, brings that app back, and presses ⌘V only once it is in front (within 0.6 s). Without
  Accessibility, the note says "Copied: press ⌘V".
- **Its page in Samo:**
  - both permissions, with ways to System Settings;
  - the pinned and the latest 20, with Copy, Pin and Delete, and the app each came from;
  - ⇧⌘C with its lamp, reporting Maccy, Paste, Raycast or Alfred running;
  - what it keeps, pause, ignore-next, and clear (asked twice);
  - the quick panel line "Clips · N items".

## Rulings
1. **History is JSON plus files, not SQLite** (plan decision 1).
2. **The list is a key window brought forward,** as Shots' editor is.
3. **Fixed during the build:**
   - Cutting long text at 64 KB could split a Thai character; it now cuts between characters.
   - A host test (`off_runs_helper_off_then_stops`) read the history line before it was written under load; it now
     waits for it.
   - A re-copied pin stays where it was pinned.
4. **Pasting presses the V key's code on an ANSI keyboard.** Other layouts may need it looked up; to check by hand.

## Verification
- See the Phase 1 demo commit, and KJ's checks (M1–M6, ADR-005 §6).
