//! `samo-fake-helper`: a stand-in for an extension's helper in the host's tests, on every OS. It follows the
//! script beside it (`<its name>.script`), one command per line, so a test can make it say hello, flood, crash,
//! hang, spawn children or burn CPU. Never shipped.
//!
//! Lines before the first `@<verb>` run for every verb; then the section for the verb Samo ran it with (`serve`,
//! `on`, `off`, `uninstall`). `$DATA` is its data folder, `$HOME` the home folder it was given (`USERPROFILE` on
//! Windows), `$PID` its process id; `{i}` counts inside `repeat`.
//!
//! `hello` · `say <line>` · `say-x <n>` · `repeat <n> <template>` · `wait-shutdown` · `sleep <secs>` ·
//! `exit <code>` · `write <path> <text>` · `env-to <path>` · `cwd-to <path>` · `spawn-sleeper <secs> [<pidfile>]` ·
//! `spawn <json array: program and arguments>` (its pid goes to `$DATA/spawned`) · `burn` · `light` · `hold-mb <n>`

use std::env;
use std::fs;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let verb = args.first().cloned().unwrap_or_default();
    if verb == "__sleep" {
        let secs = args
            .get(1)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(30.0);
        thread::sleep(Duration::from_secs_f64(secs));
        return ExitCode::SUCCESS;
    }
    let Some(script) = env::current_exe()
        .ok()
        .map(|exe| exe.with_extension("script"))
        .and_then(|path| fs::read_to_string(path).ok())
    else {
        eprintln!("samo-fake-helper: no script beside it");
        return ExitCode::from(2);
    };
    let mut section = String::new();
    let mut lines = Vec::new();
    for line in script.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(name) = line.strip_prefix('@') {
            section = name.to_string();
        } else if section.is_empty() || section == verb {
            lines.push(line.to_string());
        }
    }
    let mut held: Vec<Vec<u8>> = Vec::new();
    for line in lines {
        if let Some(code) = run(&expand(&line), &mut held) {
            return ExitCode::from(code);
        }
    }
    ExitCode::SUCCESS
}

fn expand(line: &str) -> String {
    let var = |name: &str| env::var(name).unwrap_or_default();
    let home = if cfg!(windows) {
        var("USERPROFILE")
    } else {
        var("HOME")
    };
    line.replace("$DATA", &var("SAMO_EXT_DATA_DIR"))
        .replace("$HOME", &home)
        .replace("$PID", &std::process::id().to_string())
}

fn say(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{text}");
    let _ = out.flush();
}

/// Runs one command; `Some(code)` ends the helper with that exit code.
fn run(line: &str, held: &mut Vec<Vec<u8>>) -> Option<u8> {
    let (command, rest) = line.split_once(' ').unwrap_or((line, ""));
    let number = |text: &str| text.trim().parse::<u64>().unwrap_or(0);
    match command {
        "hello" => say(r#"{"type":"hello","protocol":1}"#),
        "say" => say(rest),
        "say-x" => say(&"x".repeat(usize::try_from(number(rest)).unwrap_or(0))),
        "repeat" => {
            let (n, template) = rest.split_once(' ').unwrap_or((rest, ""));
            for i in 1..=number(n) {
                say(&template.replace("{i}", &i.to_string()));
            }
        }
        "wait-shutdown" => {
            for line in std::io::stdin().lock().lines() {
                match line {
                    Ok(line) if line.contains("shutdown") => return Some(0),
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }
        "sleep" => thread::sleep(Duration::from_secs_f64(rest.trim().parse().unwrap_or(0.0))),
        "exit" => return Some(u8::try_from(number(rest)).unwrap_or(1)),
        "write" => {
            let (path, text) = rest.split_once(' ').unwrap_or((rest, ""));
            let _ = fs::write(path, text);
        }
        "env-to" => {
            let text: String = env::vars_os()
                .map(|(k, v)| format!("{}={}\n", k.to_string_lossy(), v.to_string_lossy()))
                .collect();
            let _ = fs::write(rest.trim(), text);
        }
        "cwd-to" => {
            let cwd = env::current_dir().unwrap_or_default();
            let _ = fs::write(rest.trim(), cwd.to_string_lossy().as_bytes());
        }
        "spawn-sleeper" => {
            let mut parts = rest.split_whitespace();
            let secs = parts.next().unwrap_or("30");
            let exe = env::current_exe().unwrap_or_default();
            if let Ok(child) = Command::new(exe).args(["__sleep", secs]).spawn()
                && let Some(pidfile) = parts.next()
            {
                let _ = fs::write(PathBuf::from(pidfile), child.id().to_string());
            }
        }
        "spawn" => {
            let argv: Vec<String> = serde_json::from_str(rest).unwrap_or_default();
            if let Some((program, args)) = argv.split_first()
                && let Ok(child) = Command::new(program)
                    .args(args)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .spawn()
            {
                let data = PathBuf::from(env::var("SAMO_EXT_DATA_DIR").unwrap_or_default());
                let _ = fs::write(data.join("spawned"), child.id().to_string());
            }
        }
        "burn" => loop {
            std::hint::black_box(0u64.wrapping_add(1));
        },
        "light" => loop {
            let busy = Instant::now();
            while busy.elapsed() < Duration::from_millis(20) {
                std::hint::black_box(0u64.wrapping_add(1));
            }
            thread::sleep(Duration::from_millis(80));
        },
        "hold-mb" => {
            let bytes = usize::try_from(number(rest)).unwrap_or(0) * 1024 * 1024;
            held.push(vec![1u8; bytes]);
        }
        other => {
            eprintln!("samo-fake-helper: unknown command `{other}`");
            return Some(2);
        }
    }
    None
}
