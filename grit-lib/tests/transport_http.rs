//! Integration test for the smart-HTTP transport: `SmartHttpTransport` +
//! `http_fetch` over the default `ureq`-backed `HttpClient` (feature `http-ureq`).
//!
//! A bare source repo (two commits on `main`, a `topic` branch, an annotated
//! tag) is built with the system `git` under a temp root. The `grit-http-server`
//! crate's binary is spawned over that root on a free localhost port; an empty
//! local repo then fetches `http://127.0.0.1:<port>/repo.git` via
//! `SmartHttpTransport::connect` (advertisement) + `http_fetch` (negotiation).
//! We assert the tracking refs + tag land, the objects arrive, the fetched main
//! tip matches `git rev-parse`, and the pack `fsck`s clean.
//!
//! The test skips gracefully (returns early) when `git`, the `grit` binary, or
//! the `grit-http-server` binary is unavailable, or the server fails to bind —
//! the happy path is otherwise real end-to-end HTTP wire I/O.
//!
//! Gated on the `http-ureq` feature (the default `UreqHttpClient` lives there):
//!   cargo test -p grit-lib --features http-ureq --test transport_http

#![cfg(feature = "http-ureq")]

use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use grit_lib::fetch::NoProgress;
use grit_lib::objects::ObjectId;
use grit_lib::odb::Odb;
use grit_lib::refs::resolve_ref;
use grit_lib::transfer::{FetchOptions, TagMode, UpdateMode};
use grit_lib::transport::http::{http_fetch, SmartHttpTransport};
use grit_lib::transport::http::ureq_client::UreqHttpClient;
use grit_lib::transport::{ConnectOptions, Service, Transport};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "T")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_AUTHOR_DATE", "2005-04-07T22:13:13 +0200")
        .env("GIT_COMMITTER_NAME", "T")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_DATE", "2005-04-07T22:13:13 +0200")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf8 git output")
}

fn rev_parse(dir: &Path, rev: &str) -> ObjectId {
    ObjectId::from_hex(git(dir, &["rev-parse", rev]).trim()).expect("valid oid")
}

fn open_odb(git_dir: &Path) -> Odb {
    Odb::new(&git_dir.join("objects")).with_config_git_dir(git_dir.to_path_buf())
}

/// Build a source repo: two commits on `main`, a `topic` branch, an annotated tag.
fn build_source(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main", "."]);
    std::fs::write(dir.join("a.txt"), "one\n").unwrap();
    git(dir, &["add", "a.txt"]);
    git(dir, &["commit", "-q", "-m", "c1"]);
    std::fs::write(dir.join("b.txt"), "two\n").unwrap();
    git(dir, &["add", "b.txt"]);
    git(dir, &["commit", "-q", "-m", "c2"]);
    git(dir, &["tag", "-a", "v1", "-m", "release one"]);
    git(dir, &["branch", "topic"]);
}

/// Pick a currently-free localhost port by binding then dropping a listener.
fn free_port() -> Option<u16> {
    let l = TcpListener::bind(("127.0.0.1", 0)).ok()?;
    let p = l.local_addr().ok()?.port();
    drop(l);
    Some(p)
}

