//! The store (ADR-003): the signed catalog, its images and the cache (R11, R12, R13).
//!
//! Nothing here goes online without a [`UserAction`]. What was fetched before is kept in the cache and can be
//! shown without one, verified again each time it is read.

mod cache;
mod state;

use crate::net::{Net, NetError, UserAction};
use cache::Cache;
use samo_domain::store::{
    CACHE_CAP, CATALOG_CAP, Catalog, FileRef, ICON_CAP, ICON_MAX, SHOT_CAP, SHOT_MAX, StoreError,
    check_replay, is_fresh, resolve, unhex, verify_catalog, verify_file, verify_image,
};
use state::Floors;
use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Where Samo's store lives.
pub const STORE_URL: &str =
    "https://github.com/KJ-AIML/samo-extensions/releases/download/store/catalog.json";

/// The public half of KJ's store key (`cargo run -p xtask -- ext-keygen`). All zeros until KJ makes the key: a
/// Samo built with it trusts no catalog at all.
pub const STORE_PUBLIC_KEY: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// The longest a catalog's signature file may be: 128 hex characters and a newline or two.
const SIG_CAP: u64 = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreFail {
    /// This build of Samo has no store key.
    NoKey,
    Net(NetError),
    Bad(StoreError),
    /// The cache or the state file couldn't be written.
    Disk(String),
}

impl fmt::Display for StoreFail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreFail::NoKey => write!(f, "this build of Samo has no key for the store"),
            StoreFail::Net(e) => write!(f, "{e}"),
            StoreFail::Bad(e) => write!(f, "{e}"),
            StoreFail::Disk(e) => write!(f, "couldn't save the store's files ({e})"),
        }
    }
}

impl From<NetError> for StoreFail {
    fn from(e: NetError) -> Self {
        StoreFail::Net(e)
    }
}

impl From<StoreError> for StoreFail {
    fn from(e: StoreError) -> Self {
        StoreFail::Bad(e)
    }
}

/// Which kind of image, for its caps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Icon,
    Screenshot,
}

impl ImageKind {
    fn caps(self) -> (u64, (u32, u32)) {
        match self {
            ImageKind::Icon => (ICON_CAP, ICON_MAX),
            ImageKind::Screenshot => (SHOT_CAP, SHOT_MAX),
        }
    }
}

/// A verified catalog, and when it was fetched (ms since the epoch).
pub type Fetched = (Arc<Catalog>, i64);

pub struct Store {
    url: String,
    key: [u8; 32],
    net: Net,
    cache_dir: PathBuf,
    cache: Cache,
    floors: Floors,
    memo: Mutex<Option<Fetched>>,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl Store {
    pub fn new(url: &str, key: [u8; 32], cache_dir: &Path, state_file: &Path) -> Self {
        Self {
            url: url.into(),
            key,
            net: Net::default(),
            cache_dir: cache_dir.to_path_buf(),
            cache: Cache::at(cache_dir, CACHE_CAP),
            floors: Floors::at(state_file),
            memo: Mutex::new(None),
        }
    }

    /// Samo's own store: `SAMO_EXT_CATALOG_URL` or [`STORE_URL`], the built-in key, and the cache and state file
    /// under the home folder. `None` without a home.
    pub fn from_env() -> Option<Self> {
        let home = PathBuf::from(env::var_os("HOME").filter(|h| !h.is_empty())?);
        let url = env::var("SAMO_EXT_CATALOG_URL")
            .ok()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| STORE_URL.into());
        let mut key = [0u8; 32];
        if let Some(bytes) = unhex(STORE_PUBLIC_KEY).filter(|b| b.len() == 32) {
            key.copy_from_slice(&bytes);
        }
        Some(Self::new(
            &url,
            key,
            &home.join("Library/Caches/Samo/Store"),
            &home.join("Library/Application Support/Samo/store-state.toml"),
        ))
    }

