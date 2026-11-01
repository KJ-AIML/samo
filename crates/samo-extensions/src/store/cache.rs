//! The store's cache (R13), `~/Library/Caches/Samo/Store/`: the last verified catalog for each store with its
//! signature, and images under their SHA-256. Least recently used images go first once it passes its cap.
//! Everything read back is verified again, so a changed cache is never trusted.

use samo_domain::store::{CacheEntry, evict, sha256_hex};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Cache {
    dir: PathBuf,
    cap: u64,
}

fn ms(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// What the cache holds for one store.
pub struct CachedCatalog {
    pub bytes: Vec<u8>,
    pub sig: String,
    pub fetched_at_ms: i64,
}

impl Cache {
    pub fn at(dir: &Path, cap: u64) -> Self {
        Self {
            dir: dir.to_path_buf(),
            cap,
        }
    }

    fn catalog_stem(&self, url: &str) -> PathBuf {
        let tag = sha256_hex(url.as_bytes());
        self.dir
            .join(format!("catalog-{}", tag.get(..12).unwrap_or_default()))
    }

    fn images(&self) -> PathBuf {
        self.dir.join("images")
    }

    pub fn catalog(&self, url: &str) -> Option<CachedCatalog> {
        let stem = self.catalog_stem(url);
        let bytes = fs::read(stem.with_extension("json")).ok()?;
        let sig = fs::read_to_string(stem.with_extension("sig")).ok()?;
        let fetched_at_ms = fs::read_to_string(stem.with_extension("fetched"))
            .ok()
            .and_then(|t| t.trim().parse().ok())
            .unwrap_or(0);
        Some(CachedCatalog {
            bytes,
            sig,
            fetched_at_ms,
        })
    }

    pub fn put_catalog(
        &self,
        url: &str,
        bytes: &[u8],
        sig: &str,
        fetched_at_ms: i64,
    ) -> Result<(), String> {
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let stem = self.catalog_stem(url);
        write_atomic(&stem.with_extension("json"), bytes)?;
        write_atomic(&stem.with_extension("sig"), sig.as_bytes())?;
        write_atomic(
            &stem.with_extension("fetched"),
            fetched_at_ms.to_string().as_bytes(),
        )
    }

    /// An image by its SHA-256, marked as just used.
    pub fn image(&self, sha256: &str) -> Option<Vec<u8>> {
        let path = self.images().join(sha256);
        let bytes = fs::read(&path).ok()?;
        if let Ok(f) = fs::File::options().write(true).open(&path) {
            let _ = f.set_modified(SystemTime::now());
        }
        Some(bytes)
    }

    /// Keeps a verified image, then trims the cache to its cap.
    pub fn put_image(&self, sha256: &str, bytes: &[u8]) -> Result<(), String> {
        fs::create_dir_all(self.images()).map_err(|e| e.to_string())?;
        write_atomic(&self.images().join(sha256), bytes)?;
        self.trim()
    }

    pub fn drop_image(&self, sha256: &str) {
        let _ = fs::remove_file(self.images().join(sha256));
    }

    /// Bytes held: catalogs and images.
    pub fn size(&self) -> u64 {
        let sum = |dir: &Path| -> u64 {
            fs::read_dir(dir)
                .map(|d| {
                    d.filter_map(Result::ok)
                        .filter_map(|e| e.metadata().ok())
                        .filter(|m| m.is_file())
                        .map(|m| m.len())
                        .fold(0, u64::saturating_add)
                })
                .unwrap_or(0)
        };
        sum(&self.dir).saturating_add(sum(&self.images()))
    }

    fn trim(&self) -> Result<(), String> {
        let entries: Vec<CacheEntry> = fs::read_dir(self.images())
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                Some(CacheEntry {
                    key: e.file_name().to_string_lossy().into_owned(),
                    size: meta.len(),
                    used_at_ms: meta.modified().map(ms).unwrap_or(0),
                })
            })
            .collect();
        let images: u64 = entries.iter().map(|e| e.size).fold(0, u64::saturating_add);
        let catalogs = self.size().saturating_sub(images);
        for key in evict(&entries, self.cap.saturating_sub(catalogs)) {
            self.drop_image(&key);
        }
        Ok(())
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}
