# ADR-005: Clips, a clipboard history extension

**Date:** 2026-11-02
**Status:** Accepted by KJ on 2026-11-02 ("Approved"), with the [call]s as written. The Phase 1 plan stores history
as JSON plus files rather than SQLite (plan decision 1).
**Product:** Samo (`samo`)
**Builds on:** ADR-002 (extensions), ADR-003 (the store) and ADR-004 (extensions with their own windows, Shots).
Their rules R5–R19 still hold. This ADR adds two macOS permissions an extension may ask for, an extension that
watches while idle, and **Clips**: a clipboard history with Maccy's features.

---

## 1. Intent

**[KJ]** KJ uses Maccy and wants its features in Samo, as an extension, like Shots for Shottr.

**[KJ]** Decisions made on 2026-11-02:
- **Pasting:** picking an item pastes it into the app you're in, as Maccy does. This needs macOS Accessibility, for
  Clips alone. Copy-only was not chosen.
- **Shortcut:** Maccy's usual ⇧⌘C. KJ will quit Maccy. ⌥⌘V was not chosen.
- **History:** kept on disk, as Maccy does, in Clips' own data folder; uninstall deletes it. Memory-only was not
  chosen.
- **Code:** our own, in Shots' style, with Maccy for ideas. Maccy is open source (MIT), so reusing its code is
  allowed with its notice, but porting it was not chosen.

**What it is.** A small native helper, like Shots. It notices each copy, keeps a history, and opens a searchable list
at ⇧⌘C. Picking an item pastes it. Samo shows its page: whether macOS lets it read and paste, the latest items, the
pinned ones, and its settings.

**Checked on this Mac before writing.** macOS asks before an app reads what other apps copied: `NSPasteboard`'s
`accessBehavior`, "Paste from Other Apps" in Privacy & Security. So Clips needs that permission, for itself alone, as
Shots needs Screen Recording.

---

## 2. Goals, Non-Goals, Anti-Goals

### Goals
1. **[KJ]** Maccy's everyday features:
   - history of text, rich text, images and files;
   - ⇧⌘C opens a list at the pointer;
   - type to search, ⌘1–⌘9 or Return to paste;
   - pins;
   - a preview with the item's size, when it was copied first and last, and how many times;
   - delete, clear, and paste without formatting;
   - ignoring apps and kinds of content.
2. **[call]** Instant. The list is up within 100 ms of the shortcut. Search follows each keystroke over 1,000 items.
3. **[call]** Samo's look, as Shots has it (DESIGN.md rule 10).

### Non-Goals (v1)
- **[call]** Syncing history between Macs, or iCloud.
- **[call]** Snippets or templates, and editing an item's text.
- **[call]** A menu bar icon of its own. Samo's quick panel line and the shortcut are the ways in.

### Anti-Goals
1. **A secret is kept.** A password copied from a password manager ends up in the history.
2. **The clipboard leaves the Mac.** Anything in Clips goes online, ever.
3. **Clips makes the Mac slower.** It does more than glance at the clipboard while idle, or memory grows with
   history.
4. **It acts without saying.** A paste you didn't choose, or a permission asked with no explanation.
5. **History outlives its owner.** Uninstall leaves the history behind.

---

## 3. Core Features

| # | Feature | Covers (v1) | Does not cover (v1) |
| --- | --- | --- | --- |
| **C1** | **Watch** | Each copy in any app is added, newest first: text, rich text (RTF, HTML), images and file references. A repeated copy moves to the top and counts once more. 200 items by default (up to 1,000), plus pins. | Copies made while Clips is off. |
| **C2** | **List** | ⇧⌘C opens the list at the pointer. Type to search; ↑↓ to move, Return or ⌘1–⌘9 to paste; ⌥Return pastes without formatting. ⌥P pins, ⌥⌫ deletes. A preview beside the selected item shows an image, or the full text, with its size and when it was copied first and last, and how many times. | Regex search. |
| **C3** | **Paste** | Picking puts the item on the clipboard, gives the keyboard back to the app you were in, and presses ⌘V there (Accessibility). Without Accessibility, it copies and says *Copied: press ⌘V*. | Pasting into a chosen app. |
| **C4** | **Keep secrets out** | Never kept: anything marked concealed, transient or auto-generated (the `org.nspasteboard` markers password managers use), and anything copied from the ignored apps. Those are 1Password, Bitwarden, Keychain Access and Passwords by default; you can add more. *Ignore the next copy* and *Pause for 15 minutes* are also there. | Telling a secret apart from text by its content. |
| **C5** | **In Samo** | Its page shows Paste from Other Apps and Accessibility, each allowed or not yet, with a button to System Settings. It lists the latest 20 and the pinned (each with Copy, Pin, Delete), the shortcut and whether it works, and the settings (history size, ignored apps, pause). The quick panel line reads *Clips · 128 items*. | Thumbnails on Samo's page. |

