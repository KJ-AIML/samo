//! Signing helpers that ask macOS for a permission (ADR-004, Shots' Screen Recording).
//!
//! macOS keeps a permission for the code's designated requirement. An ad-hoc signature's requirement is the
//! binary's own hash, so every new build, and every store update, is a new app that has to be allowed again.
//! Signed with one identity, the requirement becomes "this identifier, signed by this certificate", and a grant
//! survives updates. The identity is self-signed and lives in KJ's login Keychain, like the store's key; it is made
//! once, with `ext-codesign-setup`, and must never change.

use ring::rand::{SecureRandom, SystemRandom};
use samo_domain::store::hex;
use samo_ext_protocol::Manifest;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The signing identity's name in the Keychain.
pub const IDENTITY: &str = "Talvyn Samo Extensions";
/// LibreSSL, always on macOS: its PKCS#12 files are ones `security import` reads (newer OpenSSLs' aren't, by
/// default).
const OPENSSL: &str = "/usr/bin/openssl";

fn io<E: std::fmt::Display>(what: &str) -> impl Fn(E) -> String + '_ {
    move |e| format!("{what}: {e}")
}

/// A helper that asks macOS for a permission must keep one identity across updates.
pub fn needs_signing(manifest: &Manifest) -> bool {
    !manifest.permissions.is_empty()
}

/// The identifier macOS knows a helper by: the bundle id its Info.plist carries (`dev.talvyn.samo.shots`).
pub fn identifier(manifest: &Manifest) -> String {
    format!("dev.talvyn.samo.{}", manifest.id)
}

pub fn codesign_args(
    helper: &Path,
    identifier: &str,
    identity: &str,
    keychain: Option<&Path>,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        "--force".into(),
        "--sign".into(),
        identity.into(),
        "--identifier".into(),
        identifier.into(),
    ];
    if let Some(keychain) = keychain {
        args.push("--keychain".into());
        args.push(keychain.into());
    }
    args.push(helper.into());
    args
}

/// From `codesign -d -r-`'s output, the certificate's hash, if the requirement is the stable kind: this
/// identifier, signed by one certificate. An ad-hoc requirement (`cdhash …`) or another identifier is refused.
pub fn stable_requirement(output: &str, identifier: &str) -> Result<String, String> {
    let line = output
        .lines()
        .find_map(|l| {
            l.trim()
                .trim_start_matches("# ")
                .strip_prefix("designated => ")
        })
        .ok_or("codesign printed no designated requirement")?;
    if line.contains("cdhash") {
        return Err(format!(
            "the helper's requirement is its own hash ({line}), so every update would be a new app to macOS"
        ));
    }
    if !line.contains(&format!("identifier \"{identifier}\"")) {
        return Err(format!(
            "the helper's requirement doesn't name {identifier}: {line}"
        ));
    }
    let leaf = line
        .split("certificate leaf = H\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .filter(|h| h.len() == 40 && h.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| format!("the helper's requirement names no certificate: {line}"))?;
    Ok(leaf.to_string())
}

/// Signs `helper` with the identity (or `SAMO_CODESIGN_IDENTITY`, searching `SAMO_CODESIGN_KEYCHAIN` if set) and
/// checks the result is the stable kind. Returns the certificate's hash.
pub fn sign(helper: &Path, identifier: &str) -> Result<String, String> {
    let identity = std::env::var("SAMO_CODESIGN_IDENTITY").unwrap_or_else(|_| IDENTITY.into());
    let keychain = std::env::var_os("SAMO_CODESIGN_KEYCHAIN").map(PathBuf::from);
    let out = Command::new("codesign")
        .args(codesign_args(
            helper,
            identifier,
            &identity,
            keychain.as_deref(),
        ))
        .output()
        .map_err(io("codesign"))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "couldn't sign {} as {identifier} with \"{identity}\": {}\n\
             A helper that asks macOS for a permission is signed with one identity, so updates keep the grant.\n\
             Make it once with: cargo run -p xtask -- ext-codesign-setup",
            helper.display(),
            why.trim()
        ));
    }
    let shown = Command::new("codesign")
        .args(["-d", "-r-"])
        .arg(helper)
        .output()
        .map_err(io("codesign"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&shown.stdout),
        String::from_utf8_lossy(&shown.stderr)
    );
    stable_requirement(&text, identifier)
}

