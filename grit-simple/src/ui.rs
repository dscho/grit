//! Small output-formatting helpers shared by `gs` commands.

use std::io::IsTerminal;

use grit_lib::diff::{DiffEntry, DiffStatus};

/// Width of the change-label column, sized to the longest label
/// (`"type changed"`) so the paths after it line up.
const LABEL_WIDTH: usize = 12;

/// ANSI reset.
const RESET: &str = "\x1b[0m";

/// Whether to emit ANSI color: only when stdout is a TTY and `NO_COLOR` is unset
/// (the de-facto standard for opting out — https://no-color.org).
fn use_color() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

/// Wrap `text` in the SGR `code` (e.g. `"32"`) when `color` is enabled.
fn paint(color: bool, code: &str, text: &str) -> String {
    if color {
        format!("\x1b[{code}m{text}{RESET}")
    } else {
        text.to_owned()
    }
}

/// The path a diff entry refers to (prefers the new side, falls back to old).
pub fn entry_path(entry: &DiffEntry) -> &str {
    entry
        .new_path
        .as_deref()
        .or(entry.old_path.as_deref())
        .unwrap_or("?")
}

/// A single-character glyph summarizing a change.
fn glyph(status: &DiffStatus) -> char {
    match status {
        DiffStatus::Added => '+',
        DiffStatus::Deleted => '-',
        DiffStatus::Modified | DiffStatus::TypeChanged => '~',
        DiffStatus::Renamed | DiffStatus::Copied => '»',
        DiffStatus::Unmerged => '!',
    }
}

/// A short word describing a change, shown in the (left) label column.
fn label(entry: &DiffEntry) -> &'static str {
    match entry.status {
        DiffStatus::Added => "new",
        DiffStatus::Deleted => "deleted",
        DiffStatus::Modified => "modified",
        DiffStatus::TypeChanged => "type changed",
        DiffStatus::Renamed => "renamed",
        DiffStatus::Copied => "copied",
        DiffStatus::Unmerged => "conflict",
    }
}

/// ANSI SGR color code for a change status (green new, red deleted/conflict,
/// yellow modified, cyan renamed/copied).
fn status_color(status: &DiffStatus) -> &'static str {
    match status {
        DiffStatus::Added => "32",
        DiffStatus::Deleted | DiffStatus::Unmerged => "31",
        DiffStatus::Modified | DiffStatus::TypeChanged => "33",
        DiffStatus::Renamed | DiffStatus::Copied => "36",
    }
}

/// Print a titled group of diff entries (does nothing when empty).
///
/// Each line is `  <glyph>  <label>  <path>`: the glyph and the fixed-width label
/// column come first (colored by status on a TTY) so the paths line up.
pub fn print_change_group(title: &str, entries: &[DiffEntry]) {
    if entries.is_empty() {
        return;
    }
    let color = use_color();
    println!("{}", paint(color, "1", title));
    for entry in entries {
        let g = glyph(&entry.status);
        let l = label(entry);
        let marker = format!("{g}  {l:<width$}", width = LABEL_WIDTH);
        println!(
            "  {}  {}",
            paint(color, status_color(&entry.status), &marker),
            entry_path(entry)
        );
    }
    println!();
}

/// Print the untracked-files group (does nothing when empty).
pub fn print_untracked(paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    let color = use_color();
    println!("{}", paint(color, "1", "Untracked"));
    for path in paths {
        let marker = format!("?  {l:<width$}", l = "untracked", width = LABEL_WIDTH);
        println!("  {}  {path}", paint(color, "31", &marker));
    }
    println!();
}
