//! `gs config` — read, set, and list configuration values.
//!
//! A deliberately small subset of `git config`: read a value, set one, list
//! everything, or unset a key. By default it operates on the repository's local
//! config (`.git/config`); `--global` reads from or writes to your per-user
//! config (`~/.gitconfig`).

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use grit_lib::config::{ConfigFile, ConfigScope, ConfigSet};

use crate::context;

/// Run `gs config`.
///
/// - `--list` prints every key/value pair.
/// - a key with a value sets it.
/// - a key with `--unset` removes it.
/// - a key alone prints its current value.
pub fn run(
    global: bool,
    list: bool,
    unset: bool,
    key: Option<String>,
    value: Option<String>,
) -> Result<()> {
    if list {
        if key.is_some() || value.is_some() {
            bail!("--list does not take a key or value");
        }
        return list_values(global);
    }

    let Some(key) = key else {
        bail!("specify a config key (e.g. user.name), or pass --list to see everything");
    };

    if unset {
        if value.is_some() {
            bail!("--unset does not take a value");
        }
        return unset_value(global, &key);
    }

    match value {
        Some(value) => set_value(global, &key, &value),
        None => get_value(global, &key),
    }
}

/// Set `key` to `value` in the global config (used by `gs auth` to wire up the
/// Windows credential helper).
#[cfg(windows)]
pub fn set_global(key: &str, value: &str) -> Result<()> {
    set_value(true, key, value)
}

fn get_value(global: bool, key: &str) -> Result<()> {
    let config = read_config(global)?;
    match config.get(key) {
        Some(value) => {
            println!("{value}");
            Ok(())
        }
        None => bail!("key '{key}' is not set"),
    }
}

fn list_values(global: bool) -> Result<()> {
    let config = read_config(global)?;
    for entry in config.entries() {
        match &entry.value {
            Some(value) => println!("{}={value}", entry.key),
            None => println!("{}", entry.key),
        }
    }
    Ok(())
}

fn set_value(global: bool, key: &str, value: &str) -> Result<()> {
    let (scope, path) = target_file(global)?;
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let mut cfg = ConfigFile::parse(&path, &content, scope)
        .with_context(|| format!("could not parse {}", path.display()))?;
    cfg.set(key, value)
        .with_context(|| format!("could not set {key}"))?;
    cfg.write()
        .with_context(|| format!("could not write {}", path.display()))?;
    Ok(())
}

fn unset_value(global: bool, key: &str) -> Result<()> {
    let (scope, path) = target_file(global)?;
    let Ok(content) = std::fs::read_to_string(&path) else {
        bail!("key '{key}' is not set");
    };
    let mut cfg = ConfigFile::parse(&path, &content, scope)
        .with_context(|| format!("could not parse {}", path.display()))?;
    let removed = cfg
        .unset(key)
        .with_context(|| format!("could not unset {key}"))?;
    if removed == 0 {
        bail!("key '{key}' is not set");
    }
    cfg.write()
        .with_context(|| format!("could not write {}", path.display()))?;
    Ok(())
}

/// Load config for a read. `--global` reads only the global file; otherwise we
/// read the merged repo config (system + global + local), falling back to the
/// global/system files when run outside a repository.
fn read_config(global: bool) -> Result<ConfigSet> {
    if global {
        let mut set = ConfigSet::new();
        if let Some(path) = global_config_path() {
            if let Some(file) = ConfigFile::from_path(&path, ConfigScope::Global)
                .with_context(|| format!("could not read {}", path.display()))?
            {
                set.merge(&file);
            }
        }
        Ok(set)
    } else if let Ok(repo) = context::discover() {
        ConfigSet::load(Some(&repo.git_dir), true).context("could not load config")
    } else {
        ConfigSet::load(None, true).context("could not load config")
    }
}

/// The scope and file path a write targets: the global file with `--global`,
/// otherwise the current repository's local config.
fn target_file(global: bool) -> Result<(ConfigScope, PathBuf)> {
    if global {
        let path = global_config_path().context("could not determine your global config path")?;
        Ok((ConfigScope::Global, path))
    } else {
        let repo = context::discover()?;
        Ok((ConfigScope::Local, repo.git_dir.join("config")))
    }
}

/// The global config file to write to.
///
/// We derive this from grit-lib's own global-config search list
/// ([`global_config_paths_pub`]) so a write always lands in a file the loader
/// will read back — crucial on Windows, where the home directory may resolve via
/// `%USERPROFILE%` rather than `$HOME`. Following Git's writer preference, we use
/// an existing `~/.gitconfig`, then an existing XDG `git/config`, otherwise the
/// conventional `~/.gitconfig`.
fn global_config_path() -> Option<PathBuf> {
    let paths = grit_lib::config::global_config_paths_pub();
    // grit-lib returns `[XDG git/config, ~/.gitconfig]` (or a single
    // `$GIT_CONFIG_GLOBAL`); the last entry is the conventional `~/.gitconfig`.
    let dotgitconfig = paths.last().cloned();
    if dotgitconfig.as_ref().is_some_and(|p| p.exists()) {
        return dotgitconfig;
    }
    if let Some(xdg) = paths.first() {
        if xdg.exists() {
            return Some(xdg.clone());
        }
    }
    dotgitconfig
}
