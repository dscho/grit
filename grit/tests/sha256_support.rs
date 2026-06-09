//! End-to-end tests for SHA-256 repository support.
//!
//! grit currently hardcodes object IDs to 20-byte SHA-1 (`ObjectId([u8; 20])`
//! in `grit-lib/src/objects.rs`, with `ObjectId::from_str` rejecting anything
//! that is not exactly 40 hex chars). As a result it cannot read repositories
//! created with `--object-format=sha256`, and its own write path produces
//! SHA-1-hashed objects inside a sha256 repo, corrupting it.
//!
//! These tests assert the *correct* (post-implementation) behaviour, so they
//! are expected to FAIL until real SHA-256 support lands. They cover:
//!   - init + commit round-trip that real `git` can read (init / commit)
//!   - `grit commit` producing a 64-hex OID that `grit` itself can resolve
//!   - `grit show` on a real-git sha256 repo
//!   - `grit log` on a real-git sha256 repo (the originally reported bug)
//!   - `grit rev-list` on a real-git sha256 repo
//!
//! Fixtures that must be genuinely SHA-256-correct are built with the system
//! `git` binary; the behaviour under test is always grit's.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const GRIT_BIN: &str = env!("CARGO_BIN_EXE_grit");

/// A captured command result.
struct Output {
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Output {
    fn ok(&self) -> bool {
        self.status == Some(0)
    }
    fn dump(&self, label: &str) -> String {
        format!(
            "{label}: exit={:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.status, self.stdout, self.stderr
        )
    }
}

/// Create a fresh, uniquely-named temporary directory (no external deps).
fn unique_tmp(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut p = std::env::temp_dir();
    p.push(format!(
        "grit-sha256-{tag}-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("create temp dir");
    p
}

/// Run a command in `dir` with a deterministic committer/author identity.
fn run(bin: &str, args: &[&str], dir: &Path) -> Output {
    let out = Command::new(bin)
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .env("GIT_AUTHOR_DATE", "1700000000 +0000")
        .env("GIT_COMMITTER_DATE", "1700000000 +0000")
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn {bin} {args:?}: {e}"));
    Output {
        status: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn grit(args: &[&str], dir: &Path) -> Output {
    run(GRIT_BIN, args, dir)
}

fn git(args: &[&str], dir: &Path) -> Output {
    run("git", args, dir)
}

fn write_file(dir: &Path, name: &str, contents: &str) {
    std::fs::write(dir.join(name), contents).expect("write file");
}

fn is_hex64(s: &str) -> bool {
    let s = s.trim();
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Build a SHA-256 repository using the system `git`, with one or more commits.
/// Returns the repo path. Panics (rather than skips) if the system `git`
/// cannot create a sha256 repo, so the requirement is visible.
fn real_git_sha256_repo(tag: &str, files: &[(&str, &str, &str)]) -> PathBuf {
    let dir = unique_tmp(tag);
    let init = git(&["init", "--object-format=sha256", "-q", "."], &dir);
    assert!(
        init.ok(),
        "system `git init --object-format=sha256` failed — git >= 2.29 with sha256 is required for this test\n{}",
        init.dump("git init")
    );
    // Sanity: confirm the fixture really is sha256.
    let fmt = git(&["rev-parse", "--show-object-format"], &dir);
    assert_eq!(fmt.stdout.trim(), "sha256", "fixture is not sha256");

    for (name, contents, msg) in files {
        write_file(&dir, name, contents);
        assert!(git(&["add", name], &dir).ok(), "git add failed");
        let c = git(&["commit", "-q", "-m", msg], &dir);
        assert!(c.ok(), "{}", c.dump("git commit"));
    }
    dir
}

#[test]
fn sha256_init_and_commit_roundtrip_readable_by_git() {
    // init + commit driven entirely by grit; the resulting repo must be a
    // valid sha256 repo that the system git can read back.
    let dir = unique_tmp("init-commit");

    let init = grit(&["init", "--object-format=sha256", "."], &dir);
    assert!(init.ok(), "{}", init.dump("grit init"));

    let fmt = grit(&["rev-parse", "--show-object-format"], &dir);
    assert_eq!(
        fmt.stdout.trim(),
        "sha256",
        "grit did not record sha256 object format\n{}",
        fmt.dump("show-object-format")
    );

    write_file(&dir, "a.txt", "hello sha256\n");
    assert!(grit(&["add", "a.txt"], &dir).ok(), "grit add failed");
    let commit = grit(&["commit", "-m", "first"], &dir);
    assert!(commit.ok(), "{}", commit.dump("grit commit"));

    // The system git must be able to resolve HEAD as a 64-hex sha256 OID,
    // and the object store must pass fsck (i.e. grit wrote real sha256
    // objects, not sha1 ones).
    let head = git(&["rev-parse", "HEAD"], &dir);
    assert!(head.ok(), "{}", head.dump("git rev-parse HEAD"));
    assert!(
        is_hex64(&head.stdout),
        "HEAD is not a 64-char sha256 OID: {:?}",
        head.stdout.trim()
    );

    let fsck = git(&["fsck", "--strict"], &dir);
    assert!(
        fsck.ok(),
        "git fsck failed — grit corrupted the sha256 repo\n{}",
        fsck.dump("git fsck")
    );
}

#[test]
fn sha256_commit_produces_sha256_oid_resolvable_by_grit() {
    let dir = unique_tmp("commit-oid");
    assert!(
        grit(&["init", "--object-format=sha256", "."], &dir).ok(),
        "grit init failed"
    );

    write_file(&dir, "f.txt", "content\n");
    assert!(grit(&["add", "f.txt"], &dir).ok(), "grit add failed");
    let commit = grit(&["commit", "-m", "msg"], &dir);
    assert!(commit.ok(), "{}", commit.dump("grit commit"));

    let head = grit(&["rev-parse", "HEAD"], &dir);
    assert!(head.ok(), "{}", head.dump("grit rev-parse HEAD"));
    assert!(
        is_hex64(&head.stdout),
        "grit rev-parse HEAD is not a 64-char sha256 OID: {:?}",
        head.stdout.trim()
    );
}

#[test]
fn sha256_show_reads_real_git_repo() {
    let dir = real_git_sha256_repo("show", &[("a.txt", "hello sha256\n", "first")]);

    let show = grit(&["show", "HEAD"], &dir);
    assert!(
        show.ok(),
        "grit show HEAD failed on a sha256 repo\n{}",
        show.dump("grit show")
    );
    assert!(
        show.stdout.contains("hello sha256"),
        "grit show output missing file contents\n{}",
        show.dump("grit show")
    );
}

#[test]
fn sha256_log_reads_real_git_repo() {
    // The originally reported bug: `grit log` -> "error: broken HEAD".
    let dir = real_git_sha256_repo("log", &[("a.txt", "x\n", "first commit")]);

    let log = grit(&["log"], &dir);
    assert!(
        !log.stderr.contains("broken HEAD"),
        "grit log reported 'broken HEAD' on a sha256 repo\n{}",
        log.dump("grit log")
    );
    assert!(
        log.ok(),
        "grit log failed on a sha256 repo\n{}",
        log.dump("grit log")
    );
    assert!(
        log.stdout.contains("first commit"),
        "grit log missing commit subject\n{}",
        log.dump("grit log")
    );
}

#[test]
fn sha256_rev_list_reads_real_git_repo() {
    let dir = real_git_sha256_repo(
        "rev-list",
        &[
            ("a.txt", "one\n", "c1"),
            ("b.txt", "two\n", "c2"),
        ],
    );

    let rl = grit(&["rev-list", "HEAD"], &dir);
    assert!(
        rl.ok(),
        "grit rev-list HEAD failed on a sha256 repo\n{}",
        rl.dump("grit rev-list")
    );
    let lines: Vec<&str> = rl.stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        2,
        "expected 2 commits from rev-list\n{}",
        rl.dump("grit rev-list")
    );
    for line in lines {
        assert!(
            is_hex64(line),
            "rev-list emitted a non-sha256 OID: {line:?}\n{}",
            rl.dump("grit rev-list")
        );
    }
}
