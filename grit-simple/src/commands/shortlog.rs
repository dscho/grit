//! `gs shortlog` — the commits on this branch that aren't on the target yet.

use anyhow::{bail, Context, Result};
use grit_lib::objects::ObjectId;
use grit_lib::state::{resolve_head, HeadState};
use serde::Serialize;

use crate::context;
use crate::output::{CommitJson, HumanRender};

/// Result of `gs shortlog`.
#[derive(Serialize)]
pub struct ShortlogOutcome {
    pub branch: String,
    /// Target branch name, or `null` when none could be resolved.
    pub target: Option<String>,
    pub ahead: usize,
    pub commits: Vec<CommitJson>,
}

impl HumanRender for ShortlogOutcome {
    fn render_human(&self) {
        println!("On {}", self.branch);
        let Some(target) = &self.target else {
            println!("No target branch found (tried target.branch, origin/master, origin/main, master, main).");
            return;
        };
        println!(
            "Ahead of {} by {} commit{}",
            target,
            self.ahead,
            if self.ahead == 1 { "" } else { "s" }
        );
        for commit in &self.commits {
            println!("{} {}", short_hex(&commit.oid), commit.subject);
        }
    }
}

fn short_hex(oid: &str) -> &str {
    oid.get(..7).unwrap_or(oid)
}

pub fn run() -> Result<ShortlogOutcome> {
    let repo = context::discover()?;
    let head = resolve_head(&repo.git_dir).context("could not resolve HEAD")?;
    let (branch_name, head_oid) = current_branch_and_oid(&head)?;

    let Some(target) = context::find_target_branch(&repo)? else {
        return Ok(ShortlogOutcome {
            branch: branch_name.to_owned(),
            target: None,
            ahead: 0,
            commits: Vec::new(),
        });
    };

    let commits = context::commits_ahead_of(&repo, head_oid, target.oid)?;
    Ok(ShortlogOutcome {
        branch: branch_name.to_owned(),
        target: Some(target.display_name),
        ahead: commits.len(),
        commits: commits.iter().map(CommitJson::from_summary).collect(),
    })
}

fn current_branch_and_oid(head: &HeadState) -> Result<(&str, ObjectId)> {
    match head {
        HeadState::Branch {
            short_name,
            oid: Some(oid),
            ..
        } => Ok((short_name.as_str(), *oid)),
        HeadState::Branch { short_name, .. } => {
            bail!("branch '{short_name}' does not have any commits yet")
        }
        HeadState::Detached { .. } => bail!("HEAD is detached; gs shortlog needs a current branch"),
        HeadState::Invalid => bail!("HEAD is invalid"),
    }
}
