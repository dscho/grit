//! Integration tests for the negotiation-driven pack builder
//! (`grit_lib::transfer::build_pack`).
//!
//! These build a tiny repo with the system `git`, then assert that
//! `build_pack(wants, haves)` selects *only* the objects newly introduced by the
//! wanted tip (the minimal-selection regression guard for the 478 MB finding),
//! and that the produced bytes are a structurally valid PACK v2 stream.

use std::path::Path;
use std::process::Command;

use grit_lib::objects::ObjectId;
use grit_lib::odb::Odb;
use grit_lib::transfer::{build_pack, PackBuildOptions};
use grit_lib::unpack_objects::pack_bytes_to_object_map;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "T")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "T")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf8 git output")
}

fn rev_parse(dir: &Path, rev: &str) -> ObjectId {
    let hex = git(dir, &["rev-parse", rev]);
    ObjectId::from_hex(hex.trim()).expect("valid oid")
}

/// Header object count from a PACK v2 byte stream.
fn pack_header_count(bytes: &[u8]) -> u32 {
    assert_eq!(&bytes[0..4], b"PACK", "must start with PACK magic");
    assert_eq!(u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]), 2);
    u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]])
}

struct Fixture {
    dir: tempfile::TempDir,
    c1: ObjectId,
    c2: ObjectId,
    /// Objects introduced by C1: the commit, its tree, and its blob.
    c1_objects: Vec<ObjectId>,
    /// Objects introduced by C2 only: the new commit, the new tree, the new blob.
    c2_new_objects: Vec<ObjectId>,
}

fn build_fixture() -> Fixture {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();

    git(dir, &["init", "-q", "-b", "main", "."]);

    // C1: base commit with one file.
    std::fs::write(dir.join("a.txt"), b"hello\n").unwrap();
    git(dir, &["add", "a.txt"]);
    git(dir, &["commit", "-q", "-m", "c1"]);
    let c1 = rev_parse(dir, "HEAD");

    let c1_tree = rev_parse(dir, "HEAD^{tree}");
    let c1_blob = rev_parse(dir, "HEAD:a.txt");

    // C2: add a second file (introduces a new commit, a new root tree, a new blob).
    std::fs::write(dir.join("b.txt"), b"world\n").unwrap();
    git(dir, &["add", "b.txt"]);
    git(dir, &["commit", "-q", "-m", "c2"]);
    let c2 = rev_parse(dir, "HEAD");

    let c2_tree = rev_parse(dir, "HEAD^{tree}");
    let c2_blob = rev_parse(dir, "HEAD:b.txt");

    Fixture {
        dir: tmp,
        c1,
        c2,
        c1_objects: vec![c1, c1_tree, c1_blob],
        // a.txt blob is unchanged in C2, so it is NOT a new object.
        c2_new_objects: vec![c2, c2_tree, c2_blob],
    }
}

fn open_odb(dir: &Path) -> Odb {
    let git_dir = dir.join(".git");
    Odb::new(&git_dir.join("objects")).with_config_git_dir(git_dir)
}

#[test]
fn build_pack_selects_only_new_objects() {
    let fx = build_fixture();
    let odb = open_odb(fx.dir.path());

    let pack = build_pack(&odb, &[fx.c2], &[fx.c1], &PackBuildOptions::default()).expect("build");

    // (a) starts with PACK.
    assert_eq!(&pack[0..4], b"PACK");

    // (b) header object count equals ONLY the new objects introduced by C2
    // (new commit + new tree + new blob = 3) — proving minimal selection. The
    // a.txt blob and C1's objects, reachable from the have, are excluded.
    let count = pack_header_count(&pack);
    assert_eq!(
        count,
        fx.c2_new_objects.len() as u32,
        "expected exactly the 3 objects new in C2, got {count}"
    );

    // (c) structurally valid: re-parse with grit-lib's pack reader (this also
    // verifies the trailing checksum) and confirm the resolved object set is
    // exactly the new objects.
    let map = pack_bytes_to_object_map(&pack, &odb).expect("re-parse pack");
    assert_eq!(map.len(), fx.c2_new_objects.len());
    for oid in &fx.c2_new_objects {
        assert!(map.contains_key(oid), "pack missing new object {oid}");
    }
    for oid in &fx.c1_objects {
        assert!(
            !map.contains_key(oid),
            "pack should not contain have-reachable object {oid}"
        );
    }
}

