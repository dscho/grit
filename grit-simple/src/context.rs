//! Shared repository helpers used across `gi` commands.

use std::collections::HashSet;

use anyhow::{bail, Context, Result};
use grit_lib::config::ConfigSet;
use grit_lib::objects::{parse_commit, CommitData, ObjectId, ObjectKind};
use grit_lib::refs;
use grit_lib::repo::Repository;

/// A resolved "target" branch (the trunk `gi` measures the current branch against).
#[derive(Debug, Clone)]
pub struct TargetBranch {
    pub display_name: String,
    pub oid: ObjectId,
}

/// A one-line summary of a commit, for shortlog-style output.
#[derive(Debug, Clone)]
pub struct CommitSummary {
    pub oid: ObjectId,
    pub subject: String,
}

/// Discover the repository containing the current directory.
pub fn discover() -> Result<Repository> {
    Repository::discover(None).context("not in a repository")
}

/// Find the branch `gi` should measure the current branch against, trying
/// `target.branch` from config first, then the usual trunk names.
pub fn find_target_branch(repo: &Repository) -> Result<Option<TargetBranch>> {
    for candidate in target_branch_candidates(repo)? {
        if let Some(oid) = resolve_branch_candidate(repo, &candidate) {
            return Ok(Some(TargetBranch {
                display_name: candidate,
                oid,
            }));
        }
    }
    Ok(None)
}

fn target_branch_candidates(repo: &Repository) -> Result<Vec<String>> {
    let config = ConfigSet::load(Some(&repo.git_dir), true).context("could not load config")?;
    let mut candidates = Vec::new();
    if let Some(target) = config.get("target.branch") {
        let trimmed = target.trim();
        if !trimmed.is_empty() {
            candidates.push(trimmed.to_owned());
        }
    }
    candidates.extend([
        "origin/master".to_owned(),
        "origin/main".to_owned(),
        "master".to_owned(),
        "main".to_owned(),
    ]);
    Ok(candidates)
}

fn resolve_branch_candidate(repo: &Repository, candidate: &str) -> Option<ObjectId> {
    for refname in candidate_refnames(candidate) {
        if let Ok(oid) = refs::resolve_ref(&repo.git_dir, &refname) {
            return Some(oid);
        }
    }
    None
}

fn candidate_refnames(candidate: &str) -> Vec<String> {
    if candidate.starts_with("refs/") || candidate == "HEAD" {
        return vec![candidate.to_owned()];
    }

    if let Some(remote_branch) = candidate.strip_prefix("origin/") {
        return vec![
            format!("refs/remotes/origin/{remote_branch}"),
            format!("refs/heads/{candidate}"),
        ];
    }

    vec![
        format!("refs/heads/{candidate}"),
        format!("refs/remotes/{candidate}"),
    ]
}

/// Commits reachable from `head` but not from `target`, newest first.
pub fn commits_ahead_of(
    repo: &Repository,
    head: ObjectId,
    target: ObjectId,
) -> Result<Vec<CommitSummary>> {
    let excluded = reachable_commits(repo, target)?;
    let mut seen = HashSet::new();
    let mut stack = vec![head];
    let mut commits = Vec::new();

    while let Some(oid) = stack.pop() {
        if !seen.insert(oid) || excluded.contains(&oid) {
            continue;
        }
        let commit = read_commit(repo, &oid)?;
        stack.extend(commit.parents.iter().copied());
        commits.push(CommitSummary {
            oid,
            subject: subject_line(&commit.message),
        });
    }

    Ok(commits)
}

fn reachable_commits(repo: &Repository, start: ObjectId) -> Result<HashSet<ObjectId>> {
    let mut reachable = HashSet::new();
    let mut stack = vec![start];

    while let Some(oid) = stack.pop() {
        if !reachable.insert(oid) {
            continue;
        }
        let commit = read_commit(repo, &oid)?;
        stack.extend(commit.parents.iter().copied());
    }

    Ok(reachable)
}

fn read_commit(repo: &Repository, oid: &ObjectId) -> Result<CommitData> {
    let object = repo
        .odb
        .read(oid)
        .with_context(|| format!("could not read commit {oid}"))?;
    if object.kind != ObjectKind::Commit {
        bail!("object {oid} is a {}, not a commit", object.kind);
    }
    parse_commit(&object.data).with_context(|| format!("could not parse commit {oid}"))
}

/// The first non-blank line of a commit message.
pub fn subject_line(message: &str) -> String {
    message
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or("(no subject)")
        .to_owned()
}

/// An abbreviated, 7-character object id.
pub fn short_oid(oid: &ObjectId) -> String {
    oid.to_hex().chars().take(7).collect()
}
