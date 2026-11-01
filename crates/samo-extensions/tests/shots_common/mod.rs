//! What the Shots demos share: building the real `samo-shots`, staging it as `ext-package` would, and reading
//! its page as Samo draws it.

use samo_ext_protocol::{Manifest, Widget};
use samo_extensions::{ExtRoot, ExtSlot, ExtSnapshot, Host};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub type Fallible<T> = Result<T, String>;

pub fn io<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

pub const SLOT: ExtSlot = ExtSlot(0);

pub fn shots_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions/shots")
}

/// Builds `samo-shots` (debug) and returns where it is.
pub fn build_shots() -> Fallible<PathBuf> {
    let dir = shots_dir();
    let ok = Command::new("swift")
        .args(["build", "--package-path"])
        .arg(&dir)
        .args(["--product", "samo-shots"])
        .status()
        .map_err(|e| format!("Shots needs Swift ({e})"))?;
    if !ok.success() {
        return Err("swift build failed".into());
    }
    let out = Command::new("swift")
        .args(["build", "--package-path"])
        .arg(&dir)
        .arg("--show-bin-path")
        .output()
        .map_err(io)?;
    Ok(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()).join("samo-shots"))
}

/// `<stage>/shots/`: the manifest (changed by `edit`), the helper and the fonts, as `ext-package` lays them out.
pub fn stage(stage: &Path, binary: &Path, edit: impl Fn(String) -> String) -> Fallible<Manifest> {
    let ext = stage.join("shots");
    let _ = fs::remove_dir_all(&ext);
    fs::create_dir_all(ext.join("bin")).map_err(io)?;
    fs::create_dir_all(ext.join("share/fonts")).map_err(io)?;
    let text = edit(fs::read_to_string(shots_dir().join("extension.toml")).map_err(io)?);
    fs::write(ext.join("extension.toml"), &text).map_err(io)?;
    let helper = ext.join("bin/samo-shots");
    fs::write(&helper, fs::read(binary).map_err(io)?).map_err(io)?;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).map_err(io)?;
    for font in fs::read_dir(shots_dir().join("share/fonts")).map_err(io)? {
        let font = font.map_err(io)?;
        fs::write(
            ext.join("share/fonts").join(font.file_name()),
            fs::read(font.path()).map_err(io)?,
        )
        .map_err(io)?;
    }
    Manifest::parse(&text).map_err(io)
}

pub fn wait(
    host: &Host,
    what: &str,
    secs: u64,
    test: impl Fn(&ExtSnapshot) -> bool,
) -> Fallible<Arc<ExtSnapshot>> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let snap = host.snapshot();
        if test(&snap) {
            return Ok(snap);
        }
        if Instant::now() > deadline {
            return Err(format!(
                "timed out waiting for {what}; {:?}",
                snap.get(SLOT).map(|e| (&e.status, &e.note))
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

pub fn page(snap: &ExtSnapshot) -> Vec<Widget> {
    snap.get(SLOT)
        .map(|e| (*e.view).clone())
        .unwrap_or_default()
}

pub fn has_section(snap: &ExtSnapshot, title: &str) -> bool {
    page(snap)
        .iter()
        .any(|w| matches!(w, Widget::Section { title: t, .. } if t == title))
}

pub fn chosen(snap: &ExtSnapshot, key: &str) -> Option<String> {
    page(snap).into_iter().find_map(|w| match w {
        Widget::Choice {
            key: k, selected, ..
        } if k == key => Some(selected),
        _ => None,
    })
}

/// Processes running the installed helper (the first process and its worker).
pub fn shots_processes(root: &ExtRoot) -> usize {
    let path = root.ext_dir("shots").to_string_lossy().into_owned();
    Command::new("pgrep")
        .args(["-f", &path])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().count())
        .unwrap_or(0)
}