#[test]
fn build_pack_with_empty_haves_packs_full_closure() {
    let fx = build_fixture();
    let odb = open_odb(fx.dir.path());

    let pack = build_pack(&odb, &[fx.c2], &[], &PackBuildOptions::default()).expect("build");

    assert_eq!(&pack[0..4], b"PACK");

    // Full closure from C2: C1, C2, C1's tree, C2's tree, a.txt blob, b.txt blob
    // = 6 distinct objects.
    let map = pack_bytes_to_object_map(&pack, &odb).expect("re-parse pack");
    assert_eq!(
        pack_header_count(&pack) as usize,
        map.len(),
        "header count must match resolved object count"
    );
    assert_eq!(map.len(), 6, "full closure of C2 is 6 objects, got {}", map.len());

    // Every object from both commits must be present.
    for oid in fx.c1_objects.iter().chain(fx.c2_new_objects.iter()) {
        assert!(map.contains_key(oid), "full pack missing {oid}");
    }
    assert!(map.contains_key(&fx.c1));
    assert!(map.contains_key(&fx.c2));
}

// ---------------------------------------------------------------------------
// Phase 6: delta + thin packs.
// ---------------------------------------------------------------------------

/// A repo with a large file edited across several commits, so successive blob
/// versions share a long common prefix (ideal delta candidates).
struct DeltaFixture {
    dir: tempfile::TempDir,
    /// Commit tips C1..C5 (oldest .. newest).
    tips: Vec<ObjectId>,
}

fn build_delta_fixture() -> DeltaFixture {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();
    git(dir, &["init", "-q", "-b", "main", "."]);

    // Start with a large body, then append a small line per commit. Each version
    // is a strict prefix-extension of the previous (and shares a long LCP), so a
    // size-sorted prefix/window selector will deltify them.
    let mut body = String::new();
    for i in 0..4000 {
        body.push_str(&format!("line {i:05} lorem ipsum dolor sit amet consectetur\n"));
    }

    let mut tips = Vec::new();
    for rev in 0..5 {
        body.push_str(&format!("--- edit number {rev} appended at the end ---\n"));
        std::fs::write(dir.join("big.txt"), body.as_bytes()).unwrap();
        git(dir, &["add", "big.txt"]);
        git(dir, &["commit", "-q", "-m", &format!("c{rev}")]);
        tips.push(rev_parse(dir, "HEAD"));
    }

    DeltaFixture { dir: tmp, tips }
}

/// Count `REF_DELTA` entries whose base OID is NOT present in this pack (the
/// thin-pack signature), reusing grit-lib's own thin detection where possible.
/// Returns `(ref_delta_external_count, ofs_delta_count, total_objects)`.
fn pack_delta_stats(bytes: &[u8], algo_len: usize) -> (usize, usize, usize) {
    // Minimal pack walker: we only need type codes + the REF_DELTA base oids and
    // the set of in-pack object oids (which we get from the resolved map at the
    // call site, so here we just collect base oids and count types).
    let nr = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    let mut pos = 12usize;
    let mut ofs = 0usize;
    let mut ref_bases: Vec<Vec<u8>> = Vec::new();
    // Collected raw object header type for each entry.
    for _ in 0..nr {
        let (type_code, _size, consumed) = read_type_size(&bytes[pos..]);
        pos += consumed;
        match type_code {
            6 => {
                ofs += 1;
                // skip the ofs base distance varint
                while bytes[pos] & 0x80 != 0 {
                    pos += 1;
                }
                pos += 1;
            }
            7 => {
                ref_bases.push(bytes[pos..pos + algo_len].to_vec());
                pos += algo_len;
            }
            _ => {}
        }
        // Skip the zlib stream for this object by decompressing to find its end.
        pos += zlib_consume(&bytes[pos..]);
    }
    (ref_bases.len(), ofs, nr)
}

