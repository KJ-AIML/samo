//! The store's tooling (ADR-003 M4): `ext-keygen`, `ext-package` and `ext-publish`.
//!
//! The private key lives in KJ's login Keychain and never on a command line. `ext-publish` takes every field the
//! package itself declares (id, name, version, protocol, min_samo, summary, changes) from the package's manifest,
//! and only the store's own fields from `listing.toml`, so the store can't describe one thing and ship another.

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use samo_domain::store::{
    Catalog, FileRef, ICON_CAP, ICON_MAX, Listing, MAX_SHOTS, PACKAGE_CAP, SAMO_CAP, SHOT_CAP,
    SHOT_MAX, SamoRelease, Screenshot, hex, image_size, sha256_hex, verify_catalog, verify_file,
    verify_image,
};
use samo_ext_protocol::Manifest;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const KEYCHAIN_SERVICE: &str = "dev.talvyn.samo.store";
/// Where Samo's trusted store key is compiled in.
pub const TRUSTED_KEY_FILE: &str = "crates/samo-extensions/src/store/mod.rs";
pub const KEYCHAIN_ACCOUNT: &str = "catalog-signing";

fn io<E: std::fmt::Display>(what: &str) -> impl Fn(E) -> String + '_ {
    move |e| format!("{what}: {e}")
}

/// `cargo run -p xtask -- ext-keygen`: a new ed25519 key, the private half into the Keychain, the public half printed.
pub fn keygen() -> Result<(), String> {
    if security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .is_ok()
    {
        return Err(format!(
            "a store signing key already exists in your Keychain ({KEYCHAIN_SERVICE}); Samo never overwrites it"
        ));
    }
    let pkcs8 =
        Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).map_err(|_| "couldn't make a key")?;
    let pair =
        Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| "couldn't read the new key")?;
    security_framework::passwords::set_generic_password(
        KEYCHAIN_SERVICE,
        KEYCHAIN_ACCOUNT,
        pkcs8.as_ref(),
    )
    .map_err(io("couldn't store the key in your Keychain"))?;
    println!("The store's signing key is in your Keychain ({KEYCHAIN_SERVICE}).");
    println!("Its public key, for STORE_PUBLIC_KEY in {TRUSTED_KEY_FILE}:");
    println!("{}", hex(pair.public_key().as_ref()));
    Ok(())
}

fn field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

/// `cargo run -p xtask -- ext-package <dir>`: builds an extension's helper and writes `target/ext-dist/<id>-<v>-macos-arm64.tar.gz`.
pub fn package(root: &Path, dir: &Path) -> Result<PathBuf, String> {
    let manifest_text =
        fs::read_to_string(dir.join("extension.toml")).map_err(io("extension.toml"))?;
    let manifest = Manifest::parse(&manifest_text).map_err(|e| e.to_string())?;
    let cargo = fs::read_to_string(dir.join("Cargo.toml")).map_err(io("Cargo.toml"))?;
    let package = field(&cargo, "name").ok_or("Cargo.toml has no package name")?;
    let bin = Path::new(&manifest.helper)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or("the manifest's helper has no file name")?;
    let status = Command::new("cargo")
        .args(["build", "--release", "-p", &package, "--bin", &bin])
        .current_dir(root)
        .status()
        .map_err(io("cargo build"))?;
    if !status.success() {
        return Err(format!("building {package} failed"));
    }
    let stage = root.join("target/ext-dist/stage");
    let _ = fs::remove_dir_all(stage.join(&manifest.id));
    let helper_dest = stage.join(&manifest.id).join(&manifest.helper);
    if let Some(parent) = helper_dest.parent() {
        fs::create_dir_all(parent).map_err(io("staging"))?;
    }
    fs::write(
        stage.join(&manifest.id).join("extension.toml"),
        &manifest_text,
    )
    .map_err(io("staging"))?;
    // A fresh copy, not cargo's hard link: Samo refuses hard links in a package (R5).
    fs::write(
        &helper_dest,
        fs::read(root.join("target/release").join(&bin)).map_err(io("the helper"))?,
    )
    .map_err(io("staging"))?;
    let status = Command::new("chmod")
        .arg("755")
        .arg(&helper_dest)
        .status()
        .map_err(io("chmod"))?;
    if !status.success() {
        return Err("couldn't make the helper executable".into());
    }
    let out = root.join("target/ext-dist").join(format!(
        "{}-{}-macos-arm64.tar.gz",
        manifest.id, manifest.version
    ));
    let status = Command::new("tar")
        .env("COPYFILE_DISABLE", "1")
        .arg("-C")
        .arg(&stage)
        .arg("-czf")
        .arg(&out)
        .arg(&manifest.id)
        .status()
        .map_err(io("tar"))?;
    if !status.success() {
        return Err("tar failed".into());
    }
    let bytes = fs::read(&out).map_err(io("the package"))?;
    println!(
        "{}  {} ({} bytes)",
        sha256_hex(&bytes),
        out.display(),
        bytes.len()
    );
    Ok(out)
}

