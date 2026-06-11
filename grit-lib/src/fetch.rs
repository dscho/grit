//! Wire-protocol fetch orchestration over a [`crate::transport::Connection`].
//!
//! [`fetch_remote`] is the wire counterpart to [`crate::transfer::fetch_local`]:
//! instead of copying objects between two on-disk repositories, it drives a
//! `git-upload-pack` negotiation over a live [`crate::transport::Connection`] —
//! resolving wanted oids from the connection's advertised refs (via the same
//! refspec matching `fetch_local` uses), running the
//! [`crate::fetch_negotiator::SkippingNegotiator`] `want`/`have`/`done`
//! exchange, demultiplexing the side-band pack, ingesting it with
//! [`crate::unpack_objects`], and classifying ref updates into the shared
//! [`crate::transfer::FetchOutcome`].
//!
//! This is the protocol-v0/v1 negotiation loop lifted from the CLI's
//! `fetch_transport::fetch_upload_pack_negotiate_pack_bytes_with_streams`,
//! generalized to run over the [`crate::transport::Connection`] reader/writer
//! rather than subprocess pipes. Protocol v2 is **deferred** to a later pass
//! (see the module-level note in `transport.rs`); a v2 connection advertises no
//! refs and would require an `ls-refs` + `command=fetch` round here.

use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

use crate::error::{Error, Result};
use crate::fetch_negotiator::SkippingNegotiator;
use crate::objects::ObjectId;
use crate::pkt_line;
use crate::refspec::{parse_fetch_refspec, RefspecItem};
use crate::transfer::{
    classify_update, match_positive, open_odb, prune_tracking_refs, ref_excluded, refspecs_force,
    FetchOptions, FetchOutcome, RefUpdate, UpdateMode,
};
use crate::transport::Connection;

/// Sink for the remote's human-readable progress (side-band channel 2).
///
/// Implementations receive the raw progress bytes the server writes (typically
/// `\r`-delimited counter lines). The default does nothing.
pub trait Progress {
    /// Receive a chunk of progress bytes from side-band channel 2.
    fn message(&mut self, _bytes: &[u8]) {}
}

/// A [`Progress`] that discards everything.
pub struct NoProgress;

impl Progress for NoProgress {}

// --- Negotiation flush schedule (mirrors fetch-pack.c) --------------------

const INITIAL_FLUSH: usize = 16;
const PIPESAFE_FLUSH: usize = 32;

