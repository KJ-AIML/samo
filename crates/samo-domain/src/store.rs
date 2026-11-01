//! The extension store's catalog (ADR-003): what it holds, how it is checked (R11), and the small rules around it.
//! Pure: callers pass in the bytes they fetched and the time.

use crate::extensions::numeric;
use ring::digest::{SHA256, digest};
use ring::signature::{ED25519, UnparsedPublicKey};
use samo_ext_protocol::{Change, Manifest, valid_id, valid_version};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;

/// R13's caps.
pub const CATALOG_CAP: u64 = 1024 * 1024;
pub const ICON_CAP: u64 = 256 * 1024;
pub const ICON_MAX: (u32, u32) = (256, 256);
pub const SHOT_CAP: u64 = 2 * 1024 * 1024;
pub const SHOT_MAX: (u32, u32) = (2560, 1600);
pub const MAX_SHOTS: usize = 6;
pub const PACKAGE_CAP: u64 = 50 * 1024 * 1024;
pub const SAMO_CAP: u64 = 100 * 1024 * 1024;
pub const CACHE_CAP: u64 = 50 * 1024 * 1024;
/// A cached catalog younger than this is used without fetching.
pub const FRESH_MS: i64 = 10 * 60 * 1_000;

/// The signed catalog (`catalog.json`, schema 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Catalog {
    pub schema: u32,
    /// RFC 3339; a catalog older than the newest one accepted is refused (R11).
    pub published_at: String,
    #[serde(default)]
    pub samo: Option<SamoRelease>,
    #[serde(default)]
    pub extensions: Vec<Listing>,
}

/// Samo's own latest release.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SamoRelease {
    pub version: String,
    #[serde(default)]
    pub whats_new: String,
    #[serde(default)]
    pub min_macos: String,
    pub package: FileRef,
}

/// One extension in the store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Listing {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    pub summary: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub homepage: String,
    pub version: String,
    pub protocol: u32,
    pub min_samo: String,
    #[serde(default)]
    pub whats_new: String,
    pub package: FileRef,
    #[serde(default)]
    pub icon: Option<FileRef>,
    #[serde(default)]
    pub screenshots: Vec<Screenshot>,
    #[serde(default)]
    pub changes: Vec<Change>,
}

/// A file in the store's release, pinned by its hash. `file` is resolved against the catalog's URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRef {
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Screenshot {
    #[serde(flatten)]
    pub file: FileRef,
    #[serde(default)]
    pub caption: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    BadSignature,
    NotACatalog(String),
    UnknownSchema(u32),
    BadListing(String),
    Replay {
        got: String,
        floor: String,
    },
    TooBig {
        what: String,
        size: u64,
        cap: u64,
    },
    SizeMismatch(String),
    HashMismatch(String),
    NotAnImage(String),
    ImageTooLarge {
        what: String,
        width: u32,
        height: u32,
    },
    ImageMismatch(String),
    ManifestMismatch(&'static str),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadSignature => write!(f, "the store's catalog isn't signed by Samo's key"),
            Self::NotACatalog(e) => write!(f, "the store's catalog can't be read: {e}"),
            Self::UnknownSchema(s) => write!(
                f,
                "the store's catalog is schema {s}; this Samo reads schema 1"
            ),
            Self::BadListing(e) => write!(f, "a listing in the store is not valid: {e}"),
            Self::Replay { got, floor } => write!(
                f,
                "the store sent an older catalog ({got}) than one already seen ({floor})"
            ),
            Self::TooBig { what, size, cap } => {
                write!(f, "{what} is {size} bytes, over the {cap}-byte limit")
            }
            Self::SizeMismatch(what) => write!(f, "{what} isn't the size the catalog says"),
            Self::HashMismatch(what) => write!(f, "{what} doesn't match the catalog's checksum"),
            Self::NotAnImage(what) => write!(f, "{what} isn't a PNG or JPEG image"),
            Self::ImageTooLarge {
                what,
                width,
                height,
            } => {
                write!(f, "{what} is {width} x {height}, larger than allowed")
            }
            Self::ImageMismatch(what) => write!(f, "{what} isn't the size the catalog says"),
            Self::ManifestMismatch(field) => {
                write!(f, "the package's {field} doesn't match the store's listing")
            }
        }
    }
}

/// The lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(digest(&SHA256, bytes).as_ref())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

fn safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

