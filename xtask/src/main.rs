mod commands_complete;
mod layers;
mod network;
mod purity;
mod rls;

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
    if subcmd != "verify" {
        return Err(format!(
            "unknown xtask command `{subcmd}`; expected `verify`"
        ));
    }

    let root = workspace_root()?;
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
            eprintln!("xtask verify failed: {err}");
            ExitCode::FAILURE
        }
    }
}
