# ADR-003: Samo's extension store, and updates in Settings

**Date:** 2026-11-01
**Status:** Accepted by KJ on 2026-11-01 ("Approved"), with the [call]s as written. The builder check is folded in.
**Product:** Samo (`samo`)
**Builds on:** ADR-002 (extensions). This replaces ADR-002's Phase 3 ("Download and trust") with a store that does
the same and more, and it adds updates for Samo itself. ADR-002's rules R5–R10 still hold.

---

## 1. Intent

**[KJ]** KJ wants a marketplace for extensions inside Samo, so that people can choose and install extensions
themselves. It should feel like a store, or like VS Code's extensions view: each extension has images to look at, a
description, and Install or Uninstall.

**[KJ]** KJ wants an **Updates** section in Samo's Settings, to update Samo from inside the app.

**[KJ]** Decisions made on 2026-11-01:
- **Layout:** a list on the left and the selected extension's page on the right, like VS Code, with a search box.
  This was chosen over a grid of cards like the App Store.
- **Publishing:** curated. Teammates send an extension to a store repository by pull request, KJ reviews it, and
  KJ signs the catalog. Samo installs only what KJ signed. An open marketplace was not chosen.
- **Store network:** Samo goes online when you open the store. Screenshots load when you open an extension's page,
  and everything is cached. Samo never goes online in the background. Fetching only on Refresh was not chosen.
- **Update checks:** Samo checks when you open Settings → Updates, and never by itself. A daily check in the
  background was not chosen.

**Why it is needed now.**
- Today an extension loads only from a developer's folder, and only while Samo is started with
  `SAMO_EXT_DEV_DIR`. KJ hit this on the first day: opened from the Dock, Claude Sessions said it couldn't run.
- Updating Samo means downloading a DMG by hand.
- ADR-002's Phase 3 planned signed downloads, but no screen to browse them.

**What it is:**
- A **store** on Samo's Extensions page: the list, the detail page, Install, Update, Uninstall.
- A **Settings** page whose first section is **Updates**: Samo's version, what's new, and Install and restart.
- A **store repository**, `KJ-AIML/samo-extensions` **[call]**, public, holding each extension's listing, its
  images, Samo's own release, and the signed catalog.
- **Tooling** that packages, signs and lays out what to publish.

**What it is not:**
- An open marketplace. There are no accounts, ratings, reviews or payments.
- A website. The store lives in Samo.
- Automatic updates, or checks in the background.
- A sandbox for untrusted code. Trust comes from KJ's review and signature.

**Who it's for.** **[KJ]** Anyone running Samo who wants an extension, starting with teammates who want Claude
Sessions.

---

## 2. Goals, Non-Goals, Anti-Goals

### Goals
1. **[KJ]** Browse extensions in Samo, see their screenshots and description, and install or uninstall them in one
   click.
2. **[KJ]** Update Samo from its Settings.
3. **[call]** An extension installed from the store works however Samo is opened, with no environment variable.
4. **[call]** A teammate can add an extension to the store without changing Samo's code: a pull request to the store
   repository.
5. **[call]** Updates, of Samo and of extensions, show up when you look, and installing one is a single click.

### Non-Goals (v1)
- **[call]** Publishing by anyone. Publisher accounts and a store server are out of scope.
- **[call]** Ratings, reviews, download counts.
- **[call]** Search over full descriptions. Search covers name, one-line summary, author and categories.
- **[call]** Anything that installs or checks without a click.
- **[call]** Updating from 0.1.x. Those versions have no updater, so moving to 0.2.0 is a manual DMG install, once.
- **[call]** Apple notarisation, Intel builds, Windows.

### Anti-Goals
These are **[call]**s, extending ADR-002's.
1. **Code KJ didn't sign runs on someone's Mac:** an extension, or a "Samo update" (ADR-002 anti-goal 2).
2. **Samo goes online in the background** (ADR-002 anti-goal 6).
3. **Uninstalling leaves things behind** without saying so (ADR-002 anti-goal 3). Kept on purpose, and named on the
   page: the one-time backup `~/.claude/settings.json.samo-bak`.
4. **The store makes Samo slow:** a big image, a slow network or unpacking a package freezes the window, heats the
   Mac or grows memory without bound (ADR-002 anti-goal 1).
5. **The store shows one thing and installs another:** a tampered screenshot or description, or "What it changes"
   that doesn't match the package. *(New.)*