fn valid_sha(sha: &str) -> bool {
    sha.len() == 64
        && sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn check_ref(what: &str, r: &FileRef) -> Result<(), StoreError> {
    if !safe_file_name(&r.file) {
        return Err(StoreError::BadListing(format!(
            "{what}: `{}` is not a plain file name",
            r.file
        )));
    }
    if !valid_sha(&r.sha256) {
        return Err(StoreError::BadListing(format!(
            "{what}: its checksum is not SHA-256 hex"
        )));
    }
    Ok(())
}

fn check_listing(l: &Listing) -> Result<(), StoreError> {
    let bad = |why: String| Err(StoreError::BadListing(format!("{}: {why}", l.id)));
    if !valid_id(&l.id) {
        return bad("not a valid id".into());
    }
    if !valid_version(&l.version) || numeric(&l.min_samo).is_none() {
        return bad("not a valid version".into());
    }
    if l.changes.is_empty() {
        return bad("it must say what it changes on the Mac".into());
    }
    if l.screenshots.len() > MAX_SHOTS {
        return bad(format!("more than {MAX_SHOTS} screenshots"));
    }
    check_ref(&l.id, &l.package)?;
    if let Some(icon) = &l.icon {
        check_ref(&l.id, icon)?;
    }
    for s in &l.screenshots {
        check_ref(&l.id, &s.file)?;
    }
    Ok(())
}

/// R11: the catalog's signature is checked before anything in it is read.
pub fn verify_catalog(bytes: &[u8], sig_hex: &str, key: &[u8; 32]) -> Result<Catalog, StoreError> {
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size > CATALOG_CAP {
        return Err(StoreError::TooBig {
            what: "The catalog".into(),
            size,
            cap: CATALOG_CAP,
        });
    }
    let sig = unhex(sig_hex)
        .filter(|s| s.len() == 64)
        .ok_or(StoreError::BadSignature)?;
    UnparsedPublicKey::new(&ED25519, key)
        .verify(bytes, &sig)
        .map_err(|_| StoreError::BadSignature)?;
    let catalog: Catalog =
        serde_json::from_slice(bytes).map_err(|e| StoreError::NotACatalog(e.to_string()))?;
    if catalog.schema != 1 {
        return Err(StoreError::UnknownSchema(catalog.schema));
    }
    if chrono::DateTime::parse_from_rfc3339(&catalog.published_at).is_err() {
        return Err(StoreError::NotACatalog("published_at is not a date".into()));
    }
    if let Some(samo) = &catalog.samo {
        if numeric(&samo.version).is_none() {
            return Err(StoreError::BadListing(
                "Samo's version is not a version".into(),
            ));
        }
        check_ref("Samo", &samo.package)?;
    }
    for listing in &catalog.extensions {
        check_listing(listing)?;
    }
    Ok(catalog)
}

/// R11: a catalog older than the newest one accepted is refused, so an old signed catalog can't be replayed.
pub fn check_replay(published_at: &str, floor: Option<&str>) -> Result<(), StoreError> {
    let Some(floor) = floor else { return Ok(()) };
    let parse = |t: &str| chrono::DateTime::parse_from_rfc3339(t).ok();
    match (parse(published_at), parse(floor)) {
        (Some(got), Some(min)) if got < min => Err(StoreError::Replay {
            got: published_at.into(),
            floor: floor.into(),
        }),
        (Some(_), _) => Ok(()),
        (None, _) => Err(StoreError::NotACatalog("published_at is not a date".into())),
    }
}

/// R11 and R13: a file is used only if it is no larger than `cap`, has the size the catalog gives (if any), and
/// hashes to the catalog's SHA-256.
pub fn verify_file(what: &str, bytes: &[u8], r: &FileRef, cap: u64) -> Result<(), StoreError> {
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size > cap {
        return Err(StoreError::TooBig {
            what: what.into(),
            size,
            cap,
        });
    }
    if r.size.is_some_and(|s| s != size) {
        return Err(StoreError::SizeMismatch(what.into()));
    }
    if sha256_hex(bytes) != r.sha256 {
        return Err(StoreError::HashMismatch(what.into()));
    }
    Ok(())
}

/// R13: an image is checked, including its pixel size read from its header, before anything decodes it.
pub fn verify_image(
    what: &str,
    bytes: &[u8],
    r: &FileRef,
    cap: u64,
    max: (u32, u32),
) -> Result<(u32, u32), StoreError> {
    verify_file(what, bytes, r, cap)?;
    let (width, height) = image_size(bytes).ok_or_else(|| StoreError::NotAnImage(what.into()))?;
    if width > max.0 || height > max.1 || width == 0 || height == 0 {
        return Err(StoreError::ImageTooLarge {
            what: what.into(),
            width,
            height,
        });
    }
    if r.width.is_some_and(|w| w != width) || r.height.is_some_and(|h| h != height) {
        return Err(StoreError::ImageMismatch(what.into()));
    }
    Ok((width, height))
}

/// A PNG's or JPEG's width and height, from its header alone.
pub fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.starts_with(PNG) {
        if bytes.get(12..16)? != b"IHDR" {
            return None;
        }
        let be = |r: std::ops::Range<usize>| -> Option<u32> {
            Some(u32::from_be_bytes(bytes.get(r)?.try_into().ok()?))
        };
        return Some((be(16..20)?, be(20..24)?));
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2usize;
        while i + 4 <= bytes.len() {
            if bytes[i] != 0xFF {
                return None;
            }
            let marker = bytes[i + 1];
            if marker == 0xFF {
                i += 1;
                continue;
            }
            if (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
                i += 2;
                continue;
            }
            let len = usize::from(u16::from_be_bytes([*bytes.get(i + 2)?, *bytes.get(i + 3)?]));
            let start_of_frame =
                matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF);
            if start_of_frame {
                let h = u16::from_be_bytes([*bytes.get(i + 5)?, *bytes.get(i + 6)?]);
                let w = u16::from_be_bytes([*bytes.get(i + 7)?, *bytes.get(i + 8)?]);
                return Some((u32::from(w), u32::from(h)));
            }
            i += 2 + len;
        }
    }
    None
}

