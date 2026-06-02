//! `grit filter-branch` — rewrite branches by delegating to the system's
//! `git-filter-branch` shell script.

use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use std::process::Command;

/// Arguments for `grit filter-branch`.
#[derive(Debug, ClapArgs)]
#[command(about = "Rewrite branches (delegates to system git-filter-branch)")]
pub struct Args {
    /// Raw arguments forwarded to git-filter-branch.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}

/// Resolve `git-filter-branch` and the exec-path directory used for helper scripts.
fn resolve_filter_branch_script() -> Result<(std::path::PathBuf, std::path::PathBuf)> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exec_path) = std::env::var("GIT_EXEC_PATH") {
        candidates.push(std::path::PathBuf::from(exec_path).join("git-filter-branch"));
    }
    for dir in &[
        "/usr/lib/git-core",
        "/usr/libexec/git-core",
        "/usr/local/lib/git-core",
        "/usr/local/libexec/git-core",
    ] {
        candidates.push(std::path::Path::new(dir).join("git-filter-branch"));
    }
    for path in candidates {
        if path.is_file() {
            let exec_dir = path.parent().unwrap_or(path.as_path()).to_path_buf();
            return Ok((path, exec_dir));
        }
    }
    anyhow::bail!("cannot find git-filter-branch");
}

pub fn run(args: Args) -> Result<()> {
    let (script_path, exec_path) = resolve_filter_branch_script()?;

    // Prepend the exec path to PATH so that `git-sh-setup` and other
    // shell helpers sourced by filter-branch can be found.
    let current_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{current_path}", exec_path.display());

    let status = Command::new("bash")
        .arg(script_path)
        .args(&args.args)
        .env("PATH", &new_path)
        .status()
        .context("failed to run git-filter-branch")?;

    std::process::exit(status.code().unwrap_or(1));
}
