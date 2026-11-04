//! An old Samo reads the new catalog (#10, ADR-006): Samo 0.4.1's own reader, vendored verbatim, is given a signed
//! catalog with Windows packages and sees the same listings and Mac packages it always did.

#[allow(dead_code)]
#[path = "fixtures/store_v041.rs"]
mod v041;

use ring::signature::{Ed25519KeyPair, KeyPair};
use samo_domain::store::{
    Capabilities, Catalog, FileRef, Listing, SamoRelease, WINDOWS, hex, verify_catalog,
};
use samo_ext_protocol::Change;
use std::collections::BTreeMap;

fn file(name: &str) -> FileRef {
    FileRef {
        file: name.into(),
        size: Some(3),
        sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
        width: None,
        height: None,
    }
}

fn listing(id: &str) -> Listing {
    Listing {
        id: id.into(),
        name: "Claude Sessions".into(),
        author: "KJ".into(),
        summary: "Sessions".into(),
        description: String::new(),
        categories: Vec::new(),
        homepage: String::new(),
        version: "0.2.1".into(),
        protocol: 1,
        min_samo: "0.2.0".into(),
        whats_new: String::new(),
        package: file(&format!("{id}-0.2.1-macos-arm64.tar.gz")),
        platforms: BTreeMap::from([(
            WINDOWS.to_string(),
            file(&format!("{id}-0.2.1-windows-x86_64.tar.gz")),
        )]),
        icon: None,
        screenshots: Vec::new(),
        changes: vec![Change {
            what: "Hooks".into(),
            place: "~/.claude/settings.json".into(),
            undo: "Disconnect".into(),
        }],
        windows: None,
        watches: None,
        permissions: Vec::new(),
        min_macos: None,
        limits: None,
        overrides: BTreeMap::new(),
    }
}

/// A listing with a Windows build that declares other capabilities, as Clips does.
fn clips_like() -> Listing {
    let mut l = listing("hello");
    l.overrides.insert(
        WINDOWS.to_string(),
        Capabilities {
            summary: Some("Win+Alt+V finds it".into()),
            changes: vec![Change {
                what: "Presses Ctrl+V for you".into(),
                place: "The app you were in".into(),
                undo: "Nothing to undo".into(),
            }],
            windows: Some("The list at its shortcut".into()),
            watches: Some("Windows tells it of each new copy".into()),
            ..Capabilities::default()
        },
    );
    l
}

#[test]
fn old_samo_reads_the_new_catalog() -> Result<(), String> {
    let catalog = Catalog {
        schema: 1,
        published_at: "2026-11-05T10:00:00Z".into(),
        samo: Some(SamoRelease {
            version: "0.5.0".into(),
            whats_new: String::new(),
            min_macos: "14.0".into(),
            package: file("Samo-0.5.0-macos-arm64.zip"),
            platforms: BTreeMap::from([(
                WINDOWS.to_string(),
                file("Samo-0.5.0-windows-x86_64.zip"),
            )]),
        }),
        extensions: vec![listing("claude-sessions"), clips_like()],
    };
    let bytes = serde_json::to_vec(&catalog).map_err(|e| e.to_string())?;
    let key = Ed25519KeyPair::from_seed_unchecked(&[7u8; 32]).map_err(|e| e.to_string())?;
    let public: [u8; 32] = key
        .public_key()
        .as_ref()
        .try_into()
        .map_err(|_| "a 32-byte key")?;
    let sig = hex(key.sign(&bytes).as_ref());

    // Today's reader sees the Windows packages.
    let new = verify_catalog(&bytes, &sig, &public).map_err(|e| e.to_string())?;
    assert_eq!(new, catalog);

    // 0.4.1's reader accepts it and sees what it always did: the same listings and Mac packages.
    let old = v041::verify_catalog(&bytes, &sig, &public).map_err(|e| e.to_string())?;
    assert_eq!(old.extensions.len(), 2);
    for (o, n) in old.extensions.iter().zip(&new.extensions) {
        assert_eq!(o.id, n.id);
        assert_eq!(o.version, n.version);
        assert_eq!(o.package.file, n.package.file);
        assert_eq!(o.package.sha256, n.package.sha256);
    }
    // The Windows build's own capabilities don't change what 0.4.1 sees.
    let mac = old.extensions.get(1).ok_or("0.4.1 lost a listing")?;
    assert_eq!(mac.summary, "Sessions");
    assert_eq!(mac.changes.len(), 1);
    assert_eq!(mac.changes.first().map(|c| c.what.as_str()), Some("Hooks"));
    let samo = old.samo.ok_or("0.4.1 lost the Samo release")?;
    assert_eq!(samo.package.file, "Samo-0.5.0-macos-arm64.zip");
    Ok(())
}
