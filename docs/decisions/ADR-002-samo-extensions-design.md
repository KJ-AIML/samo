# ADR-002: Samo Extensions, with Claude Sessions as the first one

**Date:** 2026-10-31
**Status:** Accepted by KJ on 2026-10-31 ("ok"), with the [call]s as written
**Product:** Samo (`samo`)
**Changes:** ADR-001 §3 S6 ("does not cover: plugin widgets") and the README's "Local only … nothing sent anywhere".

---

## 1. Intent

**[KJ]** KJ uses a teammate's app, [Glance](https://github.com/whoisnaisu/glance), and loves how it works. Glance shows
each Claude Code session as *working*, *waiting on you* or *done*, pops up when a session needs you or finishes, and
reads the sessions for today's tokens and the plan limits. KJ wants the same thing in Samo.

**[KJ]** It comes as an extension that people download and install into Samo. Not everyone needs it, so it is optional.

**[KJ]** A real downloadable extension, chosen on 2026-10-31 over "built into Samo, off by default" and "built in
first, extension later".

**What it is:**
- **A small extension system.** Samo can list, download, install, turn on and off, update and remove extensions. An
  extension is a package: a manifest and a helper program. Samo runs the helper as a separate process and draws what
  the helper reports with Samo's own components, so an extension looks like Samo.
- **Claude Sessions, the first extension.** Session states, pop-ups, today's tokens and plan limits, inside Samo and
  beside its menu bar item.

**What it is not:**
- **Not a port of Glance.** Glance is closed source: its license is "All rights reserved", with no modified versions,
  no reverse engineering and no hosting it elsewhere. Samo uses none of Glance's code or files. Everything here is
  built on Claude Code's documented hooks and status line. **[call]** Tell whoisnaisu before Claude Sessions ships,
  and credit Glance as the inspiration in the extension's README.
- **Not an open marketplace in v1.** Only extensions in Samo's signed catalog install, plus local builds for
  development.
- **Not a remote control for Claude Code.** Claude Sessions watches. It never answers a question, approves a
  permission or sends input.
- **Not a reason for Samo to be online.** Samo uses the network only when you press a button.

**Why it is new rather than a reuse:**
- Samo has no extension mechanism; ADR-001 S6 excluded plugin widgets.
- Glance can't be reused: it is closed and is a separate Tauri app.
- Rust plugins loaded as `.dylib` are ruled out. The workspace forbids `unsafe`, which loading a library needs, and
  Rust has no stable ABI.

**Who it's for:** **[KJ]** KJ, and teammates who run Samo and Claude Code. Anyone else never downloads code they
didn't ask for.

---

## 2. Goals, Non-Goals, Anti-Goals

### Goals
1. **[KJ]** See every Claude Code session's state at a glance, and be told when one needs you or finishes.
2. **[KJ]** See today's tokens (total and per session) and the plan limits.
3. **[KJ]** Optional: download and install it only if you want it. Without it, nothing extra runs.
4. **[call]** The mechanism is general. A second extension needs a catalog entry, not new host code.

### Non-Goals (v1)
- **[call]** Third-party extensions, or installing from an arbitrary URL.
- **[call]** Extensions that draw their own UI (web views, their own GPUI code). They get a fixed set of Samo
  components.
- **[call]** Background update checks. Updates are found when you press **Check**.
- **[call]** Glance's network usage check (`api.anthropic.com/api/oauth/usage`) and reading Claude Code's login.
- **[call]** Jumping from a pop-up to the session's terminal or editor window.
- **[call]** Session history beyond today. Windows and Linux.
- **[call]** Claude Code installs that move their config with `CLAUDE_CONFIG_DIR`. Connect writes `~/.claude/settings.json`.

### Anti-Goals
These are **[call]**s until KJ says what would make the product a failure (open question 1).
1. **An extension makes Samo slow, hot or crash.** (Extends ADR-001 anti-goal 1.)
2. **Code nobody vetted runs on the Mac.** A tampered or swapped download gets executed.
3. **An extension changes the Mac and it can't be fully undone.** Removing it leaves hooks behind, or it overwrote a
   status line or setting you had.
4. **Claude Sessions gets in Claude Code's way.** It slows Claude Code down, blocks it, or changes what Claude sees.
5. **Samo lies about a session.** It says *working* for a session that died, or `0` tokens when it couldn't read
   them. (Extends ADR-001 anti-goal 3.)
6. **Samo goes online without being asked.**

---

## 3. Core Features