---

## 4. Rules

These continue R5–R19.

| Rule | Protects | Enforcement | Required test |
| --- | --- | --- | --- |
| **R20 — Two more permissions, shown first** | Anti-goal 4 | `permissions` may also hold `paste-from-other-apps` and `accessibility`. The store shows each before install, and macOS asks for them for Clips alone: its worker is its own app, as Shots' is. Clips is signed with Talvyn's identity, so updates keep both. | `manifest_capabilities_parse_and_validate`; manual check |
| **R21 — Watching, and nothing more** | Anti-goal 3 | macOS has no "something was copied" signal, so Clips reads `changeCount` twice a second, as Maccy does. It reads the clipboard only when that changes. Nothing else runs while idle. A declared `watches = "…"` line in the manifest says so before install. Images over 10 MB are kept as files, not in memory. | Swift `IdleIsOneCounterRead`; `declared_limits_enforced` |
| **R22 — Secrets are never kept** | Anti-goal 1 | Concealed, transient and auto-generated markers, and ignored apps, are checked before anything is stored, even in memory. | Swift `ConcealedCopiesAreNeverKept`, `IgnoredAppsAreNeverKept` |
| **R23 — Local only, and gone on uninstall** | Anti-goals 2, 5 | The `network` gate scans Clips. History lives in `data/` (SQLite), protected by FileVault like the rest of the disk. Uninstall deletes `data/`, and the page says so before. | xtask `network`; `uninstall_deletes_history` |
| **R24 — Paste only on a pick** | Anti-goal 4 | ⌘V is pressed only right after you pick an item in Clips' list, into the app that was in front when you opened it. | Swift `PasteOnlyFollowsAPick` |

---

## 5. How It Gets Made

- **[call] The name Clips** (`id = "clips"`, helper `samo-clips`, bundle id `dev.talvyn.samo.clips`). Maccy's name and
  look are not used.
- **[call] A shared Swift kit.** Shots' protocol, worker spawn, theme, fonts and notes move into
  `extensions/kit` (`SamoKit`), a local Swift package that Shots and Clips both depend on. Shots is unchanged
  otherwise.
- **[call] Samo 0.4.0.**
  - The two permissions and the `watches` line, with their words in the store.
  - The `network` and `swift` gates cover every Swift extension, not only Shots.
- **Clips 0.1.0**, `min_samo` 0.4.0, `min_macos` 14.0, limits 300 MB and 20 % CPU.

## 6. Build Order

### Phase 1 — Watch, list, paste
- **Builds:**
  - Samo 0.4.0's manifest additions and the kit.
  - Clips' watcher with its secret filters, the history in SQLite, the list with search and pins, and pasting.
  - Its page in Samo, and its store listing.
- **Done when** KJ, with Clips installed from the store:
  1. Sees *Reads what other apps copy*, *Accessibility* and *Watches the clipboard* before installing.
  2. Allows both when macOS asks, for **Clips**.
  3. Copies text, an image and a file in three apps, presses ⇧⌘C, types to find one, presses Return, and it is pasted.
  4. Copies a password from Passwords or 1Password, and it isn't in the history.
  5. Restarts the Mac, and the history and pins are still there. Uninstalls, and they're gone.
  6. Sees idle CPU unchanged with Clips on.

### Phase 2 — The rest of Maccy
- **Builds:** the preview pane, paste without formatting, ignoring apps from the page, pause and ignore-next, sort
  orders, and a history of up to 1,000.
- **Done when** each of C2–C4's items works by hand, and 1,000 items search as you type.

---

## 7. Open Questions
None. KJ answered four on 2026-11-02 (§1).
