mod commands_complete;
mod layers;
mod network;
mod purity;
mod rls;
mod store;

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

pub fn slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn display_rel(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rel) => slashes(rel),
        Err(_) => slashes(path),
    }
}

fn workspace_root() -> Result<PathBuf, String> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?;
    let path = PathBuf::from(manifest_dir);
    path.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "failed to resolve workspace root from xtask".to_string())
}

fn run_tests(root: &Path) -> Result<(), String> {
    let status = Command::new("cargo")
        .arg("test")
        .arg("--workspace")
        .current_dir(root)
        .status()
        .map_err(|e| format!("failed to invoke cargo test: {e}"))?;
    if !status.success() {
        return Err("cargo test --workspace failed".to_string());
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let subcmd = args.first().map(String::as_str).unwrap_or("verify");
    let root = workspace_root()?;
    match subcmd {
        "verify" => verify(&root),
        "ext-keygen" => store::keygen(),
        "ext-package" => {
            let dir = args
                .get(1)
                .ok_or("usage: cargo run -p xtask -- ext-package <extension folder>")?;
            store::package(&root, &root.join(dir)).map(|_| ())
        }
        "ext-publish" => store::publish(&publish_args(&root, &args[1..])?),
        other => Err(format!(
            "unknown xtask command `{other}`; expected `verify`, `ext-keygen`, `ext-package` or `ext-publish`"
        )),
    }
}

/// `cargo run -p xtask -- ext-publish --store <dir> [--packages target/ext-dist] [--samo <zip>] [--out dist] [--test-key <pkcs8>]`
fn publish_args(root: &Path, args: &[String]) -> Result<store::PublishArgs, String> {
    let mut store = None;
    let mut packages = root.join("target/ext-dist");
    let mut samo = None;
    let mut out = None;
    let mut test_key = None;
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| format!("`{flag}` needs a value"))?;
        match flag.as_str() {
            "--store" => store = Some(value),
            "--packages" => packages = value,
            "--samo" => samo = Some(value),
            "--out" => out = Some(value),
            "--test-key" => test_key = Some(value),
            other => return Err(format!("ext-publish: unknown flag `{other}`")),
        }
    }
    let store =
        store.ok_or("usage: cargo run -p xtask -- ext-publish --store <samo-extensions folder>")?;
    let trusted = match &test_key {
        Some(_) => None,
        None => Some(store::trusted_key(root)?),
    };
    Ok(store::PublishArgs {
        trusted,
        out: out.unwrap_or_else(|| store.join("dist")),
        store,
        packages,
        samo,
        test_key,
        published_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    })
}

fn verify(root: &Path) -> Result<(), String> {
    let root = root.to_path_buf();
    println!("==> [xtask] gate 1/6: layers");
    layers::check_layers(&root)?;
    println!("    layers: ok");

    println!("==> [xtask] gate 2/6: purity");
    purity::check_purity(&root)?;
    println!("    purity: ok");

    println!("==> [xtask] gate 3/6: rls");
    rls::check_rls(&root)?;
    println!("    rls: ok");

    println!("==> [xtask] gate 4/6: commands-complete");
    commands_complete::check_commands_complete(&root)?;
    println!("    commands-complete: ok");

    println!("==> [xtask] gate 5/6: network");
    network::check_network(&root)?;
    println!("    network: ok");

    if env::var("XTASK_SKIP_TESTS").as_deref() != Ok("1") {
        println!("==> [xtask] gate 6/6: tests");
        run_tests(&root)?;
        println!("    tests: ok");
    }

    println!("All xtask verify gates passed.");
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask failed: {err}");
            ExitCode::FAILURE
        }
    }
}