| # | Feature | Covers (v1) | Does not cover (v1) |
| --- | --- | --- | --- |
| **E1** | **Extensions page** | Installed extensions and their state: running, off, stopped (crashed, with the reason), update available. **Check for extensions** fetches the catalog and lists what can be installed: name, one line, version, download size, and what it changes on your Mac. Install, a face switch for on/off, Update, Remove. | Search, ratings, other sources, automatic updates. |
| **E2** | **Install and trust** | A signed catalog; each package's SHA-256 pinned in it; safe unpacking; what a package changes is shown before you install it; install and update are atomic; Remove undoes the changes first. A developer folder for local builds. | Apple notarisation; a per-extension sandbox (**[call]** not needed while only KJ's own extensions install). |
| **E3** | **Extension host** | Runs the helper while the extension is on and Samo is running; JSON lines over stdin/stdout; a fixed widget set that Samo draws; a sidebar entry; a badge beside the menu bar figure; pop-ups; restart with backoff; stop after repeated crashes or excess CPU/memory. | Extensions talking to each other; custom drawing; settings beyond a declared list of switches. |
| **C1** | **Claude Sessions: states** | Per session: project (the folder name of `cwd`), state (*working*, *waiting on you*, *done*), why it waits (permission for a tool, a question, a plan to review, input for an MCP server), since when. `!N` beside the menu bar figure while N sessions wait. A pop-up when a session starts waiting and when a turn finishes. | Approving or answering from Samo; jumping to the terminal; subagent detail. |
| **C2** | **Claude Sessions: tokens** | Today's tokens, total and per session, read from Claude Code's transcripts, split into input/output and cache. | Cost in money; days before today. |
| **C3** | **Claude Sessions: plan limits** | 5-hour and weekly use (%) with reset times, from Claude Code's status line. A separate switch, off by default. | Glance's network check; sessions that run only in VS Code or the desktop app (Claude Code doesn't document running a status line there); replacing a status line you already have. |

### The Platform Underneath

**[call] The catalog.**
- `catalog.json` and its signature `catalog.json.sig` are assets of a GitHub Release tagged `extensions` on
  `KJ-AIML/samo`, which is public. That gives one fixed URL.
- Each entry has: `id`, `name`, `summary`, `version`, `protocol`, `min_samo`, `url`, `sha256`, `size`, and
  `changes` (plain-language lines shown before install).
- Packages are assets of their own releases, e.g. `claude-sessions-v0.1.0` with
  `claude-sessions-0.1.0-macos-universal.tar.gz`.
- The signature is ed25519 over the exact catalog bytes. The public key is compiled into Samo. The private key never
  enters the repo: it stays in KJ's Keychain and `cargo xtask ext-sign` reads it from there.
- Trade-off: hashes alone would protect against a corrupted download. The signature also protects against someone
  who can write to the GitHub releases but doesn't have KJ's key.

**[call] The package.** A `.tar.gz` holding one folder `<id>/`:
- `extension.toml`: `id`, `name`, `version`, `protocol = 1`, `min_samo`, `helper = "bin/<name>"`, `summary`;
  `[[changes]]` entries, each with `what`, `where` and `undo`; and `[[settings]]` switches, each with `key`, `label`
  and a default.
- `bin/<helper>`: a universal (arm64 + x86_64) binary, signed ad hoc.

**[call] On disk** (`~/Library/Application Support/Samo/Extensions/`):

```text
Extensions/
  installed.toml           # each extension: version, on/off, source (catalog | dev, with the dev source path)
  history.log              # JSON lines: install, update, remove, plus the helper's `log` lines (connect, disconnect)
  claude-sessions/
    current -> 0.1.0       # symlink, replaced atomically on update; hooks point through it
    0.1.0/extension.toml
    0.1.0/bin/samo-claude
    data/                  # the extension's own folder; kept across updates, deleted on Remove
      serve.lock           # held by the running helper (std `File::try_lock`), so two Samos never run it twice
```

**[call] Two copies of Samo** (say a dev build and the installed app) never run one extension twice. The helper
takes `data/serve.lock` at start. The second helper exits with `health { ok: false }`, and its Samo shows *running in
another copy of Samo*.

The Mac app keeps no database and has no command registry today (it calls `samo-macos` directly). So
`installed.toml` and `history.log` follow `locations.txt`'s pattern instead of `samo-store`/`samo-engine`. This
doesn't make the existing gap between ADR-001's platform layer and the Mac app any wider.

**[call] Protocol v1** (crate `samo-ext-protocol`): newline-delimited JSON.

| Direction | Messages |
| --- | --- |
| Samo → helper | `hello { protocol, samo_version, data_dir, settings }`, `setting { key, value }`, `action { id }`, `visible { page: bool }`, `shutdown` |
| Helper → Samo | `hello { protocol }`, `view { sections }`, `badge { count, summary }` (0 clears it; `summary` is the quick panel's line), `popup { key, kind: needs_you \| finished \| info, title, body }`, `log { event, detail }` (Samo appends it to `history.log`), `health { ok, message }` |

**[call] The helper owns its switches.** Its `view` reports each switch's state. A flip sends `setting { key, value }`,
the helper acts, and the next `view` shows the result. The helper may refuse; for example, Plan limits stays off when
a status line is already set. Samo stores no switch values and re-applies nothing at start. For Claude Sessions,
`~/.claude/settings.json` is the truth for Connect and Plan limits, and its pop-up switches live in `data/`.

A `view` is built only from Samo's widgets, each drawn with an existing component:

| Widget | Samo component | Notes |
| --- | --- | --- |
| `section { title }` | `section_rule` | `SESSIONS · LIVE` |
| `figure { label, value: Reading<f64>, unit }` | `dot_number` | Unknown draws `--` |
| `meter { label, fraction: Reading<f64>, note }` | `led_meter` | `None` lights nothing |
| `rows { items: [{ lamp, title, detail, trailing, tag }] }` | card rows, `tag`, `clip_text` | `lamp`: `working`, `needs_you`, `done`, `off`, `error` |
| `switch { key, label, on, note }` | `face_switch` | Only keys declared in `[[settings]]` |
| `button { action, label }` | `button` `Default`/`Link` | Never `Primary` or `Danger` |
| `note { text, kind: plain \| error }` | empty / error line | |

Colour follows DESIGN.md. *Needs you* is Ink with `!`, not Signal: waiting isn't wrong. Signal is only for `error`
(e.g. Claude Code stopped on an API error). An extension can't use dither, because dither means "Samo suggests".

### How Each Feature Behaves

#### E1 — Extensions page
1. **[call]** A new fixed `Extensions` page sits at the end of the sidebar and rail. Each installed extension that is on
   gets its own page, in a second sidebar group named `EXTENSIONS`. `NavSection` gains `Extensions` and
   `Extension(ExtSlot)`, where `ExtSlot` is a small index that keeps the enum `Copy`. ⌘1–⌘5 stay on the five fixed
   pages.
2. Opening the page shows what is installed and uses no network. **Check for extensions** is the only thing that
   fetches the catalog. It shows when you last checked.
3. A catalog entry shows its `changes` lines before the **Install** button. For Claude Sessions:
   *"Adds hooks to `~/.claude/settings.json` when you press Connect (backed up first); sets Claude Code's status line
   if you turn on Plan limits and don't have one; reads your Claude Code transcripts; never goes online."*

#### E2 — Install, update, remove
1. **Fetch:** download `catalog.json` and `.sig`, then verify the signature with the built-in key. A bad signature
   shows an error line and lists nothing.
2. **Install:**
   - Download the package. The size must equal `size` and be ≤ 50 MB **[call]**.
   - Check the SHA-256 against the catalog.
   - Validate every archive entry (R5), then unpack into `Extensions/.staging-<random>/`.
   - Check that the manifest's `id`/`version` match the catalog entry and that `helper` lies inside the folder.
   - Rename the folder into place and point `current` at it.
   - Any failure removes the staging folder and leaves the old state untouched.
3. **Update:** the same steps into a new version folder, then swap `current` atomically and restart the helper.
   The old version is deleted only after the new helper says `hello`; otherwise `current` swaps back.
4. **Remove:**
   - Turn the extension off.
   - Run `<helper> uninstall`, which undoes everything in `[[changes]]` (R7).
   - If that fails, show what is left and offer **Remove anyway**, which keeps the backups and names them.
   - Delete the extension's folder, including `data/`.
5. **Developer folder:**
   - `SAMO_EXT_DEV_DIR=/path` installs each unpacked extension in that folder without a signature. It is copied into
     `Extensions/<id>/<version>/` with `current` pointing at it, exactly like a catalog install, so hook paths are
     identical.
   - `installed.toml` records `source = dev` and the source path. A dev extension runs only while `SAMO_EXT_DEV_DIR`
     points at that path. Otherwise its page says *dev build: start Samo with SAMO_EXT_DEV_DIR to run it*. Unsigned
     code never runs just because it was installed once (R5).
   - When the source folder changes, Samo copies it again at start.
   - Dev extensions are tagged `dev` everywhere they appear.
6. **[call] Turning an extension off** stops its helper and runs `<helper> off`. That undoes what is active
   (Claude Sessions takes its hooks and status line out) and remembers it in `data/`. **On** runs `<helper> on`,
   which restores them. Off means nothing of the extension runs anywhere, Claude Code included. Quitting Samo is not
   *off*: the hooks stay, so the session states are right when Samo starts again.
7. Every install, update and remove, and every `log` line from a helper, appends a line to `history.log`.

#### E3 — Extension host
1. One supervisor thread per running helper. It spawns `<helper> serve` with a minimal environment (`HOME`, `PATH`,
   `LANG`, `SAMO_EXT_DATA_DIR`), reads stdout lines and writes `stderr` to `data/helper.log` (1 MB, one rotation).
   `samo-extensions` keeps its own `Arc<Mutex<ExtState>>` and calls a wake callback that the app passes in. The app
   wires that callback to `hub.wake_ui()` and copies `ExtState` into its `Frame` in `snapshot()`. `SharedState`
   stays in `hub.rs`.
2. **The UI wakes only when there is news.** Any message from a helper wakes the UI, coalesced to at most 4 wakes a
   second. An extension with nothing changing sends nothing, so idle Samo stays as it is: nothing animates, and
   nothing polls on the UI thread.
3. The caps are in R8. A helper that crashes restarts after 1, 2, 4 … 60 s. After 5 crashes within 10 minutes it is
   **Stopped**: the page shows the mascot asleep, the reason, and a **Restart** button.
4. **Stopping a helper:**
   - Send `shutdown`, then close stdin.
   - After 2 s, `Child::kill`. Samo holds the `Child` and hasn't reaped it, so its pid can't have been reused, which
     is what R4 protects.
   - Quitting Samo stops every helper this way.
5. **The menu bar.**
   - The badge appears as text beside the CPU figure: `23% !1`. The probe's eyes stay Samo's own state (DESIGN rule
     3), and the tray's `Shown` diffing already redraws when the title changes.
   - The quick panel gets one fact row per running extension that has a badge, e.g. "Claude · 1 needs you · 2
     working". A click opens that extension's page.
6. **Pop-ups.**
   - A `WindowKind::PopUp` window with `focus: false`, so it never takes the keyboard away from the terminal you are
     typing in.
   - It appears under the tray item (or at the top right of the main screen if that position isn't known) and hides
     after 6 s. A newer pop-up replaces the shown one. The host's flood limits are in R8; how often an extension
     *wants* to pop up is its own policy (C1.4).
   - A click opens the extension's page.
   - No pop-up while Samo's window is frontmost and showing that extension's page.
   - **[call]** No sound. Pop-ups ignore Focus modes; each extension has switches to turn its pop-ups off.

#### C1 — Session states
1. **Connect** (a switch on the Claude Sessions page, off after install):
   - Back up `~/.claude/settings.json` to `~/.claude/settings.json.samo-bak`. That happens only the first time, and a
     later connect never overwrites the backup.
   - Add one hook entry for each event in the table below. Each entry is
     `{ "type": "command", "command": <HOOK>, "async": true, "timeout": 5 }`, with no `matcher`.
   - `<HOOK>` is
     `H="$HOME/Library/Application Support/Samo/Extensions/claude-sessions/current/bin/samo-claude"; [ -x "$H" ] && "$H" hook; exit 0`.
     If the extension disappears, the hook does nothing and still exits 0. Samo recognises its own entries by the
     substring `/Samo/Extensions/claude-sessions/`.
2. **`samo-claude hook`:**
   - Reads stdin (at most 1 MB) and maps the event to a state using the table below.
   - Writes `data/sessions/<session_id>.json` atomically (temp file, then rename): state, reason, `cwd`,
     `transcript_path`, `updated_at_ms`, the event, and Claude's process identity.
   - It only replaces a file whose `updated_at_ms` is older, because async hooks can finish out of order.
   - It gives up after 500 ms, prints nothing and always exits 0 (R9).
   - `serve` checks the `data/sessions/` folder's mtime every 250 ms. That's one `stat`, and a rename into the folder
     changes its mtime. On a change it re-reads only the files that changed and sends a new `view` and `badge` at
     once. This is what makes *within 1 s* hold.

   | Claude Code event (documented) | State | Shown as |
   | --- | --- | --- |
   | `SessionStart` | done | *ready* (no pop-up) |
   | `UserPromptSubmit`, `PreToolUse` (other tools), `PostToolUse` | working | *working* |
   | `PreToolUse` with `AskUserQuestion` | needs you | *a question* |
   | `PreToolUse` with `ExitPlanMode` | needs you | *a plan to review* |
   | `PermissionRequest` (fires immediately) | needs you | *permission: <tool_name>* |
   | `Notification` `permission_prompt` (fires after ~6 s, as a backstop) | needs you | *permission* |
   | `Notification` `elicitation_dialog`, `elicitation_url_dialog`, `agent_needs_input` | needs you | *input needed* |
   | `PostToolUseFailure` with `is_interrupt: true` | done | *interrupted* |
   | `Stop` | done | *done*, or *done · N background tasks* |
   | `StopFailure` | done (error) | *stopped: API error* (Signal) |
   | `SessionEnd` | removed | |
   | `Notification` `idle_prompt` | no change | (a timing heuristic, not a question) |

3. **Known gaps in Claude Code's hooks, and how Samo stays honest (R10):**
   - Answering a permission fires nothing. The next `PreToolUse`/`PostToolUse` moves the session back to working.
   - A user interrupt (Esc) outside a tool call fires no `Stop`.
   - `SessionEnd` doesn't fire on a crash or a closed terminal.
   - Two measures cover these:
     1. **Process identity.** On the first event of a session, the hook records Claude's `ProcessIdentity` (pid and
        start time, as in R4): the nearest ancestor of the hook that isn't a shell (`sh`, `bash`, `zsh`, `dash`,
        `fish`). `serve` checks those identities every 5 s and removes a session whose Claude process is gone.
        Without an identity, a session is removed after 12 h with no event.
     2. **Quiet sessions.** After 5 minutes with no hook event and no transcript write, a *working* row says
        *working · no news for 7 min* instead of pretending to know more.
4. **Pop-ups:**
   - *Needs you* when a session enters that state.
   - *Finished* when a working turn of 15 s or more ends in done. Quick replies don't pop up.
   - At most one pop-up of each kind per session every 30 s. A permission pop-up followed shortly by *Finished*
     shows both.
5. **Disconnect** removes only entries whose command contains Samo's substring and leaves everything else exactly as
   it was (R7).

#### C2 — Tokens
1. The format of Claude Code's transcripts is **internal and undocumented**: Claude Code's own docs say it changes
   between versions. In today's files, assistant lines carry `message.id`, `requestId`, `timestamp` and
   `message.usage` with `input_tokens`, `output_tokens`, `cache_creation_input_tokens` and `cache_read_input_tokens`.
   One message is written as several lines that repeat the same usage (one sample had 41 lines for 16 messages), so
   **each `message.id` counts once**.
2. Which files are read:
   - Every `*.jsonl` at any depth under `~/.claude/projects`, subagent transcripts included, but only files modified
     since local midnight. A file not touched today can't hold today's lines.
   - These files are found from hooks (`transcript_path`) and from a sweep when `serve` starts and every 5 minutes.
   - KJ's folder holds 2.7 GB in 347 projects, but only 18 files changed in the last day. So the folder is never read
     in full.
   - Each file is read from its last offset, at no more than 8 MB/s **[call]**.
3. **[call] What counts:**
   - Lines whose own `timestamp` falls after local midnight.
   - Each `message.id` once per day across all files, because resumed and forked sessions can copy history into a
     new file.
   - The total is input + output + cache writes + cache reads. Each row shows the split.
4. A missing or unreadable folder is `Unsupported { reason: SourceNotFound }`. The page says *Claude Code's
   transcripts weren't found*.
5. If most usage lines in a file fail to parse, the reading is `Unsupported { reason: FormatChanged }`, shown as
   *Claude Code's transcript format changed*. It is never `0` (R10).

#### C3 — Plan limits
1. **Plan limits** is a switch, off by default. It changes Claude Code's footer, so it's a separate decision.
2. **Turning it on:**
   - If `statusLine` is set and isn't Samo's, the switch stays off and says why. Samo never replaces it.
   - Otherwise Samo sets `{ "type": "command", "command": <STATUSLINE> }`. `<STATUSLINE>` is the guarded path, as in
     C1, with `statusline`. No `refreshInterval` is set, so Claude Code calls it only on its own triggers.
3. **`samo-claude statusline`:**
   - Reads the documented `rate_limits.five_hour` / `rate_limits.seven_day` (`used_percentage`, `resets_at` in epoch
     seconds).
   - Writes `data/limits.json` atomically with `observed_at`.
   - Prints one short line, `5h 34% · wk 12%`, and exits 0.
4. **The page:**
   - Two meters with reset times, and "as of 14:02".
   - A window Claude Code didn't report is `Unsupported { reason: NotReported }`, drawn as `--` (*not reported*).
   - A window whose `resets_at` has passed becomes unknown until the next update.
   - Claude Code reports these only for Pro/Max subscribers, and only after the first response.
   - A note says limits update from terminal sessions only, because Claude Code doesn't document a status line in
     VS Code or the desktop app.
5. **Turning it off** removes `statusLine` only if it is still exactly Samo's command.

---

## 4. Rules That Protect the Anti-Goals

These continue ADR-001's R1–R4. Each is enforced in code and checked by a test or an `xtask` gate.

| Rule | Protects | Enforcement | Required test |
| --- | --- | --- | --- |
| **R5 — Only signed, pinned packages run** | Anti-goal 2 | `samo-domain::extensions`: `verify_catalog(bytes, sig, key)`, `verify_package(bytes, sha256, size)` and `validate_entries(entries)`. Entries are rejected if they are absolute, contain `..`, are symlinks, hardlinks or devices, sit outside `<id>/`, exceed 2,000 files or exceed 100 MB unpacked. The manifest `id`/`version` must equal the catalog's, and `helper` must resolve inside the folder. Unsigned code loads only from `SAMO_EXT_DEV_DIR` and is tagged `dev`. | `domain::tests::extension_rejects_bad_signature_hash_and_unsafe_entries` |
| **R6 — The network only on a click** | Anti-goal 6 | gpui already brings an HTTP stack (`gpui_http_client`, `zed-reqwest`, hyper, rustls, tokio) into `samo-gpui` *transitively*. So the gate checks **direct** dependencies and **call sites**: only `samo-extensions` may list an HTTP client in `[dependencies]`; `samo-gpui` must not call gpui's built-in client (`http_client(`); the download functions require a `&UserAction`; and `UserAction::clicked()` may appear only in `apps/samo-gpui/src/extensions_page.rs`. There are no timers. `samo-claude` has no HTTP or socket dependency and no `std::net`. | `xtask` gate `network` (direct-dependency and call-site checks) |
| **R7 — Everything an extension changes is declared, backed up and undone** | Anti-goal 3 | A package without `[[changes]]` won't install, and those lines are shown before install. Claude Sessions' `settings` module is pure: `connect`, `disconnect`, `set_statusline`, `clear_statusline` over a `serde_json::Value` (`preserve_order`). `connect` is idempotent. `disconnect` removes only Samo's entries. An existing `statusLine` is never replaced. The backup is written once before the first change. Writes go through a symlink to its target (dotfile managers link `settings.json`), via a temp file in the target's folder, then a rename, and only if the file is unchanged since it was read. Output is 2-space pretty JSON. "Restored" means equal JSON (same keys, values and order), not identical bytes. Remove runs `uninstall` before deleting. | `connect_then_disconnect_is_equal_json`, `disconnect_keeps_user_hooks`, `never_replaces_statusline`, `connect_twice_adds_once`, `write_refused_if_changed_since_read`, `symlinked_settings_stay_a_symlink` |
| **R8 — An extension can't slow, hang or crash Samo** | Anti-goal 1 | Out of process only; no dynamic loading, and `unsafe` stays forbidden. Caps: lines up to 64 KB (longer ones are dropped and logged); 20 messages/s, beyond which only the latest `view` is kept; 200 rows per view. Pop-ups at least 3 s apart per extension (a newer one replaces the shown one), and no more than 10 a minute; the rest are dropped and logged. The helper is stopped if it averages over 10 % of one core for 60 s or uses over 150 MB RSS, sampled every 10 s for its pid only. `samo-extensions` uses `sysinfo` directly, because the layer gate rejects a same-rank dependency on `samo-macos`. Restarts back off, and the helper is Stopped after 5 crashes in 10 minutes. All helper I/O runs on the supervisor thread. | `domain::tests::crash_backoff_and_stop`, `extensions::tests::oversize_and_flood_are_dropped_not_buffered` (with a fake helper from `samo-testkit`) |
| **R9 — Claude Sessions never gets in Claude Code's way** | Anti-goal 4 | Every hook is `async: true`. Claude Code doesn't enforce `timeout` on a running async hook, so the real limit is the hook's own: it exits within 500 ms whatever happens. `hook` and `statusline` always exit 0. `hook` never writes stdout (Claude Code adds hook stdout to Claude's context on some events), never exits 2 (which blocks), and does one file write: no transcript reads, no network. | `hook_is_silent_and_exits_zero_on_bad_input` (garbage stdin, missing or read-only data folder), `hook_exits_within_500_ms` |
| **R10 — A session's state is something true** | Anti-goal 5 | A pure `sessions::derive(file, now, alive)` decides each row. A dead Claude process means the session is removed. No news for 5 minutes adds *no news for N min*. Tokens and limits are `Reading<T>`: unreadable or changed formats are `Unsupported`, never `0`, and a limit past `resets_at` is unknown. | `dead_process_removes_session`, `no_news_is_said_not_hidden`, `unparseable_transcript_is_unsupported_not_zero`, `limits_past_reset_are_unknown`, `message_id_counted_once` |