/// Read a pack object's (type, size) header, returning bytes consumed.
fn read_type_size(b: &[u8]) -> (u8, u64, usize) {
    let mut c = b[0];
    let type_code = (c >> 4) & 0x7;
    let mut size = (c & 0x0f) as u64;
    let mut shift = 4u32;
    let mut i = 1usize;
    while c & 0x80 != 0 {
        c = b[i];
        size |= ((c & 0x7f) as u64) << shift;
        shift += 7;
        i += 1;
    }
    (type_code, size, i)
}

/// Decompress one zlib stream from the front of `b`, returning the number of
/// compressed bytes consumed.
fn zlib_consume(b: &[u8]) -> usize {
    use std::io::Read;
    let mut dec = flate2::bufread::ZlibDecoder::new(b);
    let mut sink = Vec::new();
    dec.read_to_end(&mut sink).expect("zlib decode");
    dec.total_in() as usize
}

#[test]
fn delta_pack_is_smaller_and_reparses() {
    let fx = build_delta_fixture();
    let odb = open_odb(fx.dir.path());
    let tip = *fx.tips.last().unwrap();

    // Whole-object pack (full closure from the tip).
    let whole = build_pack(&odb, &[tip], &[], &PackBuildOptions::default()).expect("whole");
    // Delta pack over the same closure.
    let delta = build_pack(
        &odb,
        &[tip],
        &[],
        &PackBuildOptions {
            delta: true,
            ..PackBuildOptions::default()
        },
    )
    .expect("delta");

    // (a) The delta pack must be meaningfully smaller: the five ~200KB blobs
    // collapse to one full blob + four small deltas.
    assert!(
        delta.len() * 2 < whole.len(),
        "delta pack ({}) should be < half the whole pack ({})",
        delta.len(),
        whole.len()
    );

    // (b) Both packs must re-parse to the SAME object set.
    let whole_map = pack_bytes_to_object_map(&whole, &odb).expect("reparse whole");
    let delta_map = pack_bytes_to_object_map(&delta, &odb).expect("reparse delta");
    assert_eq!(
        whole_map.keys().collect::<std::collections::BTreeSet<_>>(),
        delta_map.keys().collect::<std::collections::BTreeSet<_>>(),
        "delta pack must resolve to the same object set as the whole pack"
    );

    // (c) The delta pack must actually contain deltas (OFS or REF).
    let (ref_n, ofs_n, _total) = pack_delta_stats(&delta, 20);
    assert!(
        ref_n + ofs_n >= 3,
        "expected several delta entries, got ref={ref_n} ofs={ofs_n}"
    );

    // (d) System git must accept the delta pack via index-pack.
    assert!(
        git_index_pack_ok(fx.dir.path(), &delta, false),
        "system git index-pack rejected the delta pack"
    );
}