/// `listing.toml`: only the store's own fields. A field the manifest owns must not be here with another value.
#[derive(Debug, Deserialize)]
pub struct ListingFile {
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub whats_new: String,
    #[serde(default)]
    pub screenshots: Vec<Caption>,
    // Manifest fields, allowed only if equal.
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub summary: Option<String>,
    pub min_samo: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Caption {
    pub file: String,
    #[serde(default)]
    pub caption: String,
}

/// The listing's catalog entry: the manifest's fields, then the store's.
pub fn merge(
    manifest: &Manifest,
    listing: &ListingFile,
    package: FileRef,
) -> Result<Listing, String> {
    let same = |name: &str, theirs: &Option<String>, ours: &str| match theirs {
        Some(v) if v != ours => Err(format!(
            "{}: listing.toml says {name} = \"{v}\", the package says \"{ours}\"",
            manifest.id
        )),
        _ => Ok(()),
    };
    same("id", &listing.id, &manifest.id)?;
    same("name", &listing.name, &manifest.name)?;
    same("version", &listing.version, &manifest.version)?;
    same("summary", &listing.summary, &manifest.summary)?;
    same("min_samo", &listing.min_samo, &manifest.min_samo)?;
    Ok(Listing {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        author: listing.author.clone(),
        summary: manifest.summary.clone(),
        description: listing.description.clone(),
        categories: listing.categories.clone(),
        homepage: listing.homepage.clone(),
        version: manifest.version.clone(),
        protocol: manifest.protocol,
        min_samo: manifest.min_samo.clone(),
        whats_new: listing.whats_new.clone(),
        package,
        icon: None,
        screenshots: Vec::new(),
        changes: manifest.changes.clone(),
    })
}

/// Checks an image for the store and names it by its hash: `<id>-<kind>-<sha8>.<ext>`.
pub fn image_ref(
    id: &str,
    kind: &str,
    bytes: &[u8],
    cap: u64,
    max: (u32, u32),
) -> Result<FileRef, String> {
    let what = format!("{id}'s {kind}");
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size > cap {
        return Err(format!("{what} is {size} bytes, over the {cap}-byte limit"));
    }
    let (width, height) = image_size(bytes).ok_or_else(|| format!("{what} isn't a PNG or JPEG"))?;
    if width > max.0 || height > max.1 {
        return Err(format!(
            "{what} is {width} x {height}; at most {} x {}",
            max.0, max.1
        ));
    }
    let sha = sha256_hex(bytes);
    let ext = if bytes.starts_with(b"\x89PNG") {
        "png"
    } else {
        "jpg"
    };
    Ok(FileRef {
        file: format!("{id}-{kind}-{}.{ext}", sha.get(..8).unwrap_or("")),
        size: Some(size),
        sha256: sha,
        width: Some(width),
        height: Some(height),
    })
}

/// The manifest inside a package, read without unpacking it.
fn manifest_in(package: &Path) -> Result<Manifest, String> {
    let list = Command::new("tar")
        .arg("-tzf")
        .arg(package)
        .output()
        .map_err(io("tar"))?;
    let names = String::from_utf8_lossy(&list.stdout);
    let entry = names
        .lines()
        .find(|n| n.ends_with("/extension.toml") && n.matches('/').count() == 1)
        .ok_or_else(|| format!("{} has no <id>/extension.toml", package.display()))?;
    let out = Command::new("tar")
        .arg("-xOzf")
        .arg(package)
        .arg(entry)
        .output()
        .map_err(io("tar"))?;
    Manifest::parse(&String::from_utf8_lossy(&out.stdout)).map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize, Default)]
struct SamoNotes {
    #[serde(default)]
    whats_new: String,
    #[serde(default)]
    min_macos: String,
}

