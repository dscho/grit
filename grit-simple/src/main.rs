//! `gi` — a small, opinionated command line interface backed by `grit-lib`.
//!
//! `gi` deliberately does not mirror Git's UX. It favors a single obvious way
//! to do the common thing, plain-language output, and a status screen that
//! doubles as the home base: running `gi` with no arguments shows you where you
//! are, what's changed, and what to do next.

mod commands;
mod context;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// A simplified alternative to the Git-compatible `grit` command line.
#[derive(Debug, Parser)]
#[command(name = "gi", version, about = "A simple Grit-powered CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

/// Top-level `gi` commands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Show what's changed and where you are (this is the default).
    #[command(alias = "st")]
    Status,
    /// List the commits on this branch that aren't on the target branch yet.
    #[command(alias = "sl")]
    Shortlog,
    /// Stage changes. With no paths, stages everything.
    Add {
        /// Files or directories to stage. Omit to stage all changes.
        paths: Vec<String>,
    },
    /// Record the staged changes as a new commit.
    Commit {
        /// Commit message (you can also pass it with -m).
        message: Option<String>,
        /// Commit message.
        #[arg(short = 'm', long = "message", conflicts_with = "message")]
        message_flag: Option<String>,
        /// Stage every change first, then commit.
        #[arg(short = 'a', long = "all")]
        all: bool,
    },
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Status) {
        Command::Status => commands::status::run(),
        Command::Shortlog => commands::shortlog::run(),
        Command::Add { paths } => commands::add::run(&paths),
        Command::Commit {
            message,
            message_flag,
            all,
        } => commands::commit::run(message.or(message_flag), all),
    }
}