---

## 5. How It Gets Made

### KJ's Decisions
- **[KJ]** A Glance-like Claude Code session monitor inside Samo: states, pop-ups, tokens, plan limits.
- **[KJ]** Optional: a downloadable, installable extension (2026-10-31).

### Calls
1. **[call] Out-of-process helpers over JSON lines.**
   - `unsafe` is forbidden and Rust has no stable ABI, so no `.dylib`.
   - WASM can't do what Claude Sessions needs: Claude Code must be able to run it as a command, and it reads files.
   - A helper that crashes can't take Samo down.
2. **[call] A fixed widget set drawn by Samo.** The look stays Samo's, and DESIGN.md's rules hold: one Ink action per
   view stays Samo's, Signal only for errors, no dither.
3. **[call] Crates.**

   ```text
   crates/
     samo-ext-protocol/   # pure, rank 1: manifest, catalog, protocol messages, widgets; depends on samo-primitives only
     samo-domain/         # + extensions module: R5 verification and entry checks, R8 backoff policy, version checks
     samo-extensions/     # rank 5 (L5): catalog and package download (the only HTTP user), unpack, install/update/remove,
                          #   installed.toml, history.log, helper supervisor
   apps/samo-gpui/        # + extensions_page.rs, a widget renderer, extension nav slots, menu bar badge text, the pop-up window
   extensions/
     claude-sessions/     # binary `samo-claude`: serve, hook, statusline, on, off, uninstall
                          #   pure modules: settings (R7), sessions (R10), usage (C2); I/O only in main
                          #   depends only on samo-ext-protocol, samo-primitives, serde_json, sysinfo, chrono
   xtask/                 # + layer ranks for the three new crates, purity for samo-ext-protocol, gate `network` (R6),
                          #   ext-package (build the universal helper, tar, hash), ext-sign (sign the catalog)
   ```
   - This matches the baseline: "L5 integrations (the only reqwest user) and extensions".
   - **[call]** `ureq` with rustls instead of `reqwest` or gpui's own client. It is blocking and small, it runs on a
     plain thread with no async runtime, and it keeps every network call in one L5 crate that can be tested without
     the UI. The rule (one HTTP crate, at L5) is unchanged.
   - `ring` (already in `Cargo.lock`) for ed25519 and SHA-256; `flate2` (already in the lock) and `tar` (new) for
     packages.