6. **An update leaves you without a working Samo:** half-replaced, or replaced and not restarted. *(New.)*

---

## 3. Core Features

| # | Feature | Covers (v1) | Does not cover (v1) |
| --- | --- | --- | --- |
| **M1** | **The list** | Every extension in the signed catalog and every installed one. **All · Installed · Updates** segments, and a search box over name, summary, author and categories. Each row: icon, name, one-line summary, author, and a state tag (*installed*, *update*, *dev*, *off*, *needs a newer Samo*). The sidebar says how many updates there are, from the cached catalog. | Sorting by popularity, categories as pages. |
| **M2** | **The detail page** | Icon, name, version, author, size, and a *signed* (or *dev*) tag. One action button: **Install**, **Update** or **Uninstall**, plus the on/off switch once installed. Screenshots, one at a time, with ‹ › and "1 of 3". The description. **What it changes on your Mac**. *What's new*. A homepage link, which opens in the browser. | Videos, animated images, comments. |
| **M3** | **Install, update, uninstall** | ADR-002's install steps for a catalog package (R5), shown on the button while running (*Installing…*). Update swaps the version in place and keeps the extension's data. Uninstall undoes everything, then deletes. A signed install replaces a dev build of the same id, after asking, and keeps it connected. | Downgrades, installing two versions side by side. |
| **M4** | **The store repository and publishing** | `samo-extensions`: a listing and a media folder per extension. `cargo xtask ext-keygen`, `ext-package`, `ext-publish`. One GitHub release, `store`, holds the catalog, its signature, the packages, Samo's own zip, and the images. | Publishing from CI. Automatic review. |
| **M5** | **Settings → Updates** | A new **Settings** page (gear, at the foot of the sidebar). Its **Updates** section shows: this Samo's version; when it last checked; the latest version with *What's new*; **Install and restart**; and a line "*N extensions have updates*", which opens the store's Updates segment. | Other settings (the page starts with Updates only). Release channels, rolling back. |

### The Platform Underneath

**[call] The catalog** (`catalog.json`, schema 1), signed as a whole (`catalog.json.sig`: ed25519 over the exact
bytes, base64):

```json
{
  "schema": 1,
  "published_at": "2026-11-02T10:00:00Z",
  "samo": {
    "version": "0.2.0", "whats_new": "The extension store, and updates in Settings.",
    "min_macos": "11.0",
    "package": { "file": "Samo-0.2.0-macos-arm64.zip", "size": 3900000, "sha256": "…" }
  },
  "extensions": [{
    "id": "claude-sessions", "name": "Claude Sessions", "author": "KJ",
    "summary": "Your Claude Code sessions at a glance",
    "description": "Paragraphs separated by blank lines.\n\n- Lines starting with '- ' are bullets.",
    "categories": ["developer"], "homepage": "https://github.com/KJ-AIML/samo",
    "version": "0.2.0", "protocol": 1, "min_samo": "0.2.0", "whats_new": "Installs from the store.",
    "package": { "file": "claude-sessions-0.2.0-macos-arm64.tar.gz", "size": 533516, "sha256": "…" },
    "icon": { "file": "claude-sessions-icon-1a2b3c4d.png", "sha256": "…", "width": 256, "height": 256 },
    "screenshots": [{ "file": "claude-sessions-shot-5e6f7a8b.png", "sha256": "…", "width": 1600, "height": 1000,
                      "caption": "Sessions and tokens" }],
    "changes": [{ "what": "…", "where": "…", "undo": "…" }]
  }]
}
```

- **Where each field comes from.** `ext-publish` takes `id`, `name`, `version`, `protocol`, `min_samo`, `summary`
  and `changes` from the package's own `extension.toml`. It takes only the store fields (`author`, `description`,
  `categories`, `homepage`, `whats_new`, captions) from `listing.toml`. It refuses a listing that repeats a
  manifest field with a different value.
- **Files are names, not URLs.** Each `file` is resolved against the catalog's own URL. The store's URL is fixed:
  `https://github.com/KJ-AIML/samo-extensions/releases/download/store/catalog.json`. GitHub redirects downloads to
  `*.githubusercontent.com`. Samo follows HTTPS redirects only to `github.com` and `*.githubusercontent.com`, and
  plain `http` only to loopback, in tests.
- **Image names** carry the first 8 hex digits of their hash, so a new image is a new file.

**[call] The store repository's layout:**