fn next_flush_count(count: usize) -> usize {
    if count < PIPESAFE_FLUSH {
        count * 2
    } else {
        count + PIPESAFE_FLUSH
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AckKind {
    /// `ACK <oid>` with no status suffix (post-`done` or legacy).
    Bare,
    Common,
    Continue,
    Ready,
}

fn parse_ack(line: &str) -> Option<(ObjectId, AckKind)> {
    if line == "NAK" {
        return None;
    }
    let rest = line.strip_prefix("ACK ")?;
    let hex = rest.split_whitespace().next()?;
    let oid = ObjectId::from_hex(hex).ok()?;
    let tail = rest.strip_prefix(hex).unwrap_or("").trim();
    let kind = if tail.contains("continue") {
        AckKind::Continue
    } else if tail.contains("common") {
        AckKind::Common
    } else if tail.contains("ready") {
        AckKind::Ready
    } else {
        AckKind::Bare
    };
    Some((oid, kind))
}

/// Read one ACK round, feeding `common`/`continue`/`ready` acks to the
/// negotiator. Lifted from `read_ack_round_with_negotiator`.
fn read_ack_round(reader: &mut dyn Read, negotiator: &mut SkippingNegotiator) -> Result<()> {
    let mut reader = reader;
    loop {
        let Some(pkt) = pkt_line::read_packet(&mut reader)? else {
            break;
        };
        match pkt {
            pkt_line::Packet::Flush => break,
            pkt_line::Packet::Data(ln) => {
                let ln = ln.trim_end();
                if ln == "NAK" {
                    // `upload-pack` sends `NAK` as the last line of a round with no trailing
                    // flush; waiting for another packet would block forever.
                    break;
                }
                let Some((ack_oid, kind)) = parse_ack(ln) else {
                    break;
                };
                if kind == AckKind::Bare {
                    break;
                }
                let _ = negotiator.ack(ack_oid)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Read a raw pkt-line payload (length-prefixed), returning `None` on
/// flush/delim/response-end/EOF. Side-band readers stop at a flush.
fn read_pkt_payload_raw(r: &mut dyn Read) -> std::io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len_str = std::str::from_utf8(&len_buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let len = usize::from_str_radix(len_str, 16)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    match len {
        0..=2 => Ok(None),
        n if n <= 4 => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid pkt-line length: {n}"),
        )),
        n => {
            let payload_len = n - 4;
            let mut buf = vec![0u8; payload_len];
            r.read_exact(&mut buf)?;
            Ok(Some(buf))
        }
    }
}

/// Demultiplex the side-band-64k stream after `done`: collect channel-1 pack
/// bytes into `out` (scanning for the `PACK` magic, which may span chunk
/// boundaries), and forward channel-2 progress to `progress`. Channel 3 is a
/// fatal error. Lifted from `read_sideband_pack_until_done`.
fn read_sideband_pack(
    r: &mut dyn Read,
    out: &mut Vec<u8>,
    progress: &mut dyn Progress,
) -> Result<()> {
    let mut seen_pack = false;
    let mut pending: Vec<u8> = Vec::new();
    loop {
        let Some(payload) = read_pkt_payload_raw(r)? else {
            break;
        };
        if payload.is_empty() {
            continue;
        }
        match payload[0] {
            1 => {
                let data = &payload[1..];
                if seen_pack {
                    out.extend_from_slice(data);
                } else {
                    pending.extend_from_slice(data);
                    if let Some(pos) = pending.windows(4).position(|w| w == b"PACK") {
                        seen_pack = true;
                        out.extend_from_slice(&pending[pos..]);
                        pending.clear();
                    } else if pending.len() > 3 {
                        let keep_from = pending.len() - 3;
                        pending.drain(..keep_from);
                    }
                }
            }
            2 => progress.message(&payload[1..]),
            3 => {
                return Err(Error::Message(format!(
                    "remote error: {}",
                    String::from_utf8_lossy(&payload[1..]).trim_end()
                )));
            }
            _ => {
                // No side-band: raw pack bytes.
                if !seen_pack && payload.starts_with(b"PACK") {
                    seen_pack = true;
                    out.extend_from_slice(&payload);
                } else if seen_pack {
                    out.extend_from_slice(&payload);
                }
            }
        }
    }
    Ok(())
}

/// Peel `oid` to the commit usable as a negotiation tip; `None` if it is not a
/// commit (or is missing). Mirrors the CLI's `peel_commit_oid_for_negotiation`
/// but tolerates missing/non-commit objects by returning `None`.
fn peel_to_commit(repo: &crate::repo::Repository, oid: ObjectId) -> Option<ObjectId> {
    let mut current = oid;
    for _ in 0..16 {
        let obj = repo.odb.read(&current).ok()?;
        match obj.kind {
            crate::objects::ObjectKind::Commit => return Some(current),
            crate::objects::ObjectKind::Tag => {
                current = crate::objects::parse_tag(&obj.data).ok()?.object;
            }
            _ => return None,
        }
    }
    None
}

/// Negotiate with `git-upload-pack` over the connection and return the raw
/// packfile bytes for the requested `wants`.
///
/// Drives the [`SkippingNegotiator`] over the connection: sends `want` lines
/// (with v0/v1 capabilities) and the advertised refs as `known_common`, batches
/// local `have`s with flushes (reading interleaved ACK rounds), sends `done`,
/// consumes the final ACK/NAK, then demuxes the side-band pack.
fn negotiate_pack(
    local_git_dir: &Path,
    conn: &mut dyn Connection,
    wants: &[ObjectId],
    progress: &mut dyn Progress,
) -> Result<Vec<u8>> {
    let local_repo = crate::repo::Repository::open(local_git_dir, None)?;
    let want_set: HashSet<ObjectId> = wants.iter().copied().collect();

    let Some(first_want) = wants.first().copied() else {
        return Ok(Vec::new());
    };

    // Capability set matching `git fetch-pack`'s first `want` line for v0/v1.
    let caps = " multi_ack_detailed side-band-64k thin-pack no-progress include-tag ofs-delta agent=grit";

    // Capture the advertised refs before borrowing the writer (avoids aliasing
    // the connection's reader/writer with its accessors).
    let advertised: Vec<(String, ObjectId)> = conn.advertised_refs().to_vec();

    let mut req: Vec<u8> = Vec::new();
    let w0 = format!("want {}{}", first_want.to_hex(), caps);
    pkt_line::write_line_to_vec(&mut req, &w0)?;
    for w in wants.iter().skip(1) {
        pkt_line::write_line_to_vec(&mut req, &format!("want {}", w.to_hex()))?;
    }
    // Match `git fetch-pack`: with a single unique OID, repeat the bare want.
    // git-daemon expects this.
    if wants.len() == 1 {
        pkt_line::write_line_to_vec(&mut req, &format!("want {}", first_want.to_hex()))?;
    }
    req.extend_from_slice(b"0000");
    conn.writer().write_all(&req)?;
    conn.writer().flush()?;

    // Build the negotiator from local ref tips (heads, tags, HEAD), peeled to
    // commits, excluding the wants. Advertised tips we already have become
    // `known_common`.
    let mut negotiator = SkippingNegotiator::new(local_repo);
    let mut tips: Vec<ObjectId> = Vec::new();
    let mut seen_tip: HashSet<ObjectId> = HashSet::new();
    for prefix in ["refs/heads/", "refs/tags/"] {
        if let Ok(entries) = crate::refs::list_refs(local_git_dir, prefix) {
            for (_, oid) in entries {
                if let Some(c) = peel_to_commit(negotiator.repo(), oid) {
                    if !want_set.contains(&c) && seen_tip.insert(c) {
                        tips.push(c);
                    }
                }
            }
        }
    }
    if let Ok(h) = crate::refs::resolve_ref(local_git_dir, "HEAD") {
        if let Some(c) = peel_to_commit(negotiator.repo(), h) {
            if !want_set.contains(&c) && seen_tip.insert(c) {
                tips.push(c);
            }
        }
    }
    tips.sort_by_key(ObjectId::to_hex);
    for t in tips {
        negotiator.add_tip(t)?;
    }
    for (_, oid) in &advertised {
        if want_set.contains(oid) {
            continue;
        }
        if let Some(c) = peel_to_commit(negotiator.repo(), *oid) {
            negotiator.known_common(c)?;
        }
    }

    // Have/ACK exchange: batch haves, flush, read interleaved ACK rounds.
    let mut count: usize = 0;
    let mut flush_at: usize = INITIAL_FLUSH;
    let mut pending: Vec<u8> = Vec::new();
    let mut flushes: i32 = 0;
    while let Some(oid) = negotiator.next_have()? {
        pkt_line::write_line_to_vec(&mut pending, &format!("have {}", oid.to_hex()))?;
        count += 1;
        if flush_at <= count {
            pending.extend_from_slice(b"0000");
            conn.writer().write_all(&pending)?;
            conn.writer().flush()?;
            pending.clear();
            flush_at = next_flush_count(count);
            flushes += 1;
            // Keep one window ahead: skip reading ACKs after the first flush.
            if count == INITIAL_FLUSH {
                continue;
            }
            read_ack_round(conn.reader(), &mut negotiator)?;
            flushes -= 1;
        }
    }
    if !pending.is_empty() {
        pending.extend_from_slice(b"0000");
        conn.writer().write_all(&pending)?;
        conn.writer().flush()?;
        flushes += 1;
    }
    while flushes > 0 {
        read_ack_round(conn.reader(), &mut negotiator)?;
        flushes -= 1;
    }

    // Send `done` (single pkt-line, no trailing flush) and read the ACK/NAK.
    let mut tail = Vec::new();
    pkt_line::write_line_to_vec(&mut tail, "done")?;
    conn.writer().write_all(&tail)?;
    conn.writer().flush()?;

    match pkt_line::read_packet(&mut conn.reader())? {
        None => return Err(Error::Message("unexpected EOF after done".to_owned())),
        Some(pkt_line::Packet::Flush) => {
            return Err(Error::Message("unexpected flush after done".to_owned()))
        }
        Some(pkt_line::Packet::Data(ln)) => {
            let ln = ln.trim_end();
            if ln != "NAK" {
                if let Some((ack_oid, kind)) = parse_ack(ln) {
                    if kind != AckKind::Bare {
                        let _ = negotiator.ack(ack_oid)?;
                    }
                } else if let Some(msg) = ln.strip_prefix("ERR ") {
                    return Err(Error::Message(format!("remote error: {}", msg.trim_end())));
                }
            }
        }
        Some(_) => {}
    }

    let mut pack = Vec::new();
    read_sideband_pack(conn.reader(), &mut pack, progress)?;
    Ok(pack)
}

/// Fetch from a remote over a live [`Connection`], driving the upload-pack
/// negotiation and writing the resulting tracking-ref updates into
/// `local_git_dir`.
///
/// The flow mirrors [`crate::transfer::fetch_local`], but the remote ref list
/// comes from the connection's advertisement, the objects arrive over the wire
/// (negotiated pack -> [`crate::unpack_objects`]), and the local repo is opened
/// to classify ancestry. Reuses the refspec matching, tag-mode, prune, and
/// classification helpers from [`crate::transfer`].
///
/// Protocol v0/v1 only in this phase. A v2 connection (no advertised refs)
/// produces an empty outcome and is reported as unsupported.
///
/// # Errors
///
/// Returns an error if the connection is protocol v2, if a refspec is invalid,
/// if the negotiation or pack ingest fails, or on ref/odb I/O failure.
pub fn fetch_remote(
    local_git_dir: &Path,
    conn: &mut dyn Connection,
    opts: &FetchOptions,
    progress: &mut dyn Progress,
) -> Result<FetchOutcome> {
    if conn.protocol_version() >= 2 {
        return Err(Error::Message(
            "fetch_remote: protocol v2 not supported in this phase (use v0/v1)".to_owned(),
        ));
    }

    let local_odb = open_odb(local_git_dir);

    // 1. Remote refs + default branch from the advertisement.
    let default_branch = conn.head_symref().map(|t| {
        t.strip_prefix("refs/heads/")
            .unwrap_or(t)
            .to_owned()
    });
    let remote_refs: Vec<(String, ObjectId)> = conn
        .advertised_refs()
        .iter()
        .filter(|(n, _)| n != "HEAD" && !n.ends_with("^{}"))
        .cloned()
        .collect();

    // 2. Parse refspecs.
    let mut positive: Vec<RefspecItem> = Vec::new();
    let mut negatives: Vec<RefspecItem> = Vec::new();
    for spec in &opts.refspecs {
        let item = parse_fetch_refspec(spec)
            .map_err(|e| Error::Message(format!("invalid refspec '{spec}': {e}")))?;
        if item.negative {
            negatives.push(item);
        } else {
            positive.push(item);
        }
    }
    for spec in &opts.negative_refspecs {
        let item = parse_fetch_refspec(spec)
            .map_err(|e| Error::Message(format!("invalid negative refspec '{spec}': {e}")))?;
        negatives.push(item);
    }

    // 3. Match refs to refspecs (mirror transfer::fetch_local).
    let mut matched: Vec<crate::transfer::MatchedRef> = Vec::new();
    let mut matched_oids: HashSet<ObjectId> = HashSet::new();
    let mut seen_remote_ref: HashSet<String> = HashSet::new();
    for (name, oid) in &remote_refs {
        if ref_excluded(name, &negatives) {
            continue;
        }
        if let Some(local_ref) = match_positive(name, &positive) {
            if seen_remote_ref.insert(name.clone()) {
                matched_oids.insert(*oid);
                matched.push(crate::transfer::MatchedRef {
                    remote_ref: name.clone(),
                    local_ref,
                    oid: *oid,
                    force: refspecs_force(name, &positive),
                    is_tag: name.starts_with("refs/tags/"),
                });
            }
        }
    }

    // TagMode: add tags. Tag-following needs the closure of fetched objects,
    // which we cannot compute remotely; the wire `include-tag` capability makes
    // the server send tag objects with the pack, so we add advertised tags by
    // mode here and let classification proceed once the pack lands. For
    // `Following` we approximate using the advertised remote odb if present
    // (it is not, over the wire), so we add following tags whose oid is among
    // the matched set after the fact — handled below using the local odb.
    add_wire_tags(
        opts.tags,
        &remote_refs,
        &negatives,
        &mut matched,
        &mut matched_oids,
        &mut seen_remote_ref,
    );

    // 4. Wants = matched oids absent locally. Negotiate + ingest the pack.
    let wants: Vec<ObjectId> = matched_oids
        .iter()
        .copied()
        .filter(|oid| !local_odb.exists(oid))
        .collect();

    if !wants.is_empty() && !opts.dry_run {
        let pack = negotiate_pack(local_git_dir, conn, &wants, progress)?;
        if !pack.is_empty() {
            let mut cursor = std::io::Cursor::new(pack);
            crate::unpack_objects::unpack_objects(
                &mut cursor,
                &local_odb,
                &crate::unpack_objects::UnpackOptions {
                    quiet: true,
                    ..Default::default()
                },
            )?;
        }
    }

    // For TagMode::Following, prune tags whose target did not arrive in the
    // pack (now resolvable against the local odb, which holds the fetched
    // objects). All/None already handled; Following kept only when reachable.
    if opts.tags == crate::transfer::TagMode::Following {
        retain_following_tags(&local_odb, &mut matched, &matched_oids);
    }

    // 5. Classify + apply ref updates (ancestry via the now-populated local repo).
    let local_repo = if opts.dry_run {
        None
    } else {
        crate::repo::Repository::open(local_git_dir, None).ok()
    };

    let mut updates: Vec<RefUpdate> = Vec::new();

    if opts.prune {
        prune_tracking_refs(
            local_git_dir,
            &positive,
            &remote_refs,
            opts.dry_run,
            &mut updates,
        )?;
    }

    for m in &matched {
        let Some(local_ref) = &m.local_ref else {
            updates.push(RefUpdate {
                remote_ref: m.remote_ref.clone(),
                local_ref: None,
                old_oid: None,
                new_oid: Some(m.oid),
                mode: UpdateMode::NoChangeNeeded,
                note: Some("not stored (empty destination)".to_owned()),
            });
            continue;
        };

        let old = crate::refs::resolve_ref(local_git_dir, local_ref).ok();
        let mode = classify_update(old.as_ref(), &m.oid, m.force, m.is_tag, local_repo.as_ref());

        let write = matches!(
            mode,
            UpdateMode::New | UpdateMode::FastForward | UpdateMode::Forced
        );
        if write && !opts.dry_run {
            crate::refs::write_ref(local_git_dir, local_ref, &m.oid)?;
        }

        updates.push(RefUpdate {
            remote_ref: m.remote_ref.clone(),
            local_ref: Some(local_ref.clone()),
            old_oid: old,
            new_oid: Some(m.oid),
            mode,
            note: None,
        });
    }

    Ok(FetchOutcome {
        updates,
        default_branch,
    })
}

/// Add advertised tags to the matched set per [`crate::transfer::TagMode`].
///
/// Over the wire we cannot peel remote tags before the pack arrives, so:
/// * `All` adds every advertised tag.
/// * `Following` provisionally adds every advertised tag here; unreachable ones
///   are dropped by [`retain_following_tags`] after the pack is ingested.
/// * `None` adds nothing.
fn add_wire_tags(
    mode: crate::transfer::TagMode,
    remote_refs: &[(String, ObjectId)],
    negatives: &[RefspecItem],
    matched: &mut Vec<crate::transfer::MatchedRef>,
    matched_oids: &mut HashSet<ObjectId>,
    seen_remote_ref: &mut HashSet<String>,
) {
    if mode == crate::transfer::TagMode::None {
        return;
    }
    for (name, oid) in remote_refs {
        if !name.starts_with("refs/tags/") {
            continue;
        }
        if seen_remote_ref.contains(name) || ref_excluded(name, negatives) {
            continue;
        }
        seen_remote_ref.insert(name.clone());
        matched_oids.insert(*oid);
        matched.push(crate::transfer::MatchedRef {
            remote_ref: name.clone(),
            local_ref: Some(name.clone()),
            oid: *oid,
            force: false,
            is_tag: true,
        });
    }
}

/// Drop provisional `Following` tags whose object (or peeled target) did not
/// arrive in the fetched pack — i.e. is not reachable from the other matched,
/// non-tag refs we fetched. Matches `git fetch`'s default tag-following: a tag
/// is kept when it points into the fetched history.
fn retain_following_tags(
    local_odb: &crate::odb::Odb,
    matched: &mut Vec<crate::transfer::MatchedRef>,
    matched_oids: &HashSet<ObjectId>,
) {
    // Roots: every non-tag matched ref we fetched.
    let roots: Vec<ObjectId> = matched
        .iter()
        .filter(|m| !m.is_tag)
        .map(|m| m.oid)
        .collect();
    let closure = reachable_closure(local_odb, &roots);
    matched.retain(|m| {
        if !m.is_tag {
            return true;
        }
        let peeled = peel_tag_target(local_odb, m.oid);
        // Keep when the tag object itself or its peeled target is reachable from
        // the fetched heads, and we actually have the object locally.
        let have = local_odb.exists(&m.oid);
        have && (closure.contains(&m.oid)
            || closure.contains(&peeled)
            || matched_oids.contains(&peeled))
    });
}

/// Peel an (annotated) tag to its ultimate non-tag target using the local odb.
fn peel_tag_target(odb: &crate::odb::Odb, oid: ObjectId) -> ObjectId {
    let mut current = oid;
    for _ in 0..16 {
        let Ok(obj) = odb.read(&current) else {
            return current;
        };
        if obj.kind != crate::objects::ObjectKind::Tag {
            return current;
        }
        match crate::objects::parse_tag(&obj.data) {
            Ok(t) => current = t.object,
            Err(_) => return current,
        }
    }
    current
}

/// Compute the object closure reachable from `roots` (commits -> trees ->
/// blobs, peeling tags), using the local odb. Best-effort: descent stops at
/// missing objects.
fn reachable_closure(odb: &crate::odb::Odb, roots: &[ObjectId]) -> HashSet<ObjectId> {
    use crate::objects::{parse_commit, parse_tag, parse_tree, ObjectKind};

    let mut seen: HashSet<ObjectId> = HashSet::new();
    let mut stack: Vec<ObjectId> = roots.to_vec();
    while let Some(oid) = stack.pop() {
        if !seen.insert(oid) {
            continue;
        }
        let Ok(obj) = odb.read(&oid) else {
            continue;
        };
        match obj.kind {
            ObjectKind::Commit => {
                if let Ok(c) = parse_commit(&obj.data) {
                    stack.push(c.tree);
                    for p in c.parents {
                        stack.push(p);
                    }
                }
            }
            ObjectKind::Tree => {
                if let Ok(entries) = parse_tree(&obj.data) {
                    for e in entries {
                        stack.push(e.oid);
                    }
                }
            }
            ObjectKind::Tag => {
                if let Ok(t) = parse_tag(&obj.data) {
                    stack.push(t.object);
                }
            }
            ObjectKind::Blob => {}
        }
    }
    seen
}