4. **[call] Claude Sessions lives in this repo.** Lints and `xtask` gates apply to it. It ships separately, as its own
   GitHub Release.
5. **[call] `xtask` must learn the new crates in three places.** Otherwise they go unchecked without any error:
   - `layer_rank` (`layers.rs:4-16`), which skips crates it doesn't know.
   - The fixed list of manifests in `check_layers` (`layers.rs:59-70`).
   - The purity gate's fixed folder list (`purity.rs:61-65`), for `samo-ext-protocol`.
6. **[call] Three new `UnsupportedReason` variants** in `samo-primitives`: `SourceNotFound`, `FormatChanged`,
   `NotReported`. The enum stays closed and `Copy`; the UI maps each to its sentence. Existing matches on the enum
   need a line each.

---

## 6. Build Order

### Phase 1 — Host, with a local test extension (E3, part of E1)
- **Builds:**
  - `samo-ext-protocol`, the domain `extensions` module (R5 entry checks, R8 backoff).
  - The supervisor in `samo-extensions` (no network yet).
  - In the Mac app: the Extensions page, the widget renderer, extension nav slots, the menu bar badge, the pop-up
    window.
  - A tiny `hello` extension in `samo-testkit` that emits every widget, a badge and a pop-up.
  - `xtask` layer and purity entries.