/// Samo's version, from the `Info.plist` inside its zip.
fn samo_version(zip: &Path, scratch: &Path) -> Result<String, String> {
    let _ = fs::remove_dir_all(scratch);
    fs::create_dir_all(scratch).map_err(io("scratch"))?;
    let status = Command::new("ditto")
        .args(["-x", "-k"])
        .arg(zip)
        .arg(scratch)
        .status()
        .map_err(io("ditto"))?;
    if !status.success() {
        return Err("couldn't unpack Samo's zip".into());
    }
    let out = Command::new("plutil")
        .args(["-extract", "CFBundleShortVersionString", "raw"])
        .arg(scratch.join("Samo.app/Contents/Info.plist"))
        .output()
        .map_err(io("plutil"))?;
    let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || version.is_empty() {
        return Err("Samo's zip has no Samo.app with a version".into());
    }
    Ok(version)
}

pub struct PublishArgs {
    /// The key Samo trusts (`STORE_PUBLIC_KEY`); signing with another is refused. `None` with a test key.
    pub trusted: Option<[u8; 32]>,
    pub store: PathBuf,
    pub packages: PathBuf,
    pub samo: Option<PathBuf>,
    pub out: PathBuf,
    pub test_key: Option<PathBuf>,
    pub published_at: String,
}