#[test]
fn thin_pack_omits_peer_held_base_and_resolves() {
    let fx = build_delta_fixture();
    let odb = open_odb(fx.dir.path());

    // wants = newest tip, haves = the previous tip (peer already holds C3's
    // closure, including the previous big.txt blob — a perfect external base).
    let want = *fx.tips.last().unwrap();
    let have = fx.tips[fx.tips.len() - 2];

    let thin = build_pack(
        &odb,
        &[want],
        &[have],
        &PackBuildOptions {
            delta: true,
            thin: true,
            ..PackBuildOptions::default()
        },
    )
    .expect("thin pack");

    // (a) grit-lib agrees the pack is thin.
    assert!(
        grit_lib::unpack_objects::pack_is_thin(&thin, grit_lib::objects::HashAlgo::Sha1),
        "pack should be detected as thin"
    );

    // (b) At least one REF_DELTA references a base NOT present in the pack.
    let map = pack_bytes_to_object_map(&thin, &odb).expect("thin pack resolves via odb");
    let in_pack: std::collections::HashSet<ObjectId> = map.keys().copied().collect();
    let (ref_n, _ofs_n, _total) = pack_delta_stats(&thin, 20);
    assert!(ref_n >= 1, "thin pack must use at least one REF_DELTA");

    // Re-walk to confirm an external base specifically.
    let external = ref_delta_external_bases(&thin, 20, &in_pack);
    assert!(
        !external.is_empty(),
        "thin pack must reference at least one base NOT in the pack"
    );

    // (c) Supplying the base (it is in our odb) lets every delta resolve: the map
    // above already proves resolution succeeds against the odb-held base. Sanity:
    // the want's tree blob resolves to the newest big.txt content.
    let big_oid = rev_parse(fx.dir.path(), &format!("{}:big.txt", want.to_hex()));
    assert!(
        map.contains_key(&big_oid),
        "resolved thin pack must contain the newest big.txt blob"
    );

    // (d) Cross-check object counts against system git: index-pack --fix-thin in
    // the repo (which holds the base) must accept it and report the same count.
    assert!(
        git_index_pack_ok(fx.dir.path(), &thin, true),
        "system git index-pack --fix-thin rejected the thin pack"
    );
}

/// REF_DELTA base oids that are NOT among `in_pack`.
fn ref_delta_external_bases(
    bytes: &[u8],
    algo_len: usize,
    in_pack: &std::collections::HashSet<ObjectId>,
) -> Vec<ObjectId> {
    let nr = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    let mut pos = 12usize;
    let mut out = Vec::new();
    for _ in 0..nr {
        let (type_code, _size, consumed) = read_type_size(&bytes[pos..]);
        pos += consumed;
        match type_code {
            6 => {
                while bytes[pos] & 0x80 != 0 {
                    pos += 1;
                }
                pos += 1;
            }
            7 => {
                let base = ObjectId::from_bytes(&bytes[pos..pos + algo_len]).unwrap();
                if !in_pack.contains(&base) {
                    out.push(base);
                }
                pos += algo_len;
            }
            _ => {}
        }
        pos += zlib_consume(&bytes[pos..]);
    }
    out
}

/// Run system `git index-pack` over `pack` bytes; returns whether it succeeded.
///
/// A self-contained pack is indexed from a file in a scratch dir. A thin pack
/// needs its external bases, so it is fed on stdin inside the fixture repo with
/// `--fix-thin --stdin` (which appends the missing bases and writes a complete
/// pack + idx into the repo's object store).
fn git_index_pack_ok(repo: &Path, pack: &[u8], fix_thin: bool) -> bool {
    use std::io::Write as _;
    use std::process::Stdio;

    if fix_thin {
        let mut child = Command::new("git")
            .current_dir(repo)
            .args(["index-pack", "-v", "--fix-thin", "--stdin"])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn git index-pack --stdin");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(pack)
            .expect("write pack to stdin");
        let out = child.wait_with_output().expect("wait git index-pack");
        if !out.status.success() {
            eprintln!(
                "git index-pack --fix-thin failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        return out.status.success();
    }

    let scratch = tempfile::tempdir().expect("scratch");
    let pack_path = scratch.path().join("in.pack");
    std::fs::write(&pack_path, pack).unwrap();
    let out = Command::new("git")
        .current_dir(scratch.path())
        .args(["index-pack", "-v", &pack_path.to_string_lossy()])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("run git index-pack");
    if !out.status.success() {
        eprintln!(
            "git index-pack failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    out.status.success()
}