- **Done when:**
  - With `SAMO_EXT_DEV_DIR` pointing at `hello`, Samo shows its page with live widgets, `!1` beside the CPU figure,
    and a pop-up that doesn't take focus from the terminal.
  - Killing the helper 5 times shows Stopped.
  - With no extension on, idle CPU is unchanged (±0.5 % on Overview, release build).
  - `cargo run -p xtask -- verify` passes.

### Phase 2 — Claude Sessions, installed from the dev folder (C1, C2, C3). The end-to-end demo.
- **Builds:** `samo-claude` (`serve`, `hook`, `statusline`, `on`, `off`, `uninstall`) and the tests for R7, R9 and R10.
- **Done when** KJ can show, on KJ's own Mac:
  1. Install from the dev folder and press **Connect**. `~/.claude/settings.json` has Samo's hooks, and
     `settings.json.samo-bak` exists.
  2. Run two Claude Code sessions in terminals. The working one shows *working*. The one that asks for a permission
     shows *permission: Bash* within 1 s, `!1` appears beside the CPU figure and a pop-up appears. Approving it
     flips the session back to *working*. A turn of 15 s or more that ends pops up *Finished*.
  3. Close a terminal window outright (no `SessionEnd`), and its session leaves the list within 10 s.
  4. Today's tokens match a hand count from today's transcripts (deduplicated by `message.id`) within 1 %.
  5. Turn on Plan limits with no status line: 5-hour and weekly % appear after the next reply. With a status line
     already set, the switch refuses and says why.
  6. Press **Disconnect**: `settings.json` is equal JSON to its state before Connect, apart from changes Claude Code
     made in between. Turn the extension **off** and back **on**: the hooks go and come back.
  7. Claude Code feels the same: async hooks, no output.