/// The package's manifest must say what its listing says (ADR-003 M3): what you read in the store is what you
/// install.
pub fn manifest_matches(m: &Manifest, l: &Listing) -> Result<(), StoreError> {
    if m.id != l.id {
        return Err(StoreError::ManifestMismatch("id"));
    }
    if m.version != l.version {
        return Err(StoreError::ManifestMismatch("version"));
    }
    if m.protocol != l.protocol {
        return Err(StoreError::ManifestMismatch("protocol"));
    }
    if m.min_samo != l.min_samo {
        return Err(StoreError::ManifestMismatch("min_samo"));
    }
    if m.changes != l.changes {
        return Err(StoreError::ManifestMismatch("changes"));
    }
    Ok(())
}

/// How two versions compare; an unreadable version compares as unknown.
pub fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    Some(numeric(a)?.cmp(&numeric(b)?))
}

/// Whether `candidate` is newer than `installed`.
pub fn newer(candidate: &str, installed: &str) -> bool {
    compare_versions(candidate, installed) == Some(Ordering::Greater)
}

/// The listings matching `query` (name, summary, author, categories; any case), name matches first, then by name.
/// An empty query keeps every listing in order.
pub fn rank(query: &str, listings: &[Listing]) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return (0..listings.len()).collect();
    }
    let mut scored: Vec<(u8, usize)> = listings
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            let name = l.name.to_lowercase();
            let score = if name.starts_with(&q) {
                0
            } else if name.contains(&q) {
                1
            } else if l.summary.to_lowercase().contains(&q)
                || l.author.to_lowercase().contains(&q)
                || l.categories.iter().any(|c| c.to_lowercase().contains(&q))
            {
                2
            } else {
                return None;
            };
            Some((score, i))
        })
        .collect();
    scored.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| listings[a.1].name.cmp(&listings[b.1].name))
    });
    scored.into_iter().map(|(_, i)| i).collect()
}

/// A cached file: its key, size and when it was last used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntry {
    pub key: String,
    pub size: u64,
    pub used_at_ms: i64,
}

/// R13: which entries to drop so the cache fits `cap`: the least recently used first.
pub fn evict(entries: &[CacheEntry], cap: u64) -> Vec<String> {
    let mut total: u64 = entries.iter().map(|e| e.size).fold(0, u64::saturating_add);
    let mut by_age: Vec<&CacheEntry> = entries.iter().collect();
    by_age.sort_by_key(|e| e.used_at_ms);
    let mut dropped = Vec::new();
    for e in by_age {
        if total <= cap {
            break;
        }
        total = total.saturating_sub(e.size);
        dropped.push(e.key.clone());
    }
    dropped
}

/// Whether a catalog fetched at `fetched_at_ms` can be used at `now_ms` without fetching again.
pub fn is_fresh(fetched_at_ms: i64, now_ms: i64) -> bool {
    now_ms - fetched_at_ms < FRESH_MS
}