/// Signs a staged helper if its manifest asks for a permission, and says so.
pub fn sign_if_needed(manifest: &Manifest, helper: &Path) -> Result<(), String> {
    if !needs_signing(manifest) {
        return Ok(());
    }
    let identifier = identifier(manifest);
    let leaf = sign(helper, &identifier)?;
    println!(
        "Signed {} as {identifier} (certificate {leaf}): updates keep its permissions.",
        manifest.helper
    );
    Ok(())
}

/// Whether a code-signing identity called `name` is in `keychain` (or the search list).
fn identity_exists(name: &str, keychain: Option<&Path>) -> Result<bool, String> {
    let mut command = Command::new("security");
    command.args(["find-identity", "-p", "codesigning"]);
    if let Some(keychain) = keychain {
        command.arg(keychain);
    }
    let out = command.output().map_err(io("security find-identity"))?;
    Ok(String::from_utf8_lossy(&out.stdout).contains(&format!("\"{name}\"")))
}

/// Makes a self-signed code-signing identity called `name` and imports it into `keychain` (the default one if
/// `None`), usable by `codesign`. The private key exists on disk only in a private scratch folder, removed before
/// this returns. Returns the certificate's SHA-1.
pub fn make_identity(name: &str, keychain: Option<&Path>) -> Result<String, String> {
    if identity_exists(name, keychain)? {
        return Err(format!(
            "a signing identity \"{name}\" already exists; Samo never replaces it (a new one would make every \
             helper ask macOS for its permissions again)"
        ));
    }
    let mut noise = [0u8; 16];
    SystemRandom::new()
        .fill(&mut noise)
        .map_err(|_| "couldn't make a password")?;
    let scratch = std::env::temp_dir().join(format!("samo-codesign-{}", hex(&noise[..6])));
    let result = make_identity_in(name, keychain, &scratch, &hex(&noise));
    let _ = fs::remove_dir_all(&scratch);
    result
}

fn make_identity_in(
    name: &str,
    keychain: Option<&Path>,
    scratch: &Path,
    password: &str,
) -> Result<String, String> {
    fs::create_dir_all(scratch).map_err(io("scratch folder"))?;
    run("chmod", &["700".into(), scratch.into()])?;
    let config = scratch.join("cert.cnf");
    fs::write(
        &config,
        format!(
            "[req]\ndistinguished_name = dn\nx509_extensions = ext\nprompt = no\n[dn]\nCN = {name}\n\
             [ext]\nbasicConstraints = critical,CA:false\nkeyUsage = critical,digitalSignature\n\
             extendedKeyUsage = critical,codeSigning\n"
        ),
    )
    .map_err(io("certificate config"))?;
    let (key, cert, p12) = (
        scratch.join("key.pem"),
        scratch.join("cert.pem"),
        scratch.join("identity.p12"),
    );
    run(
        OPENSSL,
        &[
            "req".into(),
            "-x509".into(),
            "-newkey".into(),
            "rsa:2048".into(),
            "-nodes".into(),
            "-days".into(),
            "7300".into(),
            "-keyout".into(),
            key.clone().into(),
            "-out".into(),
            cert.clone().into(),
            "-config".into(),
            config.into(),
        ],
    )?;
    run(
        OPENSSL,
        &[
            "pkcs12".into(),
            "-export".into(),
            "-inkey".into(),
            key.into(),
            "-in".into(),
            cert.clone().into(),
            "-out".into(),
            p12.clone().into(),
            "-passout".into(),
            format!("pass:{password}").into(),
        ],
    )?;
    let mut import: Vec<OsString> = vec!["import".into(), p12.into()];
    if let Some(keychain) = keychain {
        import.push("-k".into());
        import.push(keychain.into());
    }
    import.extend([
        "-P".into(),
        password.into(),
        "-T".into(),
        "/usr/bin/codesign".into(),
    ]);
    run("security", &import)?;
    let fingerprint = Command::new(OPENSSL)
        .args(["x509", "-noout", "-fingerprint", "-sha1", "-in"])
        .arg(&cert)
        .output()
        .map_err(io("openssl"))?;
    let text = String::from_utf8_lossy(&fingerprint.stdout);
    let sha1: String = text
        .split('=')
        .nth(1)
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_hexdigit)
        .collect::<String>()
        .to_lowercase();
    if sha1.len() != 40 {
        return Err(format!(
            "couldn't read the certificate's fingerprint: {text}"
        ));
    }
    Ok(sha1)
}

