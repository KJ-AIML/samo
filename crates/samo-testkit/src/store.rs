//! A store on this Mac for tests: a folder served over HTTP, a catalog signed with a test key, and the files it
//! pins.

use crate::http::TestServer;
use ring::signature::{Ed25519KeyPair, KeyPair};
use samo_domain::store::{Catalog, FileRef, Listing, hex, image_size, sha256_hex};
use samo_ext_protocol::{Change, Manifest};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

pub struct FakeStore {
    pub dir: TempDir,
    pub server: TestServer,
    pair: Ed25519KeyPair,
}

impl FakeStore {
    pub fn new() -> Result<Self, String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let server = TestServer::serve(dir.path()).map_err(|e| e.to_string())?;
        let pair = Ed25519KeyPair::from_seed_unchecked(&[9u8; 32]).map_err(|e| e.to_string())?;
        Ok(Self { dir, server, pair })
    }

    /// The test key's public half, for `Store::new`.
    pub fn public_key(&self) -> [u8; 32] {
        let mut key = [0u8; 32];
        key.copy_from_slice(self.pair.public_key().as_ref());
        key
    }

    pub fn catalog_url(&self) -> String {
        format!("{}/catalog.json", self.server.url())
    }

    /// Puts a file in the store and pins it: its size, its hash, and its pixel size if it is an image.
    pub fn put(&self, name: &str, bytes: &[u8]) -> Result<FileRef, String> {
        fs::write(self.dir.path().join(name), bytes).map_err(|e| e.to_string())?;
        let dims = image_size(bytes);
        Ok(FileRef {
            file: name.into(),
            size: Some(u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
            sha256: sha256_hex(bytes),
            width: dims.map(|d| d.0),
            height: dims.map(|d| d.1),
        })
    }

    /// Replaces a file without changing the catalog, as an attacker on the wire would.
    pub fn tamper(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        fs::write(self.dir.path().join(name), bytes).map_err(|e| e.to_string())
    }

    pub fn sign(&self, bytes: &[u8]) -> String {
        hex(self.pair.sign(bytes).as_ref())
    }

    /// Writes `catalog.json` and its signature.
    pub fn publish(&self, catalog: &Catalog) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(catalog).map_err(|e| e.to_string())?;
        fs::write(self.dir.path().join("catalog.json.sig"), self.sign(&bytes))
            .map_err(|e| e.to_string())?;
        fs::write(self.dir.path().join("catalog.json"), bytes).map_err(|e| e.to_string())
    }

    /// Packs `<stage>/<id>/` as `<id>-<version>-macos-arm64.tar.gz` in the store, as `ext-package` does.
    pub fn put_package(&self, stage: &Path, id: &str, version: &str) -> Result<FileRef, String> {
        let name = format!("{id}-{version}-macos-arm64.tar.gz");
        let status = Command::new("tar")
            .env("COPYFILE_DISABLE", "1")
            .arg("-C")
            .arg(stage)
            .arg("-czf")
            .arg(self.dir.path().join(&name))
            .arg(id)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("tar failed".into());
        }
        let bytes = fs::read(self.dir.path().join(&name)).map_err(|e| e.to_string())?;
        self.put(&name, &bytes)
    }
}

/// A catalog published at `published_at` with these listings.
pub fn catalog(published_at: &str, extensions: Vec<Listing>) -> Catalog {
    Catalog {
        schema: 1,
        published_at: published_at.into(),
        samo: None,
        extensions,
    }
}

/// A valid listing for `id` at `version`, pinning `package`.
pub fn listing(id: &str, version: &str, package: FileRef) -> Listing {
    Listing {
        id: id.into(),
        name: id.into(),
        author: "Samo".into(),
        summary: format!("{id} for tests"),
        description: String::new(),
        categories: Vec::new(),
        homepage: String::new(),
        version: version.into(),
        protocol: 1,
        min_samo: "0.1.0".into(),
        whats_new: String::new(),
        package,
        icon: None,
        screenshots: Vec::new(),
        changes: vec![Change {
            what: "Nothing outside its folder".into(),
            place: "-".into(),
            undo: "-".into(),
        }],
    }
}

/// The listing a package's own manifest calls for, pinning `package`.
pub fn listing_for(manifest: &Manifest, package: FileRef) -> Listing {
    Listing {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        author: "Samo".into(),
        summary: manifest.summary.clone(),
        description: String::new(),
        categories: Vec::new(),
        homepage: String::new(),
        version: manifest.version.clone(),
        protocol: manifest.protocol,
        min_samo: manifest.min_samo.clone(),
        whats_new: String::new(),
        package,
        icon: None,
        screenshots: Vec::new(),
        changes: manifest.changes.clone(),
    }
}

/// The first bytes of a PNG `width` × `height`: enough for the header check, which is all Samo reads first.
pub fn png(width: u32, height: u32) -> Vec<u8> {
    let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    b.extend_from_slice(&width.to_be_bytes());
    b.extend_from_slice(&height.to_be_bytes());
    b.extend_from_slice(&[8, 6, 0, 0, 0]);
    b
}