/// Locate a sibling binary (`grit`, `grit-http-server`) in the cargo target
/// directory. The test executable lives at `target/<profile>/deps/<exe>`, so the
/// binaries are one directory up.
fn find_binary(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // .../target/<profile>/deps/transport_http-<hash>
    let deps = exe.parent()?; // deps
    let profile = deps.parent()?; // <profile>
    for cand in [profile.join(name), deps.join(name)] {
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// Spawn `grit-http-server --root <root> --bind 127.0.0.1:<port>`, pointing the
/// server's upload-pack at the built `grit` binary via `GUST_BIN`. Returns the
/// child handle, or `None` if a binary is missing.
fn spawn_server(server_bin: &Path, grit_bin: &Path, root: &Path, port: u16) -> Option<Child> {
    Command::new(server_bin)
        .arg("--root")
        .arg(root)
        .arg("--bind")
        .arg(format!("127.0.0.1:{port}"))
        .env("GUST_BIN", grit_bin)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

/// Wait until the HTTP server answers a TCP connect on `port`, or time out.
fn wait_ready(port: u16) -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

struct ServerGuard(Child);
impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn fetch_over_smart_http_lands_refs_and_objects() {
    let Some(grit_bin) = find_binary("grit") else {
        eprintln!("SKIP: `grit` binary not found in target dir (build grit-cli first)");
        return;
    };
    let Some(server_bin) = find_binary("grit-http-server") else {
        eprintln!("SKIP: `grit-http-server` binary not found (build grit-http-server first)");
        return;
    };

    // Build a source repo, then mirror it into a bare repo under the server root
    // (served at `/repo.git`).
    let tmp = tempfile::tempdir().expect("tempdir");
    let work = tmp.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    build_source(&work);

    let root = tmp.path().join("srv");
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("repo.git");
    git(
        &work,
        &["clone", "-q", "--bare", ".", source.to_str().expect("utf8 path")],
    );
    git(&source, &["symbolic-ref", "HEAD", "refs/heads/main"]);

    let main_oid = rev_parse(&source, "refs/heads/main");
    let topic_oid = rev_parse(&source, "refs/heads/topic");
    let c1_oid = rev_parse(&work, "HEAD~1");
    let tag_oid = rev_parse(&source, "refs/tags/v1");

    let Some(port) = free_port() else {
        eprintln!("SKIP: could not allocate a free port");
        return;
    };
    let Some(child) = spawn_server(&server_bin, &grit_bin, &root, port) else {
        eprintln!("SKIP: could not spawn grit-http-server");
        return;
    };
    let _guard = ServerGuard(child);
    if !wait_ready(port) {
        eprintln!("SKIP: grit-http-server did not become ready on port {port}");
        return;
    }

    let url = format!("http://127.0.0.1:{port}/repo.git");

    // Empty local repo.
    let local = tmp.path().join("local");
    std::fs::create_dir_all(&local).unwrap();
    git(&local, &["init", "-q", "-b", "main", "."]);
    let local_git = local.join(".git");

    // 1. Connect via the trait and check the advertisement.
    let client = UreqHttpClient::new();
    let transport = SmartHttpTransport::new(client);
    let conn = match transport.connect(&url, Service::UploadPack, &ConnectOptions::default()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("SKIP: could not connect to grit-http-server: {e}");
            return;
        }
    };
    assert!(
        conn.advertised_refs()
            .iter()
            .any(|(n, o)| n == "refs/heads/main" && *o == main_oid),
        "advertisement missing refs/heads/main = {}",
        main_oid.to_hex()
    );
    assert_eq!(conn.head_symref(), Some("refs/heads/main"));
    assert_eq!(conn.protocol_version(), 0);
    drop(conn);

    // 2. Fetch via http_fetch over a fresh client.
    let client = UreqHttpClient::new();
    let opts = FetchOptions {
        refspecs: vec!["+refs/heads/*:refs/remotes/origin/*".to_owned()],
        tags: TagMode::All,
        ..Default::default()
    };
    let outcome = http_fetch(&client, &local_git, &url, &opts, &mut NoProgress)
        .expect("http_fetch over grit-http-server");

    // Tracking refs written.
    let got_main = resolve_ref(&local_git, "refs/remotes/origin/main").expect("origin/main");
    let got_topic = resolve_ref(&local_git, "refs/remotes/origin/topic").expect("origin/topic");
    assert_eq!(got_main, main_oid, "origin/main oid mismatch vs source");
    assert_eq!(got_topic, topic_oid, "origin/topic oid mismatch vs source");

    // Annotated tag arrived (TagMode::All).
    let got_tag = resolve_ref(&local_git, "refs/tags/v1").expect("tag v1 written");
    assert_eq!(got_tag, tag_oid, "tag v1 oid mismatch vs source");

    // Objects landed in the local odb.
    let local_odb = open_odb(&local_git);
    for oid in [main_oid, topic_oid, c1_oid, tag_oid] {
        assert!(
            local_odb.exists(&oid),
            "object {} missing from local odb after http fetch",
            oid.to_hex()
        );
        local_odb
            .read(&oid)
            .unwrap_or_else(|e| panic!("read {}: {e}", oid.to_hex()));
    }

    // Per-ref update modes.
    let main_update = outcome
        .updates
        .iter()
        .find(|u| u.remote_ref == "refs/heads/main")
        .expect("update for main");
    assert_eq!(main_update.mode, UpdateMode::New);
    assert_eq!(main_update.new_oid, Some(main_oid));

    // Default branch from the server's HEAD symref.
    assert_eq!(outcome.default_branch.as_deref(), Some("main"));

    // Cross-check the fetched main tip against git's view of the source.
    assert_eq!(
        got_main.to_hex(),
        git(&source, &["rev-parse", "refs/heads/main"]).trim()
    );

    // The fetched pack re-indexes / fsck's clean in the local repo.
    let fsck = Command::new("git")
        .current_dir(&local)
        .args(["fsck", "--no-dangling"])
        .output()
        .expect("run git fsck");
    assert!(
        fsck.status.success(),
        "git fsck failed after http fetch: {}",
        String::from_utf8_lossy(&fsck.stderr)
    );
}