### Phase 3 — Download and trust (E1, E2)
- **Builds:**
  - Catalog and signature checks, download with `ureq`, SHA-256 check, unpacking, atomic update with rollback, and
    Remove with `uninstall`.
  - R5 and R6 tests and the `network` gate.
  - `xtask ext-package` / `ext-sign`.
  - The first `claude-sessions-v0.1.0` and `extensions` releases.
  - Update the README's "Safety and privacy": Samo goes online only when you press Check for extensions, Install or
    Update.
- **Done when:**
  - On a clean macOS user account: **Check for extensions → Install Claude Sessions** works as in Phase 2.
  - A package with one byte changed is refused with a clear line, and so is a catalog with a bad signature.
  - **Remove** leaves no Samo hooks or status line in `settings.json` and nothing under `Extensions/claude-sessions/`.

---

## 7. What Carries Over

| Concept | What it becomes in Samo | Source |
| --- | --- | --- |
| Glance's three states and its short pop-up | C1's states and pop-ups (Samo's look, Samo's words) | Glance README only; no code exists to read |
| Glance's Connect: hooks added with a backup, Disconnect removes only its own | C1 Connect/Disconnect, R7 | Glance README, "What it changes on your computer" |
| Glance's plan limits from the status line, never replacing yours | C3 | Glance README |
| Glance's tokens read from transcripts, read-only | C2 | Glance README |
| `Reading<T>` and honest gaps (R3) | Tokens and limits are `Reading<T>`; unknown is `--` | `crates/samo-primitives/src/reading.rs` |
| `ProcessIdentity` (R4) | Claude's process identity: a dead session is noticed even without `SessionEnd` | `crates/samo-domain/src/process.rs` |
| The sampler thread and wake channel | Extension updates ride the existing wake | `apps/samo-gpui/src/hub.rs` |
| The quick panel's `PopUp` window | The pop-up window (without focus) | `apps/samo-gpui/src/menubar.rs:148-199` |
| `locations.txt` | `installed.toml`, `history.log` | `apps/samo-gpui/src/locations.rs` |
| Trash, never `rm` | Remove undoes changes before deleting; backups are kept | ADR-001 R2 |