/// `cargo run -p xtask -- ext-publish`: checks every listing, writes `dist/`, signs the catalog and verifies the result.
pub fn publish(args: &PublishArgs) -> Result<(), String> {
    let out = &args.out;
    let _ = fs::remove_dir_all(out);
    fs::create_dir_all(out).map_err(io("dist"))?;
    let copy = |name: &str, bytes: &[u8]| fs::write(out.join(name), bytes).map_err(io("dist"));

    let mut packages: Vec<PathBuf> = fs::read_dir(&args.packages)
        .map_err(io("the packages folder"))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".tar.gz"))
        .collect();
    packages.sort();

    let mut ids: Vec<String> = fs::read_dir(args.store.join("extensions"))
        .map_err(io("the store's extensions folder"))?
        .filter_map(Result::ok)
        .filter(|e| e.path().join("listing.toml").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    ids.sort();

    let mut listings = Vec::new();
    for id in &ids {
        let dir = args.store.join("extensions").join(id);
        let text = fs::read_to_string(dir.join("listing.toml")).map_err(io("listing.toml"))?;
        let listing: ListingFile =
            toml::from_str(&text).map_err(|e| format!("{id}/listing.toml: {}", e.message()))?;
        // The newest package for this id.
        let mut best: Option<(Manifest, PathBuf)> = None;
        for p in &packages {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !name.starts_with(&format!("{id}-")) {
                continue;
            }
            let m = manifest_in(p)?;
            if m.id == *id
                && best
                    .as_ref()
                    .is_none_or(|(b, _)| samo_domain::store::newer(&m.version, &b.version))
            {
                best = Some((m, p.clone()));
            }
        }
        let (manifest, path) =
            best.ok_or_else(|| format!("{id}: no package in {}", args.packages.display()))?;
        let bytes = fs::read(&path).map_err(io("a package"))?;
        let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if size > PACKAGE_CAP {
            return Err(format!("{id}'s package is over {PACKAGE_CAP} bytes"));
        }
        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        copy(&file, &bytes)?;
        let mut entry = merge(
            &manifest,
            &listing,
            FileRef {
                file,
                size: Some(size),
                sha256: sha256_hex(&bytes),
                width: None,
                height: None,
            },
        )?;
        if let Ok(icon) = fs::read(dir.join("icon.png")) {
            let r = image_ref(id, "icon", &icon, ICON_CAP, ICON_MAX)?;
            copy(&r.file, &icon)?;
            entry.icon = Some(r);
        }
        let mut shots: Vec<PathBuf> = fs::read_dir(dir.join("screenshots"))
            .map(|d| d.filter_map(Result::ok).map(|e| e.path()).collect())
            .unwrap_or_default();
        shots.sort();
        if shots.len() > MAX_SHOTS {
            return Err(format!(
                "{id} has {} screenshots; at most {MAX_SHOTS}",
                shots.len()
            ));
        }
        for (n, shot) in shots.iter().enumerate() {
            let bytes = fs::read(shot).map_err(io("a screenshot"))?;
            let r = image_ref(id, &format!("shot{}", n + 1), &bytes, SHOT_CAP, SHOT_MAX)?;
            copy(&r.file, &bytes)?;
            let name = shot
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let caption = listing
                .screenshots
                .iter()
                .find(|c| c.file == name)
                .map(|c| c.caption.clone())
                .unwrap_or_default();
            entry.screenshots.push(Screenshot { file: r, caption });
        }
        listings.push(entry);
    }

    let samo = match &args.samo {
        Some(zip) => {
            let bytes = fs::read(zip).map_err(io("Samo's zip"))?;
            let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            if size > SAMO_CAP {
                return Err(format!("Samo's zip is over {SAMO_CAP} bytes"));
            }
            let version = samo_version(zip, &out.join(".scratch"))?;
            let _ = fs::remove_dir_all(out.join(".scratch"));
            let notes: SamoNotes = fs::read_to_string(args.store.join("samo.toml"))
                .ok()
                .and_then(|t| toml::from_str(&t).ok())
                .unwrap_or_default();
            let file = format!("Samo-{version}-macos-arm64.zip");
            copy(&file, &bytes)?;
            Some(SamoRelease {
                version,
                whats_new: notes.whats_new,
                min_macos: notes.min_macos,
                package: FileRef {
                    file,
                    size: Some(size),
                    sha256: sha256_hex(&bytes),
                    width: None,
                    height: None,
                },
            })
        }
        None => None,
    };

    let catalog = Catalog {
        schema: 1,
        published_at: args.published_at.clone(),
        samo,
        extensions: listings,
    };
    let bytes = serde_json::to_vec_pretty(&catalog).map_err(|e| e.to_string())?;
    let pkcs8 = match &args.test_key {
        Some(path) => fs::read(path).map_err(io("the test key"))?,
        None => security_framework::passwords::get_generic_password(
            KEYCHAIN_SERVICE,
            KEYCHAIN_ACCOUNT,
        )
        .map_err(|_| {
            "no store signing key in your Keychain; run `cargo run -p xtask -- ext-keygen` first"
                .to_string()
        })?,
    };
    // `ext-keygen` stores PKCS#8 v2; a test key from `openssl genpkey -algorithm ed25519` is v1.
    let pair = Ed25519KeyPair::from_pkcs8(&pkcs8)
        .or_else(|_| Ed25519KeyPair::from_pkcs8_maybe_unchecked(&pkcs8))
        .map_err(|_| "the signing key can't be read")?;
    if let Some(trusted) = args.trusted
        && trusted != pair.public_key().as_ref()
    {
        return Err(format!(
            "the signing key ({}) isn't the one Samo trusts ({}, STORE_PUBLIC_KEY in {TRUSTED_KEY_FILE}); \
             Samo would refuse this catalog",
            hex(pair.public_key().as_ref()),
            hex(&trusted)
        ));
    }
    let sig = hex(pair.sign(&bytes).as_ref());
    copy("catalog.json", &bytes)?;
    copy("catalog.json.sig", sig.as_bytes())?;

    // Check what was written, as Samo will.
    let mut public = [0u8; 32];
    public.copy_from_slice(pair.public_key().as_ref());
    verify_written(out, &public)?;
    println!("Signed with public key {}", hex(&public));
    println!("Wrote {} and verified it. Upload with:", out.display());
    println!(
        "  gh release upload store {}/* -R KJ-AIML/samo-extensions --clobber",
        out.display()
    );
    Ok(())
}

/// `STORE_PUBLIC_KEY` from Samo's source: the first 64 hex characters in quotes after its name.
pub fn key_in_source(text: &str) -> Option<[u8; 32]> {
    let after = text.split_once("pub const STORE_PUBLIC_KEY")?.1;
    let quoted = after.split('"').nth(1)?;
    let bytes = samo_domain::store::unhex(quoted.trim())?;
    let mut key = [0u8; 32];
    (bytes.len() == 32).then(|| key.copy_from_slice(&bytes))?;
    Some(key)
}

/// The key the Samo in `root` trusts.
pub fn trusted_key(root: &Path) -> Result<[u8; 32], String> {
    let text = fs::read_to_string(root.join(TRUSTED_KEY_FILE)).map_err(io(TRUSTED_KEY_FILE))?;
    match key_in_source(&text) {
        Some(key) if key != [0u8; 32] => Ok(key),
        Some(_) => Err(format!(
            "STORE_PUBLIC_KEY in {TRUSTED_KEY_FILE} is still all zeros; run ext-keygen and paste its public key"
        )),
        None => Err(format!("no STORE_PUBLIC_KEY in {TRUSTED_KEY_FILE}")),
    }
}

