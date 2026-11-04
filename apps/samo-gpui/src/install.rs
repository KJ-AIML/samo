//! Samo for this user on Windows (#10, ADR-006). Samo.exe runs from wherever its zip was unpacked; Settings offers
//! to install it in `%LOCALAPPDATA%\Programs\Samo` (no admin), with a Start-menu shortcut, and then starts the
//! installed Samo. **Start at sign-in** is a value under `HKCU\…\CurrentVersion\Run`.
//!
//! The paths are worked out as strings, so the rules are tested on every OS (ADR-006 R25).

#[cfg(any(windows, test))]
use samo_primitives::ProcessIdentity;
#[cfg(any(windows, test))]
use std::ffi::OsString;
#[cfg(any(windows, test))]
use std::path::{Path, PathBuf};

/// What a just-installed Samo starts with: `--after <pid> <start time>`, the Samo to wait for, so the two never run
/// at once (one Samo at a time).
#[cfg(any(windows, test))]
pub const AFTER: &str = "--after";

/// Where Samo is installed for this user: `%LOCALAPPDATA%\Programs\Samo`, Windows' place for a user's programs.
#[cfg(any(windows, test))]
pub fn install_dir_from(get: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let local = get("LOCALAPPDATA").filter(|v| !v.is_empty())?;
    Some(PathBuf::from(local).join("Programs").join("Samo"))
}

/// The Start-menu shortcut: `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Samo.lnk`.
#[cfg(any(windows, test))]
pub fn shortcut_from(get: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let roaming = get("APPDATA").filter(|v| !v.is_empty())?;
    Some(
        PathBuf::from(roaming)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Samo.lnk"),
    )
}

/// A path's folder and file name, split at its last `\` or `/`.
#[cfg(any(windows, test))]
fn split_last(path: &Path) -> (String, String) {
    let text = path.to_string_lossy();
    match text.rfind(['\\', '/']) {
        Some(at) => (text[..at].to_string(), text[at + 1..].to_string()),
        None => (String::new(), text.into_owned()),
    }
}

/// Whether `exe` is Samo installed in `dir`: `Samo.exe` there. Windows paths are the same in any case.
#[cfg(any(windows, test))]
pub fn is_installed_at(exe: &Path, dir: &Path) -> bool {
    let (folder, name) = split_last(exe);
    let dir = dir.to_string_lossy();
    let dir = dir.trim_end_matches(['\\', '/']);
    name.eq_ignore_ascii_case("Samo.exe")
        && folder
            .replace('/', "\\")
            .eq_ignore_ascii_case(&dir.replace('/', "\\"))
}

/// The PowerShell that makes the shortcut at `lnk` to `target`, with every path in single quotes (a `'` in one is
/// doubled). It is given on PowerShell's standard input, so nothing in it is a command-line argument.
#[cfg(any(windows, test))]
pub fn shortcut_script(lnk: &Path, target: &Path) -> String {
    let quote = |p: &str| format!("'{}'", p.replace('\'', "''"));
    let (folder, _) = split_last(target);
    format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut({})\n$s.TargetPath = {}\n\
         $s.WorkingDirectory = {}\n$s.Save()\n",
        quote(&lnk.to_string_lossy()),
        quote(&target.to_string_lossy()),
        quote(&folder),
    )
}