```text
samo-extensions/
  extensions/<id>/listing.toml     # store fields only (see above)
  extensions/<id>/icon.png         # 256 x 256
  extensions/<id>/screenshots/*.png  # at most 6, at most 2560 x 1600
  README.md                        # how to submit an extension
```

**[call] Publishing.**
- `cargo xtask ext-publish --store ../samo-extensions --samo <Samo zip>` reads that repository, the built packages
  and Samo's zip.
- It checks every listing and every image: dimensions, at most 2 MB, PNG or JPEG.
- It writes `dist/` (packages, Samo's zip, hash-named images, `catalog.json`, `catalog.json.sig`), signing with the
  private key from KJ's Keychain.
- It then verifies `dist/` with the public key.
- KJ uploads with `gh release upload store dist/* --clobber`, because Heli keeps GitHub writes with the human.

**[call] The signing key.**
- `cargo xtask ext-keygen` creates an ed25519 key pair in-process and stores the private key in the login Keychain
  (service `dev.talvyn.samo.store`) through the `security-framework` crate. The key never appears on a command line.
- It refuses to overwrite an existing key, and prints the public key.
- The public key is compiled into `samo-extensions` as `STORE_PUBLIC_KEY`. The library takes the key as a
  parameter: the app always passes `STORE_PUBLIC_KEY`, tests pass a test key, and no environment variable can
  change it.
- KJ runs `ext-keygen` once, by hand. Rotating the key means releasing a new Samo.

**[call] Where things are kept.**
- **The cache:** `~/Library/Caches/Samo/Store/`. It holds the last verified catalog with its signature, and images
  under their SHA-256, at most 50 MB, least recently used evicted first.
- **The replay floor:** the newest `published_at` Samo has accepted, kept per catalog URL in
  `~/Library/Application Support/Samo/store-state.toml`. It is not kept in the cache, which macOS may purge.

### How Each Feature Behaves

#### M1 — The list
1. **Going online only on a click (R12).**
   - Only a click opens the store with a `UserAction`: the sidebar entry, the rail icon, the quick panel's
     extension line, or Settings' "*N extensions have updates*".
   - Samo opening on the store at launch (`SAMO_START_PAGE=extensions`) shows the cache only.
   - A click starts a catalog fetch if the cached one is older than 10 minutes. **Refresh** fetches at once.
   - Rows' icons are fetched with the catalog, when not already cached.
2. **What the list holds.**
   - Installed extensions come first, then the rest by name.
   - A dev install shows as *dev*. A signed install with a newer catalog version shows as *update*.
   - An entry whose `min_samo` or `protocol` this Samo can't meet shows *needs a newer Samo*, with Install
     disabled.
3. **Search** filters as you type, case-insensitively, over name, summary, author and categories. Matches in the
   name come first.
4. **[call] Without the network,** the store shows the cached catalog and says *Offline: showing the store as of
   14:02*. With no cache, it shows installed extensions only, and says so.

#### M2 — The detail page
1. **Screenshots** download when the page is opened by a click (R12).
   - Before an image is handed to gpui to decode, Samo reads its dimensions from the header and refuses anything
     over 2560 × 1600 (icons 256 × 256), as well as anything whose dimensions differ from the catalog's.
   - Until an image is verified and decoded, it shows a dithered placeholder, never a broken image.
   - Leaving the page drops its decoded images from gpui (`drop_image`), so browsing doesn't grow memory.
2. **Which version the page describes.**
   - Not installed: the catalog's version.
   - Installed and current: the installed manifest's "What it changes".
   - An update is waiting: the page says so, and shows the new version's *What's new* and changes, labelled *In
     0.2.0*.
3. **The description** is plain paragraphs, with `- ` lines drawn as bullets. There is no markdown engine.
4. **The button.**
   - **Install** or **Update** is the view's Primary action.
   - **Uninstall** is a Danger button. It names the backup it keeps.

#### M3 — Install, update, uninstall
1. **Install** follows ADR-002 E2 steps 2–3 for a `Catalog` source.
   - Download with the size capped, check the SHA-256, check every entry (R5), and unpack into staging.
   - Check the manifest against the catalog entry: `id`, `version`, `protocol`, `min_samo` and `changes` must all
     match. Then rename into place and point `current` at it.
   - The registry records `source = "catalog"`, and the helper starts.
2. **Replacing a dev build (same id).**
   - Samo asks first: *Replace the dev build with the signed one?*
   - The replacement stages the signed copy, stops the helper, and moves the dev version's folder aside. That
     folder can have the same version number. Samo then puts the signed folder in place, swaps `current`, records
     `catalog`, and restarts the helper.
   - It keeps `data/` and the on/off state. It runs neither `off` nor `uninstall`, so Claude Sessions stays
     connected.
   - **[call]** From then on, a dev folder (`SAMO_EXT_DEV_DIR`) never replaces a catalog install: it is skipped,
     and the Extensions page says so. Going back to dev means uninstalling first.
3. **Update** installs the new version beside the old one, swaps `current`, and restarts the helper. If the new
   helper doesn't say hello, `current` swaps back. `data/` is kept, and old versions are removed once the new one
   runs.
4. **Uninstall.**
   - Turn the extension off, which runs `<helper> off` and undoes active changes. Then run `<helper> uninstall`.
   - Delete `Extensions/<id>/`, including `data/`, mark its slot gone for this run, remove the registry entry, and
     record it in `history.log`.
   - The helper's stderr from `off` and `uninstall` is kept, so if a step fails, the page shows what it said and
     offers **Uninstall anyway**.
   - A dev build that can't run here (no `SAMO_EXT_DEV_DIR`) counts as a failed step, never as a silent success.
5. **While it runs,** the button shows *Installing…*, *Updating…* or *Uninstalling…*. Errors appear on the detail
   page in words that say what to do.
6. **Slots** only ever count up within a run. A pop-up or page for an uninstalled extension says it's gone; it
   never points at another extension.

#### M4 — Publishing
1. **Submitting.** A teammate opens a pull request to `samo-extensions` with `listing.toml`, an icon, screenshots,
   and the source or a link to it. KJ reviews it.
2. **Packaging.** `cargo xtask ext-package <path>` builds the helper and lays out the `.tar.gz`.
   `scripts/ext-dev.sh` stays for development.
3. **Releasing Samo.** `scripts/bundle-macos.sh` builds the app. `ext-publish --samo` puts its zip in the catalog.
4. **Publishing.** `cargo xtask ext-publish` checks, writes `dist/`, signs and verifies. KJ uploads `dist/*` to the
   `store` release.

#### M5 — Settings → Updates
1. **The page.**
   - **Settings** is a fixed page at the foot of the sidebar (and rail).
   - Opening it by a click makes a `UserAction`, and the **Updates** section fetches the catalog the same way the
     store does. It's the same file and the same cache.
   - Opening it at launch shows the cache only.
2. **What it shows.**
   - *Samo 0.2.0 · checked 14:02*. When Samo is current: *You have the latest Samo*.
   - When a newer version is waiting: *Samo 0.3.0 is available* with *What's new*, and **Install and restart** as
     the page's Primary action.
   - *N extensions have updates* opens the store's Updates segment.
3. **Install and restart.**
   - **Download:** Samo's zip, with the size capped, checked against the signed catalog's SHA-256.
   - **Check:** unpack into `~/Library/Caches/Samo/Update/<version>/`. The unpacked app must have bundle id
     `dev.talvyn.samo`, the catalog's version in its `Info.plist`, and pass `codesign --verify --deep --strict`.
     Nothing changes before all of this passes.
   - **Swap:** Samo starts the *new* app's own binary in update mode, `samo-gpui --finish-update <old app> <new app>
     <pid>`, and quits. The new binary waits for the old process to end, moves the old `Samo.app` to the Trash
     (`samo-macos`'s Trash mover, never `rm`), moves the new one into the old one's place, and opens it.
   - **If the swap fails** after the old app has gone to the Trash, it puts the old app back, opens it, and leaves
     a line in `history.log`.
4. **Updating isn't possible** when Samo isn't running from an app bundle (`cargo run`, `target/`), or when it
   can't write to the folder the app is in. Then the button is disabled, and the page says why and links the
   release page.

---

## 4. Rules That Protect the Anti-Goals

These continue ADR-002's R5–R10.

| Rule | Protects | Enforcement | Required test |
| --- | --- | --- | --- |
| **R11 — Only signed listings are shown; only pinned files are used** | Anti-goals 1, 5 | `samo-domain::store::verify_catalog(bytes, sig, key)` runs before the catalog is parsed. Every package, Samo zip, icon and screenshot is checked against its SHA-256 (and image dimensions) in the signed catalog before it is used, installed or drawn; a mismatch is discarded and never shown. A package's manifest must equal its catalog entry. A catalog older than the replay floor is refused. | `store_rejects_bad_signature_replay_and_mismatched_files`, `manifest_must_match_catalog_entry` |
| **R12 — Online only on a click, never in the background** | Anti-goal 2 | Refines R6. A `UserAction` is made only in click handlers in `apps/samo-gpui/src/extensions_page.rs` (the store) and `settings_page.rs` (Updates): opening either page by a click, Refresh, Install, Update, Uninstall, Install and restart. It can't be cloned, and every fetch needs one. There are no timers. A page shown at launch uses the cache. The sidebar's update counts come from the cache. The `network` gate allows exactly those two files. | `xtask` gate `network`; `store_fetch_needs_user_action`; `start_page_does_not_fetch` |
| **R13 — The store never freezes or bloats Samo** | Anti-goal 4 | Fetching, hashing and unpacking run on a store thread. Image decoding happens on gpui's background executor, only after the header check. Images are dropped when their page closes. Caps: catalog 1 MB; icon 256 KB and 256 × 256; screenshot 2 MB and 2560 × 1600; at most 6 screenshots; package 50 MB; Samo zip 100 MB; a request fails after 15 s without data; the cache holds 50 MB, least recently used first. Over a cap means refused, and said. | `oversized_files_are_refused`, `image_dimensions_checked_before_decoding`, `cache_stays_under_its_cap` |
| **R14 — An update never leaves you without a working Samo** | Anti-goals 1, 6 | Everything is downloaded and checked (R11, bundle id, version, `codesign --verify`) before Samo quits. The old app goes to the Trash, never `rm`, and only once the new one is in place. If the swap fails, the old app is put back and opened. | `update_swap_restores_old_app_on_failure` (two dummy bundles in a temporary folder) |
| **R7 (extended) — Uninstall undoes before it deletes** | Anti-goal 3 | Uninstall runs `off` and `uninstall` before deleting. If either fails (or the helper can't run here), nothing is deleted without **Uninstall anyway**. The kept backup is named on the page. | `uninstall_undoes_settings_then_deletes` (Claude Sessions, temporary home); `blocked_dev_uninstall_is_a_failed_step` |

---

## 5. How It Gets Made

### KJ's Decisions
- **[KJ]** A store in Samo: list and detail like VS Code, with screenshots, a description, Install and Uninstall.
- **[KJ]** Curated: a teammate's pull request, KJ's review, KJ's signature.
- **[KJ]** Online when the store is opened; screenshots when a page opens; cached; never in the background.
- **[KJ]** An Updates section in Settings; checked when you open it, never by itself.

### Calls
1. **[call] Crates.**
   - `ureq` 3 with rustls, in `samo-extensions::net` (still the only HTTP client).
   - `ring` for ed25519 and SHA-256 (already in `Cargo.lock`).
   - `tar`, `flate2` and `zip` for packages and Samo's zip.
   - `security-framework` in `xtask` only, for the Keychain.
   - Image headers are read with a few lines of our own (PNG IHDR, JPEG SOF); gpui decodes.
2. **[call] The search box** is gpui's text-input example, ported into `samo-gpui` (`text_input.rs`, no `unsafe`).
3. **[call] Where the code goes.**
   - Pure parts go in `samo-domain::store`: catalog types and checks, versions, search ranking, cache eviction,
     image headers.
   - I/O goes in `samo-extensions`: network, cache, install, update, uninstall, the Samo update.
   - Screens:
     - the store stays at `apps/samo-gpui/src/extensions_page.rs` (it *is* the Extensions page, and the `network`
       gate already names it);
     - Settings is new: `settings_page.rs`;
     - the existing per-extension page and `status_tag` move into the store's detail page.
   - The Trash mover for the update swap comes from `samo-macos`.
4. **[call] Versions.**
   - Samo becomes 0.2.0, the first version with the store and the updater.
   - Claude Sessions becomes 0.2.0 with `min_samo` 0.2.0, so the store's first Claude Sessions isn't another 0.1.x.
   - The README's privacy line changes to: *"Samo goes online only when you open its Extensions store or Settings
     → Updates, and only to its store."*
5. **[call] The `network` gate (KJ's file)** changes from one click site to two: `extensions_page.rs` and
   `settings_page.rs`.

---

## 6. Build Order

### Phase 1 — Trust, install and update, without the screens
- **Builds:**
  - `samo-domain::store` (R11 checks, versions, search, cache policy, image headers) and `samo-extensions::net`.
  - The cache and the replay floor. Install, update, replace-dev and uninstall from a catalog.
  - The Samo update: download, check, and `--finish-update`.
  - `xtask ext-keygen`, `ext-package`, `ext-publish`, and the `network` gate's second click site.
  - The `samo-extensions` repository, with Claude Sessions and Hello listed.
- **Done when:**
  - An integration test serves a catalog signed with a test key from a local HTTP server. It installs, updates,
    replaces a dev build (Claude Sessions stays connected) and uninstalls Hello and Claude Sessions, with a
    temporary home.
  - The same test refuses each of: a bad signature, a replayed older catalog, a package with one byte changed, an
    image with one byte changed or the wrong dimensions, a manifest that doesn't match its entry, and an oversized
    file.
  - `update_swap_restores_old_app_on_failure` passes.
  - `ext-publish` produces a `dist/` that verifies with the real public key.

### Phase 2 — The store and Settings. The end-to-end demo.
- **Builds:** the list, segments, search, the detail page, screenshots, the action buttons, the sidebar's update
  count, offline mode; the Settings page with Updates.
- **Done when** KJ can show, on KJ's Mac (after installing 0.2.0 from its DMG, once):
  1. Samo opened from the **Dock**. The Extensions store lists Claude Sessions and Hello with icons.
  2. Claude Sessions' page shows its screenshots and what it changes. **Install** replaces the dev build, after
     asking. Claude Sessions runs, still connected, with no `SAMO_EXT_DEV_DIR`.
  3. Hello installs, shows *update* after a newer version is published, updates in one click, and uninstalls,
     leaving nothing under `Extensions/hello`.
  4. Claude Sessions' **Uninstall** leaves `~/.claude/settings.json` without Samo's hooks or status line, and
     names the backup it kept.
  5. With 0.2.1 published, **Settings → Updates** offers it. **Install and restart** brings Samo back as 0.2.1
     within seconds, with the old app in the Trash.
  6. With Wi-Fi off, the store and Updates show the cache and say so.
  7. With neither page opened by a click, Samo makes no network connections (`nettop` shows none from Samo over 5
     minutes, launch included), and idle CPU is unchanged.

---

## 7. What Carries Over

| Concept | What it becomes | Source |
| --- | --- | --- |
| ADR-002 E2 install steps, R5 entry checks | Install, update and replace-dev from a catalog | `crates/samo-extensions/src/install.rs` (`install_dev`), `crates/samo-domain/src/extensions.rs` |
| `Source::Catalog`, `installed.toml`, `history.log` | Unchanged | `crates/samo-extensions/src/registry.rs` |
| `UserAction` and the `network` gate (R6) | R12, with a second click site | `xtask/src/network.rs` |
| `off` and `uninstall` helper commands | Uninstall (the host doesn't run `uninstall` yet; it will, and keep the stderr) | `extensions/claude-sessions/src/connection.rs`, `crates/samo-extensions/src/supervisor.rs` (`one_shot`) |
| The Trash mover | The update swap | `crates/samo-macos/src/cleanup.rs` |
| Today's Extensions page cards (what it changes, the switch, Restart) | The detail page | `apps/samo-gpui/src/extensions_page.rs` |
| VS Code's extensions view | The list and detail layout | VS Code (idea only) |

New, with no source yet: `verify_catalog`, the package and image checks, the network client, the cache, the
updater.

**What is left behind:** ADR-002's plan of one GitHub release per package and a separate `extensions` release.
There is now a single `store` release.

---

## 8. Customisation per Machine

- **[call]** `SAMO_EXT_CATALOG_URL`: another store, for testing (plain `http` only on loopback). It is still
  verified with the built-in key. Only the test suite, through the library's key parameter, uses another key.
- `SAMO_EXT_DEV_DIR` still works for development. Dev builds show as *dev*, and never replace a store install.

---

## 9. Left to the Builder

- The exact look within DESIGN.md: row height, the screenshot frame, the placeholder's dither, the gear icon.
- How screenshots step: buttons, arrow keys, or both.
- How the cache is indexed, as long as R13's caps hold.

---

## 10. Open Questions

None blocking. One action needs KJ, not a decision: run `cargo xtask ext-keygen` once, so the store has its
signing key.
