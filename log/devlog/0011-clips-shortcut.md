# Devlog 0011 — Clips 0.1.1: change the shortcut

- **Date:** 2026-11-02
- **Asked by:** KJ, after installing Clips 0.1.0: "why not Hold Shift and Command, then tap V". KJ chose ⇧⌘V on
  their Mac now, and a way to change the shortcut in Clips 0.1.1. New installs keep Maccy's ⇧⌘C.
- **Branch:** `feat/clips-shortcut` from `main` (Samo 0.4.0). Store-only: Samo is unchanged.

## What was wrong first
KJ reported that ⇧⌘C showed nothing. On their Mac, the installed Clips opened its list on a synthetic ⇧⌘C and kept it
open until Esc. Nothing was taking the key: no secure keyboard input, no event tap that filters keys, no remapping,
ABC layout. A first guess (the list closing as it lost the keyboard) was wrong: the early closes in testing were
KJ's own trackpad clicks, which close the list as they should. KJ's next screenshot showed the list at ⇧⌘C. No change
to the list.

## Clips 0.1.1
- **Change shortcut…** on Clips' page opens Clips' own small window (declared in the manifest's `windows`, R15),
  like Shots' settings: click the button, then press the keys. A shortcut needs ⌘, ⌃ or ⌥; ⌫ leaves Clips without
  one; Esc keeps the one it has. The shortcut is let go while recording, so pressing it records it instead of
  opening the list. A new shortcut is saved in `data/settings.json` and used at once; one macOS refuses shows on the
  page as before (R18).
- The page says the shortcut you chose, not always ⇧⌘C ("Copy something, then press ⇧⌘V."). With none, it says the
  list can't open and points to Change shortcut….
- The manifest says "its shortcut (⇧⌘C unless you change it)" and names the new window.

## Checked
- 24 Swift tests (the page with ⇧⌘V and with none; a chosen shortcut kept in settings). `xtask verify` passes; the
  Phase 1 demo expects the new "No shortcut" wording.
- By hand on a test copy (its own data folder, ⇧⌘J): Change shortcut… opened the window; recording ⇧⌘K saved it and
  ⇧⌘K opened the list at once; Esc while recording kept ⇧⌘K; ⌫ left none; the page followed each change.