/// A file's URL: `file` beside the catalog at `catalog_url`.
pub fn resolve(catalog_url: &str, file: &str) -> String {
    match catalog_url.rsplit_once('/') {
        Some((base, _)) => format!("{base}/{file}"),
        None => file.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn key() -> Result<Ed25519KeyPair, String> {
        Ed25519KeyPair::from_seed_unchecked(&[7u8; 32]).map_err(|e| e.to_string())
    }

    fn public(k: &Ed25519KeyPair) -> [u8; 32] {
        let mut out = [0u8; 32];
        out.copy_from_slice(k.public_key().as_ref());
        out
    }

    fn file(name: &str, bytes: &[u8]) -> FileRef {
        FileRef {
            file: name.into(),
            size: Some(u64::try_from(bytes.len()).unwrap_or(0)),
            sha256: sha256_hex(bytes),
            width: None,
            height: None,
        }
    }

    fn change() -> Change {
        Change {
            what: "Nothing".into(),
            place: "nowhere".into(),
            undo: "nothing".into(),
        }
    }

    fn listing() -> Listing {
        Listing {
            id: "hello".into(),
            name: "Hello".into(),
            author: "KJ".into(),
            summary: "Every widget".into(),
            description: String::new(),
            categories: vec!["example".into()],
            homepage: String::new(),
            version: "0.2.0".into(),
            protocol: 1,
            min_samo: "0.2.0".into(),
            whats_new: String::new(),
            package: file("hello-0.2.0-macos-arm64.tar.gz", b"package"),
            icon: None,
            screenshots: Vec::new(),
            changes: vec![change()],
        }
    }

    fn catalog_bytes(published_at: &str) -> Result<Vec<u8>, String> {
        let c = Catalog {
            schema: 1,
            published_at: published_at.into(),
            samo: None,
            extensions: vec![listing()],
        };
        serde_json::to_vec(&c).map_err(|e| e.to_string())
    }

    #[test]
    fn verifies_good_signature_and_rejects_tampering() -> Result<(), String> {
        let k = key()?;
        let bytes = catalog_bytes("2026-11-02T10:00:00Z")?;
        let sig = hex(k.sign(&bytes).as_ref());
        let c = verify_catalog(&bytes, &sig, &public(&k)).map_err(|e| e.to_string())?;
        assert_eq!(c.extensions[0].id, "hello");

        let mut tampered = bytes.clone();
        if let Some(b) = tampered.iter_mut().find(|b| **b == b'H') {
            *b = b'J';
        }
        assert_eq!(
            verify_catalog(&tampered, &sig, &public(&k)),
            Err(StoreError::BadSignature)
        );
        assert_eq!(
            verify_catalog(&bytes, "zz", &public(&k)),
            Err(StoreError::BadSignature)
        );
        let other = Ed25519KeyPair::from_seed_unchecked(&[8u8; 32]).map_err(|e| e.to_string())?;
        assert_eq!(
            verify_catalog(&bytes, &sig, &public(&other)),
            Err(StoreError::BadSignature)
        );

        // Signed but unsafe: a file name with a path in it.
        let mut bad = listing();
        bad.package.file = "../../evil".into();
        let c = Catalog {
            schema: 1,
            published_at: "2026-11-02T10:00:00Z".into(),
            samo: None,
            extensions: vec![bad],
        };
        let bytes = serde_json::to_vec(&c).map_err(|e| e.to_string())?;
        let sig = hex(k.sign(&bytes).as_ref());
        assert!(matches!(
            verify_catalog(&bytes, &sig, &public(&k)),
            Err(StoreError::BadListing(_))
        ));
        Ok(())
    }

    #[test]
    fn replay_older_catalog_refused() {
        assert!(check_replay("2026-11-02T10:00:00Z", None).is_ok());
        assert!(check_replay("2026-11-02T10:00:00Z", Some("2026-11-02T10:00:00Z")).is_ok());
        assert!(check_replay("2026-11-03T00:00:00Z", Some("2026-11-02T10:00:00Z")).is_ok());
        assert!(matches!(
            check_replay("2026-11-01T00:00:00Z", Some("2026-11-02T10:00:00Z")),
            Err(StoreError::Replay { .. })
        ));
    }

    #[test]
    fn file_hash_and_size_checked() {
        let r = file("a.tar.gz", b"abc");
        assert!(verify_file("A", b"abc", &r, 10).is_ok());
        assert_eq!(
            verify_file("A", b"abd", &r, 10),
            Err(StoreError::HashMismatch("A".into()))
        );
        assert_eq!(
            verify_file("A", b"abcd", &r, 10),
            Err(StoreError::SizeMismatch("A".into()))
        );
        assert!(matches!(
            verify_file("A", b"abc", &r, 2),
            Err(StoreError::TooBig { .. })
        ));
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0]);
        b
    }

    fn jpeg(w: u16, h: u16) -> Vec<u8> {
        let mut b = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00];
        b.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&[0; 12]);
        b
    }

    #[test]
    fn image_dimensions_from_headers() {
        assert_eq!(image_size(&png(1600, 1000)), Some((1600, 1000)));
        assert_eq!(image_size(&jpeg(640, 480)), Some((640, 480)));
        assert_eq!(image_size(b"GIF89a"), None);
        assert_eq!(image_size(&png(1, 1)[..20]), None);

        let big = png(40_000, 40_000);
        let r = file("shot.png", &big);
        assert!(matches!(
            verify_image("Shot", &big, &r, SHOT_CAP, SHOT_MAX),
            Err(StoreError::ImageTooLarge { .. })
        ));
        let ok = png(1600, 1000);
        let mut r = file("shot.png", &ok);
        assert_eq!(
            verify_image("Shot", &ok, &r, SHOT_CAP, SHOT_MAX),
            Ok((1600, 1000))
        );
        r.width = Some(1280);
        assert_eq!(
            verify_image("Shot", &ok, &r, SHOT_CAP, SHOT_MAX),
            Err(StoreError::ImageMismatch("Shot".into()))
        );
    }

    #[test]
    fn manifest_must_equal_listing_fields() -> Result<(), String> {
        let manifest = Manifest {
            id: "hello".into(),
            name: "Hello".into(),
            version: "0.2.0".into(),
            protocol: 1,
            min_samo: "0.2.0".into(),
            helper: "bin/samo-ext-hello".into(),
            summary: "Every widget".into(),
            changes: vec![change()],
            settings: Vec::new(),
        };
        let l = listing();
        manifest_matches(&manifest, &l).map_err(|e| e.to_string())?;
        let mut other = manifest.clone();
        other.version = "0.2.1".into();
        assert_eq!(
            manifest_matches(&other, &l),
            Err(StoreError::ManifestMismatch("version"))
        );
        let mut other = manifest.clone();
        other.changes[0].what = "Everything".into();
        assert_eq!(
            manifest_matches(&other, &l),
            Err(StoreError::ManifestMismatch("changes"))
        );
        Ok(())
    }

    #[test]
    fn versions_order() {
        assert!(newer("0.2.1", "0.2.0"));
        assert!(newer("1.0.0", "0.9.9"));
        assert!(!newer("0.2.0", "0.2.0"));
        assert!(!newer("0.1.9", "0.2.0"));
        assert!(!newer("x", "0.2.0"));
    }

    #[test]
    fn search_ranks_name_first() {
        let mk = |name: &str, summary: &str| Listing {
            name: name.into(),
            summary: summary.into(),
            ..listing()
        };
        let all = vec![
            mk("Docker Watch", "containers"),
            mk("Claude Sessions", "Claude Code at a glance"),
            mk("Hello", "an example that says hello"),
        ];
        assert_eq!(rank("", &all), vec![0, 1, 2]);
        assert_eq!(rank("hello", &all), vec![2]);
        assert_eq!(rank("CLAUDE", &all), vec![1]);
        assert_eq!(rank("a", &all), vec![1, 0, 2]);
        assert_eq!(rank("zzz", &all), Vec::<usize>::new());
    }

    #[test]
    fn eviction_keeps_under_cap() {
        let e = |key: &str, size, used| CacheEntry {
            key: key.into(),
            size,
            used_at_ms: used,
        };
        let entries = vec![e("new", 40, 300), e("old", 40, 100), e("mid", 40, 200)];
        assert_eq!(evict(&entries, 100), vec!["old".to_string()]);
        assert_eq!(
            evict(&entries, 40),
            vec!["old".to_string(), "mid".to_string()]
        );
        assert!(evict(&entries, 1_000).is_empty());
        assert!(is_fresh(0, FRESH_MS - 1));
        assert!(!is_fresh(0, FRESH_MS));
        assert_eq!(
            resolve(
                "https://github.com/KJ-AIML/samo-extensions/releases/download/store/catalog.json",
                "a.png"
            ),
            "https://github.com/KJ-AIML/samo-extensions/releases/download/store/a.png"
        );
    }
}