**What is left behind:** Glance's network usage check and credential reading, its pixel clock, themes, the floating
bar and dot, the Windows build and Start with Windows.

---

## 8. Customisation per Machine

- **[call]** `SAMO_EXT_DEV_DIR`: load unpacked extensions from a folder, unsigned, tagged `dev`.
- **[call]** `SAMO_EXT_CATALOG_URL`: use another catalog (for testing). It must still verify against the built-in key.
- **[call]** Claude Sessions' switches: **Connect to Claude Code**, **Plan limits**, **Pop-up when a session needs
  you**, **Pop-up when a turn finishes**.

---

## 9. Left to the Builder

- Wording and layout of the Extensions and Claude Sessions pages, within DESIGN.md.
- Polling `data/sessions/` versus watching it (polling its mtime every second is fine).
- The pop-up's exact size and position, and the status-line text.
- `ring` versus `ed25519-dalek`/`sha2`; `tar.gz` versus `zip`.
- How `samo-testkit`'s fake helper is built.

---

## 10. Open Questions

None blocking. Two were put to KJ on 2026-10-31; KJ approved the doc without choosing, so the defaults stand:

1. **Anti-goals.** The six in §2 stay **[call]**s, unranked. If KJ later names the one that would make the product a
   failure, it moves to the top and gets the strictest test.
2. **Signed catalog.** The **[call]** stands: ed25519, private key in KJ's Keychain, `cargo xtask ext-sign`. It
   matters only in Phase 3.