/// The `Run` value that starts `exe` at sign-in: its path, quoted, as Windows runs it.
#[cfg(any(windows, test))]
pub fn run_value(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

/// The Samo to wait for, if Samo was started with `--after <pid> <start time>`.
#[cfg(any(windows, test))]
pub fn after_from(args: &[String]) -> Option<ProcessIdentity> {
    if args.get(1).map(String::as_str) != Some(AFTER) {
        return None;
    }
    Some(ProcessIdentity {
        pid: args.get(2)?.parse().ok()?,
        start_time_secs: args.get(3)?.parse().ok()?,
    })
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

    pub fn install_dir() -> Option<PathBuf> {
        install_dir_from(|name| std::env::var_os(name))
    }

    /// Whether this Samo runs from its installed place.
    pub fn is_installed() -> bool {
        match (std::env::current_exe(), install_dir()) {
            (Ok(exe), Some(dir)) => is_installed_at(&exe, &dir),
            _ => false,
        }
    }

    /// Copies this Samo (and its `release.toml`) to its installed place and adds the Start-menu shortcut. Returns
    /// the installed `Samo.exe`, and what didn't work beyond it, if anything.
    pub fn install() -> Result<(PathBuf, Option<String>), String> {
        let exe = std::env::current_exe()
            .map_err(|e| format!("Samo can't tell where it runs from: {e}"))?;
        let dir = install_dir().ok_or("Windows didn't say where this user's programs go")?;
        fs::create_dir_all(&dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
        let to = dir.join("Samo.exe");
        fs::copy(&exe, &to).map_err(|e| format!("couldn't copy Samo to {}: {e}", dir.display()))?;
        if let Some(release) = exe
            .parent()
            .map(|p| p.join("release.toml"))
            .filter(|p| p.is_file())
        {
            let _ = fs::copy(release, dir.join("release.toml"));
        }
        let shortcut = shortcut_from(|name| std::env::var_os(name))
            .ok_or_else(|| "no Start menu folder".to_string())
            .and_then(|lnk| make_shortcut(&lnk, &to))
            .err();
        Ok((to, shortcut))
    }

    /// Makes the shortcut at `lnk` to `target`, through PowerShell's `WScript.Shell` (by full path, script on stdin).
    pub fn make_shortcut(lnk: &Path, target: &Path) -> Result<(), String> {
        if let Some(parent) = lnk.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let mut ps = Command::new(format!(
            r"{root}\System32\WindowsPowerShell\v1.0\powershell.exe"
        ))
        .args(["-NoProfile", "-NonInteractive", "-Command", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("couldn't run PowerShell: {e}"))?;
        if let Some(mut stdin) = ps.stdin.take() {
            stdin
                .write_all(shortcut_script(lnk, target).as_bytes())
                .map_err(|e| e.to_string())?;
        }
        let done = ps.wait().map_err(|e| e.to_string())?;
        if done.success() && lnk.is_file() {
            Ok(())
        } else {
            Err("the Start-menu shortcut couldn't be made".into())
        }
    }

    /// Starts the installed Samo, to take over once this one quits.
    pub fn start_installed(exe: &Path) -> Result<(), String> {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        let me = crate::platform::inspect_process_identity(std::process::id())
            .ok_or("Samo couldn't read its own process")?;
        Command::new(exe)
            .arg(AFTER)
            .arg(me.pid.to_string())
            .arg(me.start_time_secs.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("couldn't start the installed Samo: {e}"))
    }

    /// Started with `--after`: waits (up to 30 s) for that Samo to quit, by its identity (R4).
    pub fn wait_after(args: &[String]) {
        let Some(old) = after_from(args) else {
            return;
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        while crate::platform::inspect_process_identity(old.pid) == Some(old)
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Whether Windows starts this Samo at sign-in.
    pub fn starts_at_sign_in() -> bool {
        let Ok(exe) = std::env::current_exe() else {
            return false;
        };
        windows_registry::CURRENT_USER
            .open(RUN_KEY)
            .and_then(|key| key.get_string("Samo"))
            .is_ok_and(|value| value.eq_ignore_ascii_case(&run_value(&exe)))
    }

    /// Starts this Samo at sign-in, or stops doing so.
    pub fn set_start_at_sign_in(on: bool) -> Result<(), String> {
        let key = windows_registry::CURRENT_USER
            .create(RUN_KEY)
            .map_err(|e| e.to_string())?;
        if on {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            key.set_string("Samo", run_value(&exe))
                .map_err(|e| e.to_string())
        } else {
            match key.remove_value("Samo") {
                Ok(()) => Ok(()),
                Err(_) if !starts_at_sign_in() => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }
    }
}

#[cfg(windows)]
pub use windows::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    fn get(name: &str) -> Option<OsString> {
        match name {
            "LOCALAPPDATA" => Some(r"C:\Users\Jo\AppData\Local".into()),
            "APPDATA" => Some(r"C:\Users\Jo\AppData\Roaming".into()),
            _ => None,
        }
    }

    #[test]
    fn samo_installs_in_programs_under_localappdata() {
        assert_eq!(
            install_dir_from(get),
            Some(
                PathBuf::from(r"C:\Users\Jo\AppData\Local")
                    .join("Programs")
                    .join("Samo")
            )
        );
        assert_eq!(
            shortcut_from(get),
            Some(
                PathBuf::from(r"C:\Users\Jo\AppData\Roaming")
                    .join("Microsoft")
                    .join("Windows")
                    .join("Start Menu")
                    .join("Programs")
                    .join("Samo.lnk")
            )
        );
        assert_eq!(install_dir_from(|_| None), None);
    }

    #[test]
    fn installed_means_samo_exe_in_its_folder() {
        let dir = Path::new(r"C:\Users\Jo\AppData\Local\Programs\Samo");
        assert!(is_installed_at(&dir.join("Samo.exe"), dir));
        // Windows paths are the same in any case.
        assert!(is_installed_at(
            Path::new(r"c:\users\jo\appdata\local\programs\samo\SAMO.EXE"),
            dir
        ));
        assert!(!is_installed_at(&dir.join("samo-gpui.exe"), dir));
        assert!(!is_installed_at(
            Path::new(r"C:\Users\Jo\Downloads\Samo\Samo.exe"),
            dir
        ));
    }

    #[test]
    fn the_shortcut_script_quotes_its_paths() {
        let script = shortcut_script(
            Path::new(r"C:\Users\O'Brien\Start Menu\Samo.lnk"),
            Path::new(r"C:\Users\O'Brien\Programs\Samo\Samo.exe"),
        );
        assert!(
            script.contains(r"CreateShortcut('C:\Users\O''Brien\Start Menu\Samo.lnk')"),
            "{script}"
        );
        assert!(
            script.contains(r"TargetPath = 'C:\Users\O''Brien\Programs\Samo\Samo.exe'"),
            "{script}"
        );
        assert!(
            script.contains(r"WorkingDirectory = 'C:\Users\O''Brien\Programs\Samo'"),
            "{script}"
        );
        assert!(script.trim_end().ends_with("$s.Save()"), "{script}");
    }

    #[test]
    fn start_at_sign_in_runs_the_quoted_program() {
        assert_eq!(
            run_value(Path::new(r"C:\Users\Jo Smith\Programs\Samo\Samo.exe")),
            r#""C:\Users\Jo Smith\Programs\Samo\Samo.exe""#
        );
    }

    /// A real shortcut, through PowerShell, in a temporary folder (never the Start menu). A path with `'` and a
    /// space in it, as a user name can have.
    #[cfg(windows)]
    #[test]
    fn a_shortcut_is_made_through_powershell() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let folder = dir.path().join("Jo O'Brien");
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let target = folder.join("Samo.exe");
        std::fs::write(&target, b"MZ").map_err(|e| e.to_string())?;
        let lnk = folder.join("Start Menu").join("Samo.lnk");
        make_shortcut(&lnk, &target)?;
        assert!(lnk.is_file());
        Ok(())
    }

    #[test]
    fn after_args_name_the_samo_to_wait_for() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            after_from(&args(&["Samo.exe", AFTER, "42", "1700000000"])),
            Some(samo_primitives::ProcessIdentity {
                pid: 42,
                start_time_secs: 1_700_000_000
            })
        );
        assert_eq!(after_from(&args(&["Samo.exe"])), None);
        assert_eq!(after_from(&args(&["Samo.exe", AFTER, "x", "1"])), None);
    }
}