    /// Tests use a shorter wait for a quiet server.
    pub fn with_net(mut self, net: Net) -> Self {
        self.net = net;
        self
    }

    /// Tests use a smaller cache.
    pub fn with_cache_cap(mut self, cap: u64) -> Self {
        self.cache = Cache::at(&self.cache_dir, cap);
        self
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    fn memo(&self) -> std::sync::MutexGuard<'_, Option<Fetched>> {
        self.memo.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn check(&self, bytes: &[u8], sig: &str) -> Result<Catalog, StoreFail> {
        if self.key == [0u8; 32] {
            return Err(StoreFail::NoKey);
        }
        let catalog = verify_catalog(bytes, sig, &self.key)?;
        check_replay(&catalog.published_at, self.floors.get(&self.url).as_deref())?;
        Ok(catalog)
    }

    /// The last catalog fetched, from memory or the cache, verified again. Never goes online.
    pub fn cached(&self) -> Option<Fetched> {
        if let Some(f) = self.memo().clone() {
            return Some(f);
        }
        let c = self.cache.catalog(&self.url)?;
        let catalog = self.check(&c.bytes, &c.sig).ok()?;
        let fetched = (Arc::new(catalog), c.fetched_at_ms);
        *self.memo() = Some(fetched.clone());
        Some(fetched)
    }

    /// The catalog, fetched unless the cached one is under 10 minutes old (`force` fetches anyway).
    pub fn refresh(&self, action: &UserAction, force: bool) -> Result<Fetched, StoreFail> {
        if !force && let Some(f) = self.cached().filter(|(_, at)| is_fresh(*at, now_ms())) {
            return Ok(f);
        }
        if self.key == [0u8; 32] {
            return Err(StoreFail::NoKey);
        }
        let bytes = self.net.fetch(&self.url, CATALOG_CAP, action)?;
        let sig = self
            .net
            .fetch(&format!("{}.sig", self.url), SIG_CAP, action)?;
        let sig = String::from_utf8_lossy(&sig).trim().to_string();
        let catalog = self.check(&bytes, &sig)?;
        let at = now_ms();
        self.floors
            .raise(&self.url, &catalog.published_at)
            .map_err(StoreFail::Disk)?;
        self.cache
            .put_catalog(&self.url, &bytes, &sig, at)
            .map_err(StoreFail::Disk)?;
        let fetched = (Arc::new(catalog), at);
        *self.memo() = Some(fetched.clone());
        Ok(fetched)
    }

    /// An image the cache already holds, verified again. Never goes online.
    pub fn cached_image(&self, r: &FileRef, kind: ImageKind) -> Option<Vec<u8>> {
        let (cap, max) = kind.caps();
        let bytes = self.cache.image(&r.sha256)?;
        if verify_image(&r.file, &bytes, r, cap, max).is_err() {
            self.cache.drop_image(&r.sha256);
            return None;
        }
        Some(bytes)
    }

    /// An image, from the cache or the store. Its hash and its pixel size, read from its header, are checked
    /// before it is kept or handed on to be decoded (R11, R13).
    pub fn image(
        &self,
        r: &FileRef,
        kind: ImageKind,
        action: &UserAction,
    ) -> Result<Vec<u8>, StoreFail> {
        if let Some(bytes) = self.cached_image(r, kind) {
            return Ok(bytes);
        }
        let (cap, max) = kind.caps();
        let bytes = self.net.fetch(&resolve(&self.url, &r.file), cap, action)?;
        verify_image(&r.file, &bytes, r, cap, max)?;
        self.cache
            .put_image(&r.sha256, &bytes)
            .map_err(StoreFail::Disk)?;
        Ok(bytes)
    }

    /// A package or Samo's zip, checked against its hash. Not cached.
    pub fn file(
        &self,
        what: &str,
        r: &FileRef,
        cap: u64,
        action: &UserAction,
    ) -> Result<Vec<u8>, StoreFail> {
        let bytes = self.net.fetch(&resolve(&self.url, &r.file), cap, action)?;
        verify_file(what, &bytes, r, cap)?;
        Ok(bytes)
    }

    /// Bytes the cache holds now.
    pub fn cache_size(&self) -> u64 {
        self.cache.size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use samo_testkit::store::{FakeStore, catalog, listing, png};
    use std::time::Duration;

    struct Fixture {
        shop: FakeStore,
        home: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Result<Self, String> {
            Ok(Self {
                shop: FakeStore::new()?,
                home: tempfile::tempdir().map_err(|e| e.to_string())?,
            })
        }

        /// A Store as Samo would make it, on this test's folders; a new one is a restarted Samo.
        fn store(&self) -> Store {
            Store::new(
                &self.shop.catalog_url(),
                self.shop.public_key(),
                &self.home.path().join("Caches/Store"),
                &self.home.path().join("store-state.toml"),
            )
            .with_net(Net::with_idle(Duration::from_secs(2)))
        }

        /// A catalog with one listing whose icon is `icon`.
        fn publish_with_icon(&self, at: &str, icon: FileRef) -> Result<(), String> {
            let pkg = self.shop.put("hello-0.2.0-macos-arm64.tar.gz", b"pkg")?;
            let mut l = listing("hello", "0.2.0", pkg);
            l.icon = Some(icon);
            self.shop.publish(&catalog(at, vec![l]))
        }
    }

    #[test]
    fn store_rejects_bad_signature_replay_and_mismatched_files() -> Result<(), String> {
        let f = Fixture::new()?;
        let click = UserAction::clicked();
        let icon = f.shop.put("hello-icon.png", &png(128, 128))?;
        f.publish_with_icon("2026-11-02T10:00:00Z", icon.clone())?;

        // A signature from another key.
        let other = Store::new(
            &f.shop.catalog_url(),
            [3u8; 32],
            &f.home.path().join("Other"),
            &f.home.path().join("other.toml"),
        );
        assert_eq!(
            other.refresh(&click, true),
            Err(StoreFail::Bad(StoreError::BadSignature))
        );
        // A catalog changed after signing.
        let good =
            std::fs::read(f.shop.dir.path().join("catalog.json")).map_err(|e| e.to_string())?;
        let mut changed = good.clone();
        changed.extend_from_slice(b" ");
        f.shop.tamper("catalog.json", &changed)?;
        let store = f.store();
        assert_eq!(
            store.refresh(&click, true),
            Err(StoreFail::Bad(StoreError::BadSignature))
        );
        assert!(store.cached().is_none(), "nothing unverified is kept");
        f.shop.tamper("catalog.json", &good)?;
        let (catalog, _) = store.refresh(&click, true).map_err(|e| e.to_string())?;
        assert_eq!(catalog.extensions[0].id, "hello");

        // An older catalog, properly signed, is refused once a newer one was seen, even by a restarted Samo.
        f.publish_with_icon("2026-11-01T10:00:00Z", icon.clone())?;
        assert!(matches!(
            f.store().refresh(&click, true),
            Err(StoreFail::Bad(StoreError::Replay { .. }))
        ));

        // An image that isn't the one the catalog pins is refused and never cached.
        f.publish_with_icon("2026-11-03T10:00:00Z", icon.clone())?;
        let store = f.store();
        store.refresh(&click, true).map_err(|e| e.to_string())?;
        f.shop.tamper("hello-icon.png", &png(128, 129))?;
        assert!(matches!(
            store.image(&icon, ImageKind::Icon, &click),
            Err(StoreFail::Bad(StoreError::HashMismatch(_)))
        ));
        assert!(store.cached_image(&icon, ImageKind::Icon).is_none());
        Ok(())
    }

    #[test]
    fn store_fetch_needs_user_action() -> Result<(), String> {
        let f = Fixture::new()?;
        let icon = f.shop.put("hello-icon.png", &png(64, 64))?;
        f.publish_with_icon("2026-11-02T10:00:00Z", icon.clone())?;

        // Everything that needs no click stays offline, at launch and after.
        let store = f.store();
        assert!(store.cached().is_none());
        assert!(store.cached_image(&icon, ImageKind::Icon).is_none());
        assert!(
            f.shop.server.hits().is_empty(),
            "nothing goes online without a click"
        );

        let click = UserAction::clicked();
        store.refresh(&click, false).map_err(|e| e.to_string())?;
        store
            .image(&icon, ImageKind::Icon, &click)
            .map_err(|e| e.to_string())?;
        assert_eq!(
            f.shop.server.hits(),
            ["/catalog.json", "/catalog.json.sig", "/hello-icon.png"]
        );

        // A restarted Samo shows the cache without going online.
        let restarted = f.store();
        assert!(restarted.cached().is_some());
        assert!(restarted.cached_image(&icon, ImageKind::Icon).is_some());
        // And a click within 10 minutes uses it too.
        restarted
            .refresh(&click, false)
            .map_err(|e| e.to_string())?;
        assert_eq!(f.shop.server.hits().len(), 3);
        Ok(())
    }

    #[test]
    fn image_dimensions_checked_before_decoding() -> Result<(), String> {
        let f = Fixture::new()?;
        let click = UserAction::clicked();
        // A screenshot whose header claims more pixels than Samo allows, pinned by a (signed) catalog.
        let huge = f.shop.put("huge.png", &png(20_000, 20_000))?;
        let wide = f.shop.put("wide.png", &png(2561, 100))?;
        let fine = f.shop.put("fine.png", &png(2560, 1600))?;
        let mut lied = f.shop.put("lied.png", &png(800, 600))?;
        lied.width = Some(400);
        let junk = f.shop.put("junk.png", b"<html>not an image</html>")?;
        let pkg = f.shop.put("hello-0.2.0-macos-arm64.tar.gz", b"pkg")?;
        f.shop.publish(&catalog(
            "2026-11-02T10:00:00Z",
            vec![listing("hello", "0.2.0", pkg)],
        ))?;
        let store = f.store();
        store.refresh(&click, true).map_err(|e| e.to_string())?;

        for bad in [&huge, &wide] {
            assert!(matches!(
                store.image(bad, ImageKind::Screenshot, &click),
                Err(StoreFail::Bad(StoreError::ImageTooLarge { .. }))
            ));
        }
        assert!(matches!(
            store.image(&lied, ImageKind::Screenshot, &click),
            Err(StoreFail::Bad(StoreError::ImageMismatch(_)))
        ));
        assert!(matches!(
            store.image(&junk, ImageKind::Screenshot, &click),
            Err(StoreFail::Bad(StoreError::NotAnImage(_)))
        ));
        // A screenshot's size is too big for an icon.
        assert!(matches!(
            store.image(&fine, ImageKind::Icon, &click),
            Err(StoreFail::Bad(StoreError::ImageTooLarge { .. }))
        ));
        assert!(store.image(&fine, ImageKind::Screenshot, &click).is_ok());
        for bad in [&huge, &wide, &lied, &junk] {
            assert!(store.cached_image(bad, ImageKind::Screenshot).is_none());
        }
        Ok(())
    }

    #[test]
    fn oversized_files_are_refused() -> Result<(), String> {
        let f = Fixture::new()?;
        let click = UserAction::clicked();
        let mut big_icon = png(256, 256);
        big_icon.resize(usize::try_from(ICON_CAP).unwrap_or(0) + 1, 0);
        let icon = f.shop.put("big-icon.png", &big_icon)?;
        f.publish_with_icon("2026-11-02T10:00:00Z", icon.clone())?;
        let store = f.store();
        store.refresh(&click, true).map_err(|e| e.to_string())?;
        assert!(matches!(
            store.image(&icon, ImageKind::Icon, &click),
            Err(StoreFail::Net(NetError::TooBig { .. }))
        ));
        let pkg = FileRef {
            file: "big-icon.png".into(),
            size: None,
            sha256: icon.sha256.clone(),
            width: None,
            height: None,
        };
        assert!(matches!(
            store.file("hello", &pkg, 1024, &click),
            Err(StoreFail::Net(NetError::TooBig { .. }))
        ));

        // A catalog over 1 MB is refused before it is read.
        let mut l = listing("hello", "0.2.0", icon);
        l.description = "x".repeat(usize::try_from(CATALOG_CAP).unwrap_or(0));
        f.shop.publish(&catalog("2026-11-03T10:00:00Z", vec![l]))?;
        assert!(matches!(
            store.refresh(&click, true),
            Err(StoreFail::Net(NetError::TooBig { .. }))
        ));
        Ok(())
    }

    #[test]
    fn cache_stays_under_its_cap() -> Result<(), String> {
        let f = Fixture::new()?;
        let click = UserAction::clicked();
        let shot = |n: u32| -> Result<FileRef, String> {
            let mut bytes = png(1000 + n, 800);
            bytes.resize(300 * 1024, 0);
            f.shop.put(&format!("shot{n}.png"), &bytes)
        };
        let shots: Vec<FileRef> = (0..6).map(shot).collect::<Result<_, _>>()?;
        let pkg = f.shop.put("hello-0.2.0-macos-arm64.tar.gz", b"pkg")?;
        f.shop.publish(&catalog(
            "2026-11-02T10:00:00Z",
            vec![listing("hello", "0.2.0", pkg)],
        ))?;
        let cap = 1024 * 1024;
        let store = f.store().with_cache_cap(cap);
        store.refresh(&click, true).map_err(|e| e.to_string())?;
        for s in &shots {
            store
                .image(s, ImageKind::Screenshot, &click)
                .map_err(|e| e.to_string())?;
            std::thread::sleep(Duration::from_millis(5));
            assert!(
                store.cache_size() <= cap,
                "cache at {} bytes",
                store.cache_size()
            );
        }
        // The newest stay; the oldest went first.
        assert!(
            store
                .cached_image(&shots[5], ImageKind::Screenshot)
                .is_some()
        );
        assert!(
            store
                .cached_image(&shots[0], ImageKind::Screenshot)
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn offline_uses_cache() -> Result<(), String> {
        let f = Fixture::new()?;
        let click = UserAction::clicked();
        let icon = f.shop.put("hello-icon.png", &png(64, 64))?;
        f.publish_with_icon("2026-11-02T10:00:00Z", icon.clone())?;
        let store = f.store();
        let (_, fetched_at) = store.refresh(&click, true).map_err(|e| e.to_string())?;
        store
            .image(&icon, ImageKind::Icon, &click)
            .map_err(|e| e.to_string())?;

        // The store goes away; the next Samo still has what it saw.
        let url = f.shop.catalog_url();
        let key = f.shop.public_key();
        let Fixture { shop, home } = f;
        drop(shop);
        // The same cache under another key (another build of Samo) is not trusted.
        let elsewhere = Store::new(
            &url,
            [5u8; 32],
            &home.path().join("Caches/Store"),
            &home.path().join("x.toml"),
        );
        assert!(elsewhere.cached().is_none());
        let restarted = Store::new(
            &url,
            key,
            &home.path().join("Caches/Store"),
            &home.path().join("store-state.toml"),
        )
        .with_net(Net::with_idle(Duration::from_secs(2)));
        assert!(matches!(
            restarted.refresh(&click, true),
            Err(StoreFail::Net(_))
        ));
        let (catalog, at) = restarted.cached().ok_or("the cache should be there")?;
        assert_eq!(
            (catalog.extensions[0].id.as_str(), at),
            ("hello", fetched_at)
        );
        assert!(restarted.cached_image(&icon, ImageKind::Icon).is_some());
        Ok(())
    }
}
