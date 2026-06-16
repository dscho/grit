//! `gs log` — the recent history reachable from HEAD.
//!
//! Deliberately minimal: it shows one page of commits (newest first) and, when
//! there's more, prints the command to fetch the next page.

use anyhow::{Context, Result};
use grit_lib::rev_list::{rev_list, RevListOptions};
use serde::Serialize;

use crate::context::{self, subject_line};
use crate::output::{CommitJson, HumanRender};

/// How many commits to show per page.
const PAGE: usize = 10;

/// Result of `gs log`: one page of commits, plus the next page's start (if any).
#[derive(Serialize)]
pub struct LogOutcome {
    pub commits: Vec<CommitJson>,
    /// Full oid to resume from (`gs log --before=<next>`), or `null` when there
    /// is no further history.
    pub next: Option<String>,
}

impl HumanRender for LogOutcome {
    fn render_human(&self) {
        if self.commits.is_empty() {
            println!("No commits yet.");
            return;
        }
        for commit in &self.commits {
            println!("{}  {}", short_hex(&commit.oid), commit.subject);
        }
        if let Some(next) = &self.next {
            println!();
            println!("→ more: gs log --before={}", short_hex(next));
        }
    }
}

/// Abbreviate a full hex oid to the 7-char short form used in human output.
fn short_hex(oid: &str) -> &str {
    oid.get(..7).unwrap_or(oid)
}

pub fn run(before: Option<String>) -> Result<LogOutcome> {
    let repo = context::discover()?;
    let start = before.unwrap_or_else(|| "HEAD".to_owned());

    let opts = RevListOptions {
        // One extra so we know whether there's a next page.
        max_count: Some(PAGE + 1),
        ..Default::default()
    };
    let result = rev_list(&repo, std::slice::from_ref(&start), &[], &opts)
        .with_context(|| format!("could not list commits from {start}"))?;

    let commits = result
        .commits
        .iter()
        .take(PAGE)
        .map(|oid| {
            let commit = context::read_commit(&repo, oid)?;
            Ok(CommitJson {
                oid: oid.to_hex(),
                subject: subject_line(&commit.message),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let next = result.commits.get(PAGE).map(grit_lib::objects::ObjectId::to_hex);

    Ok(LogOutcome { commits, next })
}