/// Reads `dist/` back and checks it all, exactly as Samo would.
pub fn verify_written(out: &Path, public: &[u8; 32]) -> Result<Catalog, String> {
    let bytes = fs::read(out.join("catalog.json")).map_err(io("catalog.json"))?;
    let sig = fs::read_to_string(out.join("catalog.json.sig")).map_err(io("catalog.json.sig"))?;
    let catalog = verify_catalog(&bytes, &sig, public).map_err(|e| e.to_string())?;
    let read = |f: &FileRef| fs::read(out.join(&f.file)).map_err(io("a file in dist"));
    for l in &catalog.extensions {
        verify_file(&l.id, &read(&l.package)?, &l.package, PACKAGE_CAP)
            .map_err(|e| e.to_string())?;
        if let Some(icon) = &l.icon {
            verify_image(&l.id, &read(icon)?, icon, ICON_CAP, ICON_MAX)
                .map_err(|e| e.to_string())?;
        }
        for s in &l.screenshots {
            verify_image(&l.id, &read(&s.file)?, &s.file, SHOT_CAP, SHOT_MAX)
                .map_err(|e| e.to_string())?;
        }
    }
    if let Some(samo) = &catalog.samo {
        verify_file("Samo", &read(&samo.package)?, &samo.package, SAMO_CAP)
            .map_err(|e| e.to_string())?;
    }
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use samo_ext_protocol::Change;

    fn manifest() -> Manifest {
        Manifest {
            id: "hello".into(),
            name: "Hello".into(),
            version: "0.2.0".into(),
            protocol: 1,
            min_samo: "0.2.0".into(),
            helper: "bin/samo-ext-hello".into(),
            summary: "Every widget".into(),
            changes: vec![Change {
                what: "Nothing".into(),
                place: "nowhere".into(),
                undo: "nothing".into(),
            }],
            settings: Vec::new(),
        }
    }

    fn pkg() -> FileRef {
        FileRef {
            file: "hello-0.2.0-macos-arm64.tar.gz".into(),
            size: Some(3),
            sha256: sha256_hex(b"abc"),
            width: None,
            height: None,
        }
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0]);
        b
    }

    #[test]
    fn publish_merges_manifest_and_listing_and_refuses_conflicts() -> Result<(), String> {
        let listing: ListingFile = toml::from_str(
            "author = \"KJ\"\ndescription = \"Hi\"\ncategories = [\"example\"]\nwhats_new = \"First\"\n",
        )
        .map_err(|e| e.to_string())?;
        let l = merge(&manifest(), &listing, pkg())?;
        assert_eq!(
            (l.id.as_str(), l.version.as_str(), l.author.as_str()),
            ("hello", "0.2.0", "KJ")
        );
        assert_eq!(l.changes, manifest().changes);

        let conflicting: ListingFile =
            toml::from_str("version = \"9.9.9\"\n").map_err(|e| e.to_string())?;
        assert!(merge(&manifest(), &conflicting, pkg()).is_err());
        let agreeing: ListingFile =
            toml::from_str("name = \"Hello\"\n").map_err(|e| e.to_string())?;
        assert!(merge(&manifest(), &agreeing, pkg()).is_ok());
        Ok(())
    }

    #[test]
    fn publish_rejects_oversized_or_wrong_size_images() -> Result<(), String> {
        let icon = image_ref("hello", "icon", &png(256, 256), ICON_CAP, ICON_MAX)?;
        assert!(icon.file.starts_with("hello-icon-") && icon.file.ends_with(".png"));
        assert_eq!((icon.width, icon.height), (Some(256), Some(256)));
        assert!(image_ref("hello", "icon", &png(512, 512), ICON_CAP, ICON_MAX).is_err());
        assert!(image_ref("hello", "shot1", b"not an image", SHOT_CAP, SHOT_MAX).is_err());
        let big = vec![0u8; usize::try_from(ICON_CAP).unwrap_or(0) + 1];
        assert!(image_ref("hello", "icon", &big, ICON_CAP, ICON_MAX).is_err());
        Ok(())
    }

    #[test]
    fn publish_output_verifies() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("samo-publish-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = dir.join("store/extensions/hello");
        fs::create_dir_all(store.join("screenshots")).map_err(|e| e.to_string())?;
        fs::write(
            store.join("listing.toml"),
            "author = \"KJ\"\n[[screenshots]]\nfile = \"a.png\"\ncaption = \"All of it\"\n",
        )
        .map_err(|e| e.to_string())?;
        fs::write(store.join("icon.png"), png(256, 256)).map_err(|e| e.to_string())?;
        fs::write(store.join("screenshots/a.png"), png(1600, 1000)).map_err(|e| e.to_string())?;

        // A package with just a manifest is enough for publishing.
        let stage = dir.join("stage/hello");
        fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
        fs::write(
            stage.join("extension.toml"),
            "id = \"hello\"\nname = \"Hello\"\nversion = \"0.2.0\"\nprotocol = 1\nmin_samo = \"0.2.0\"\n\
             helper = \"bin/samo-ext-hello\"\nsummary = \"Every widget\"\n\n[[changes]]\nwhat = \"Nothing\"\n\
             where = \"nowhere\"\nundo = \"nothing\"\n",
        )
        .map_err(|e| e.to_string())?;
        let packages = dir.join("packages");
        fs::create_dir_all(&packages).map_err(|e| e.to_string())?;
        let ok = Command::new("tar")
            .env("COPYFILE_DISABLE", "1")
            .arg("-C")
            .arg(dir.join("stage"))
            .arg("-czf")
            .arg(packages.join("hello-0.2.0-macos-arm64.tar.gz"))
            .arg("hello")
            .status()
            .map_err(|e| e.to_string())?;
        assert!(ok.success());

        let key = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).map_err(|_| "key")?;
        fs::write(dir.join("test-key.pk8"), key.as_ref()).map_err(|e| e.to_string())?;
        let out = dir.join("dist");
        publish(&PublishArgs {
            trusted: None,
            store: dir.join("store"),
            packages,
            samo: None,
            out: out.clone(),
            test_key: Some(dir.join("test-key.pk8")),
            published_at: "2026-11-02T10:00:00Z".into(),
        })?;
        let pair = Ed25519KeyPair::from_pkcs8(key.as_ref()).map_err(|_| "key")?;
        let mut public = [0u8; 32];
        public.copy_from_slice(pair.public_key().as_ref());
        let catalog = verify_written(&out, &public)?;
        assert_eq!(catalog.extensions[0].screenshots[0].caption, "All of it");
        assert!(catalog.extensions[0].icon.is_some());

        // One byte changed in the package, and it no longer verifies.
        let p = out.join("hello-0.2.0-macos-arm64.tar.gz");
        let mut bytes = fs::read(&p).map_err(|e| e.to_string())?;
        if let Some(b) = bytes.last_mut() {
            *b ^= 1;
        }
        fs::write(&p, bytes).map_err(|e| e.to_string())?;
        assert!(verify_written(&out, &public).is_err());
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn publish_refuses_a_key_samo_doesnt_trust() -> Result<(), String> {
        let src = "/// doc\npub const STORE_PUBLIC_KEY: &str =\n    \"54fdd32a521a6b5c0d68e14becad464ee9b64bf82e06d5b2c8e6ba2d6beb4b27\";\n";
        let key = key_in_source(src).ok_or("no key")?;
        assert_eq!(
            hex(&key),
            "54fdd32a521a6b5c0d68e14becad464ee9b64bf82e06d5b2c8e6ba2d6beb4b27"
        );
        assert_eq!(
            key_in_source("pub const STORE_PUBLIC_KEY: &str = \"abc\";"),
            None
        );
        // This repository's Samo trusts a real key.
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("no root")?;
        assert!(trusted_key(root).is_ok());

        let dir = std::env::temp_dir().join(format!("samo-publish-trust-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("store/extensions")).map_err(|e| e.to_string())?;
        fs::create_dir_all(dir.join("packages")).map_err(|e| e.to_string())?;
        let pk8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).map_err(|_| "key")?;
        fs::write(dir.join("k.pk8"), pk8.as_ref()).map_err(|e| e.to_string())?;
        let result = publish(&PublishArgs {
            trusted: Some(key),
            store: dir.join("store"),
            packages: dir.join("packages"),
            samo: None,
            out: dir.join("dist"),
            test_key: Some(dir.join("k.pk8")),
            published_at: "2026-11-02T10:00:00Z".into(),
        });
        assert!(result.is_err_and(|e| e.contains("isn't the one Samo trusts")));
        assert!(
            !dir.join("dist/catalog.json.sig").exists(),
            "nothing is signed"
        );
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