fn run(program: &str, args: &[OsString]) -> Result<(), String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(io(program))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// `cargo run -p xtask -- ext-codesign-setup`: the signing identity, made once in the login Keychain.
pub fn setup() -> Result<(), String> {
    let sha1 = make_identity(IDENTITY, None)?;
    println!("\"{IDENTITY}\" is in your Keychain (certificate {sha1}).");
    println!("ext-package now signs every helper that asks macOS for a permission with it.");
    println!(
        "Back it up: in Keychain Access, export \"{IDENTITY}\" (certificate and key) as a .p12 somewhere safe. \
         If it is lost, the next update of such a helper asks macOS for its permission once more."
    );
    println!(
        "The first time codesign uses it, macOS may ask to let codesign use the key: choose Always Allow."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(permissions: &str) -> Result<Manifest, String> {
        Manifest::parse(&format!(
            "id = \"shots\"\nname = \"Shots\"\nversion = \"0.2.0\"\nprotocol = 1\nmin_samo = \"0.3.0\"\n\
             helper = \"bin/samo-shots\"\nsummary = \"s\"\n{permissions}\n[[changes]]\nwhat = \"w\"\n\
             where = \"x\"\nundo = \"y\"\n"
        ))
        .map_err(|e| e.to_string())
    }

    #[test]
    fn helpers_that_ask_for_a_permission_are_signed() -> Result<(), String> {
        let shots = manifest("windows = \"w\"\npermissions = [\"screen-recording\"]\n")?;
        assert!(needs_signing(&shots));
        assert_eq!(identifier(&shots), "dev.talvyn.samo.shots");
        assert!(!needs_signing(&manifest("")?));
        Ok(())
    }

    #[test]
    fn codesign_command() {
        let args = codesign_args(
            Path::new("/s/bin/samo-shots"),
            "dev.talvyn.samo.shots",
            IDENTITY,
            None,
        );
        assert_eq!(
            args,
            [
                "--force",
                "--sign",
                IDENTITY,
                "--identifier",
                "dev.talvyn.samo.shots",
                "/s/bin/samo-shots"
            ]
            .map(OsString::from)
        );
        let in_keychain = codesign_args(
            Path::new("/h"),
            "x",
            "y",
            Some(Path::new("/tmp/k.keychain")),
        );
        assert_eq!(in_keychain[5], "--keychain");
    }

    #[test]
    fn requirement_must_name_the_certificate() {
        let leaf = "21f7e44cd7f596d005f04f29282f6afd03a21dbe";
        let stable = format!(
            "Executable=/x\ndesignated => identifier \"dev.talvyn.samo.shots\" and certificate leaf = H\"{leaf}\"\n"
        );
        assert_eq!(
            stable_requirement(&stable, "dev.talvyn.samo.shots"),
            Ok(leaf.to_string())
        );
        // Ad hoc: the binary's own hash, which changes with every build.
        let adhoc = "# designated => cdhash H\"5ac09a5170b7ecabd4ffa1353b63b94f964002aa\"";
        assert!(stable_requirement(adhoc, "dev.talvyn.samo.shots").is_err());
        // Another identifier.
        assert!(stable_requirement(&stable, "dev.talvyn.samo.other").is_err());
        assert!(stable_requirement("nothing", "x").is_err());
    }

    /// The user's keychain search list, as `security list-keychains` prints it.
    fn search_list() -> Result<Vec<String>, String> {
        let out = Command::new("security")
            .args(["list-keychains", "-d", "user"])
            .output()
            .map_err(io("security list-keychains"))?;
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim().trim_matches('"').to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

    fn set_search_list(paths: &[String]) -> Result<(), String> {
        let mut args: Vec<OsString> = vec![
            "list-keychains".into(),
            "-d".into(),
            "user".into(),
            "-s".into(),
        ];
        args.extend(paths.iter().map(OsString::from));
        run("security", &args)
    }

    /// The whole thing, in a throwaway keychain: two different builds signed with one identity share one
    /// requirement, which is what macOS checks a stored permission against.
    ///
    /// Older macOS (GitHub's runners) finds a signing identity for `codesign --keychain` only in a keychain on the
    /// search list, so the throwaway keychain joins the list while the test runs, and the list is put back after.
    #[test]
    fn two_builds_share_one_requirement() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("samo-codesign-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(io("scratch"))?;
        let keychain = dir.join("test.keychain");
        let before = search_list()?;
        let result = (|| {
            run(
                "security",
                &[
                    "create-keychain".into(),
                    "-p".into(),
                    "test".into(),
                    keychain.clone().into(),
                ],
            )?;
            run(
                "security",
                &[
                    "unlock-keychain".into(),
                    "-p".into(),
                    "test".into(),
                    keychain.clone().into(),
                ],
            )?;
            let mut with_test = vec![keychain.to_string_lossy().into_owned()];
            with_test.extend(before.iter().cloned());
            set_search_list(&with_test)?;
            let name = "Samo Codesign Test";
            let sha1 = make_identity(name, Some(&keychain))?;
            // Made once: a second is refused.
            assert!(make_identity(name, Some(&keychain)).is_err());
            run(
                "security",
                &[
                    "set-key-partition-list".into(),
                    "-S".into(),
                    "apple-tool:,apple:".into(),
                    "-s".into(),
                    "-k".into(),
                    "test".into(),
                    keychain.clone().into(),
                ],
            )?;
            // Two different programs stand in for two builds.
            let (a, b) = (dir.join("a"), dir.join("b"));
            fs::copy("/usr/bin/true", &a).map_err(io("copy"))?;
            fs::copy("/usr/bin/false", &b).map_err(io("copy"))?;
            let mut requirements = Vec::new();
            for helper in [&a, &b] {
                let out = Command::new("codesign")
                    .args(codesign_args(
                        helper,
                        "dev.talvyn.samo.test",
                        &sha1,
                        Some(&keychain),
                    ))
                    .output()
                    .map_err(io("codesign"))?;
                if !out.status.success() {
                    return Err(String::from_utf8_lossy(&out.stderr).into_owned());
                }
                let shown = Command::new("codesign")
                    .args(["-d", "-r-"])
                    .arg(helper)
                    .output()
                    .map_err(io("codesign"))?;
                let text = String::from_utf8_lossy(&shown.stdout).into_owned();
                assert_eq!(stable_requirement(&text, "dev.talvyn.samo.test")?, sha1);
                requirements.push(text);
            }
            assert_eq!(requirements[0], requirements[1]);
            // The second build satisfies the first's requirement.
            let requirement = requirements[0]
                .lines()
                .find_map(|l| l.strip_prefix("designated => "))
                .unwrap_or_default()
                .to_string();
            let ok = Command::new("codesign")
                .arg("--verify")
                .arg(format!("-R={requirement}"))
                .arg(&b)
                .status()
                .map_err(io("codesign"))?;
            assert!(ok.success(), "a later build keeps the grant");
            Ok(())
        })();
        // The search list as it was, whatever happened.
        let restored = set_search_list(&before);
        let _ = Command::new("security")
            .arg("delete-keychain")
            .arg(&keychain)
            .status();
        let _ = fs::remove_dir_all(&dir);
        result.and(restored)
    }
}
