# Devlog 0005 — The store and Settings → Updates, 0.2.0

- **Date:** 2026-11-01
- **Design:** ADR-003, accepted by KJ ("Approved"). Plan: `docs/samo/2026-11-01-samo-store-p1-p2-plan.md`, all 12
  tasks, one commit each.
- **Phase 1, trust and install.**
  - `samo-domain::store`: the catalog, its ed25519 signature (checked before anything is parsed), per-file
    SHA-256, the replay floor, image sizes from PNG and JPEG headers, versions, search, cache eviction.
  - `samo-extensions`: `net` (ureq 3 with rustls; HTTPS to GitHub's hosts only, plain http only to this Mac; caps;
    15 s without data), `store` (the cache in `~/Library/Caches/Samo/Store`, the floor in
    `~/Library/Application Support/Samo/store-state.toml`), `worker` (one thread for every store job), installs
    and uninstalls in the host, and `update` (Samo's zip unpacked with `ditto` and checked with `plutil` and
    `codesign`).
  - Packages are read by a small tar reader over `flate2`, which checks every entry before writing it and never
    creates a link.
  - An update installs beside the old version and goes back if the new helper doesn't say hello. Replacing a dev
    build keeps `data/` and the switch and runs neither `off` nor `uninstall`, so Claude Sessions stays connected.
  - `samo-gpui --finish-update` waits for the old Samo by its process identity, trashes it, puts the new one in
    place and opens it, or puts the old one back.
  - `xtask ext-keygen`, `ext-package`, `ext-publish`; the store's first contents in the `samo-extensions` folder.
- **Phase 2, the screens.** A text field (ported from gpui's example), the store list and detail page, Settings with
  Updates, a sidebar count of updates from the cache.
- **Checks by hand, against a store served on this Mac with a test key, in a temporary home:**
  - **M1:** the list and a detail page, dark and light; search; offline (*Offline: showing the store as of
    19:19*). At launch: no request at all.
  - **M2:** Install, Update (0.1.0 to 0.1.1, old version folder gone) and Uninstall (asks first; nothing left
    under `Extensions/hello`).
  - **M3:** 20 detail pages opened and closed: memory 106.7 MB before, 97.8 MB after.
  - **M4:** Settings offered 0.1.2; Install and restart downloaded it, the old Samo quit, its app went to the
    Trash, 0.1.2 opened in its place, within a second.
- **Verification:** `scripts/ci-local.sh` (215 tests), including `store_phase2_demo`, the plan's Task 12 demo.
- **Versions:** Samo 0.2.0, Claude Sessions 0.2.0, Hello 0.2.0 (both `min_samo` 0.2.0).
