//! Wire-protocol push orchestration over a [`crate::transport::Connection`].
//!
//! [`push_remote`] is the wire counterpart to [`crate::transfer::push_local`]:
//! instead of copying objects between two on-disk repositories, it drives a
//! `git-receive-pack` exchange over a live [`crate::transport::Connection`] —
//! reading the receive-pack advertisement (remote refs + `.have` lines +
//! capabilities), deciding each ref update against the advertised remote refs
//! (reusing the same fast-forward / force / force-with-lease rules as
//! `push_local`), building the minimal pack with [`crate::transfer::build_pack`]
//! (using the advertised remote tips + `.have`s as the negotiation `haves`),
//! streaming it, and parsing the `report-status` / `report-status-v2` reply into
//! per-ref [`crate::push_report::PushRefResult`]s.
//!
//! This is the send-pack flow lifted from the CLI's `commands/send_pack.rs`
//! (`run`, `report_has_rejections`, `demux_report_and_remote_messages`),
//! generalized to run over the [`crate::transport::Connection`] reader/writer
//! rather than a spawned `receive-pack` subprocess.
//!
//! Protocol v0/v1 only in this phase (the classic receive-pack advertisement).
//! A protocol-v2 push would require the `command=push` round and is deferred.
//!
//! The wire OID width is the repository's hash algorithm (threaded through
//! [`crate::odb::Odb::hash_algo`]), so SHA-256 repositories push correctly: the
//! zero/null OID, the empty-pack trailer, and the advertisement parsing are all
//! hash-width aware.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;

use crate::error::{Error, Result};
use crate::fetch::Progress;
use crate::objects::{parse_tag, HashAlgo, ObjectId, ObjectKind};
use crate::pkt_line::{self, Packet};
use crate::push_report::{PushRefResult, PushRefStatus};
use crate::transfer::{
    build_pack, open_odb, PackBuildOptions, PushOptions, PushOutcome, PushRefSpec,
};
use crate::transport::Connection;

/// The receive-pack capabilities we negotiate, in the order Git's `send-pack`
/// lists them. `report-status-v2` is requested alongside `report-status` so a
/// modern server can reply with the richer per-ref report; `side-band-64k` lets
/// the server multiplex the report (band 1) and hook/diagnostic output (band 2).
const PUSH_CAPS_BASE: &str = "report-status report-status-v2 quiet";

/// Push refs to a remote over a live [`Connection`] speaking `git-receive-pack`.
///
/// The flow mirrors [`crate::transfer::push_local`], but the remote ref list and
/// `.have` hints come from the connection's advertisement, the objects are
/// streamed over the wire as a single pack, and per-ref acceptance/rejection is
/// learned from the server's `report-status` reply (a server may reject an update
/// our local checks would have accepted, e.g. `denyNonFastForwards` or a
/// pre-receive hook).
///
/// Steps:
/// 1. Read the receive-pack advertisement from `conn`: remote refs (name -> oid),
///    `.have` oids, and capabilities (`report-status(-v2)`, `side-band-64k`,
///    `ofs-delta`, `object-format`).
/// 2. Decide each [`PushRefSpec`] against the advertised remote refs (up-to-date,
///    new, fast-forward, forced, non-fast-forward rejection, force-with-lease
///    stale) — the client-side gate before anything is sent.
/// 3. Write the ref-update commands for the accepted, value-changing updates
///    (`<old> <new> <ref>\0<caps>\n` first, `<old> <new> <ref>\n` rest), then a
///    flush.
/// 4. Build the minimal pack with [`build_pack`] (wants = new tips, haves =
///    advertised remote tips + `.have`s) and stream it; for deletion-only pushes
///    stream the empty pack.
/// 5. Read + parse `report-status` / `report-status-v2` (demultiplexing the
///    side-band if negotiated) and fold the per-ref `ok`/`ng` lines back into the
///    decided results.
///
/// `progress` receives the remote's side-band channel-2 bytes (hook output,
/// `remote: …` diagnostics) when `side-band-64k` is negotiated.
///
/// Protocol v0/v1 only; a v2 connection is rejected.
///
/// # Errors
///
/// Returns an error if the connection is protocol v2, if a source object is
/// missing from the local odb, if the pack build fails, or on wire/parse I/O
/// failure.
pub fn push_remote(
    local_git_dir: &Path,
    conn: &mut dyn Connection,
    refs: &[PushRefSpec],
    opts: &PushOptions,
    progress: &mut dyn Progress,
) -> Result<PushOutcome> {
    if conn.protocol_version() >= 2 {
        return Err(Error::Message(
            "push_remote: protocol v2 not supported in this phase (use v0/v1)".to_owned(),
        ));
    }

    let local_odb = open_odb(local_git_dir);
    let algo = local_odb.hash_algo();
    let local_repo = crate::repo::Repository::open(local_git_dir, None).ok();

    // 1. Advertisement: split the connection's parsed advertisement into the
    //    remote ref map and the `.have` hints. `read_advertisement` records the
    //    `.have` lines as refs literally named `.have` (one per line), so peel
    //    those out here; everything else is a real remote ref.
    let mut remote_refs: HashMap<String, ObjectId> = HashMap::new();
    let mut advertised_haves: Vec<ObjectId> = Vec::new();
    for (name, oid) in conn.advertised_refs() {
        if name == ".have" {
            advertised_haves.push(*oid);
        } else {
            remote_refs.insert(name.clone(), *oid);
        }
    }
    let caps: Vec<String> = conn.capabilities().to_vec();
    let server_sideband = caps
        .iter()
        .any(|c| c == "side-band-64k" || c == "side-band");
    // The server tells us whether it accepts OFS_DELTA bases; without it we must
    // restrict in-pack deltas to REF_DELTA. Thin packs are always advertised by
    // smart-protocol receive-pack, so (like Git's send-pack) we send thin.
    let server_ofs_delta = caps.iter().any(|c| c == "ofs-delta");

    // 2. Decide each ref update client-side against the advertised remote refs.
    let mut decisions: Vec<PushDecision> = Vec::with_capacity(refs.len());
    for spec in refs {
        decisions.push(decide_push_wire(
            spec,
            &local_odb,
            &remote_refs,
            local_repo.as_ref(),
        )?);
    }

    // Atomic: a single client-side rejection aborts the whole push without
    // sending anything; the otherwise-accepted updates become AtomicPushFailed.
    let any_rejected = decisions.iter().any(|d| d.result.status.is_error());
    if opts.atomic && any_rejected {
        for d in &mut decisions {
            if matches!(d.result.status, PushRefStatus::Ok) {
                d.result.status = PushRefStatus::AtomicPushFailed;
                d.send = false;
            }
        }
        return Ok(PushOutcome {
            results: decisions.into_iter().map(|d| d.result).collect(),
        });
    }

    // Updates we will actually request from the server.
    let to_send: Vec<usize> = decisions
        .iter()
        .enumerate()
        .filter_map(|(i, d)| if d.send { Some(i) } else { None })
        .collect();

    // Nothing to send (all up-to-date / client-rejected): no wire round needed.
    if to_send.is_empty() || opts.dry_run {
        return Ok(PushOutcome {
            results: decisions.into_iter().map(|d| d.result).collect(),
        });
    }

    // 3. Write the ref-update commands. The first command carries the negotiated
    //    capability list after a NUL; the rest are bare. The OID width is the
    //    repository's hash algorithm (zero/null OID for create/delete).
    let zero_hex = "0".repeat(algo.hex_len());
    let mut command_caps = PUSH_CAPS_BASE.to_owned();
    if server_sideband {
        command_caps.push_str(" side-band-64k");
    }
    command_caps.push_str(&format!(" object-format={}", algo.name()));

    let mut commands: Vec<u8> = Vec::new();
    let mut first = true;
    for &i in &to_send {
        let d = &decisions[i];
        let old_hex = d
            .result
            .old_oid
            .map(|o| o.to_hex())
            .unwrap_or_else(|| zero_hex.clone());
        let new_hex = d
            .result
            .new_oid
            .map(|o| o.to_hex())
            .unwrap_or_else(|| zero_hex.clone());
        let line = if first {
            first = false;
            format!("{old_hex} {new_hex} {}\0{command_caps}\n", d.result.remote_ref)
        } else {
            format!("{old_hex} {new_hex} {}\n", d.result.remote_ref)
        };
        pkt_line::write_line_to_vec(&mut commands, &line)?;
    }
    commands.extend_from_slice(b"0000");
    conn.writer().write_all(&commands)?;
    conn.writer().flush()?;

    // 4. Build and stream the pack. Wants are the new tips; haves are the
    //    advertised remote ref tips plus `.have` hints, so we only pack the
    //    objects the remote does not already have. A deletion-only push carries
    //    no new objects: stream the (hash-width) empty pack so the server reads a
    //    well-formed packfile.
    let wants: Vec<ObjectId> = to_send
        .iter()
        .filter_map(|&i| decisions[i].new_tip)
        .collect();

    if wants.is_empty() {
        conn.writer().write_all(&empty_pack_bytes(algo))?;
    } else {
        let mut haves: Vec<ObjectId> = remote_refs.values().copied().collect();
        haves.extend_from_slice(&advertised_haves);
        // Send a thin, delta-compressed pack: the haves are everything the remote
        // already advertised, so blob deltas may reference those peer-held bases
        // without re-sending them (thin), and OFS_DELTA is used only when the
        // server advertised the `ofs-delta` capability.
        let pack = build_pack(
            &local_odb,
            &wants,
            &haves,
            &PackBuildOptions {
                thin: true,
                delta: true,
                use_ofs_delta: server_ofs_delta,
                ..PackBuildOptions::default()
            },
        )?;
        conn.writer().write_all(&pack)?;
    }
    conn.writer().flush()?;

    // 5. Read the server's report. With side-band, band 1 carries the
    //    report-status pkt-lines and band 2/3 carry remote diagnostics; without
    //    it the raw stream is the report-status itself.
    let mut raw = Vec::new();
    conn.reader().read_to_end(&mut raw)?;
    let report = if server_sideband {
        demux_report_and_remote_messages(&raw, progress)?
    } else {
        raw
    };

    apply_report_status(&report, &mut decisions);

    Ok(PushOutcome {
        results: decisions.into_iter().map(|d| d.result).collect(),
    })
}

/// A client-side push decision for one ref, plus what to send over the wire.
struct PushDecision {
    result: PushRefResult,
    /// The new tip object to pack (None for deletions / no-ops).
    new_tip: Option<ObjectId>,
    /// Whether to send a ref-update command for this ref to the server.
    send: bool,
}

/// Decide one [`PushRefSpec`] against the advertised remote refs, without any
/// I/O to the remote. Mirrors [`crate::transfer`]'s `decide_push`, but the
/// "remote current" value comes from the advertisement map rather than an
/// on-disk remote ref.
fn decide_push_wire(
    spec: &PushRefSpec,
    local_odb: &crate::odb::Odb,
    remote_refs: &HashMap<String, ObjectId>,
    local_repo: Option<&crate::repo::Repository>,
) -> Result<PushDecision> {
    let remote_current = remote_refs.get(&spec.dst).copied();

    let no_op = |status: PushRefStatus,
                 old: Option<ObjectId>,
                 new: Option<ObjectId>,
                 deletion: bool,
                 message: Option<String>| {
        PushDecision {
            result: PushRefResult {
                local_ref: None,
                remote_ref: spec.dst.clone(),
                old_oid: old,
                new_oid: new,
                forced: false,
                deletion,
                status,
                message,
            },
            new_tip: None,
            send: false,
        }
    };

    // Up-to-date trumps every lease (creating/moving a ref to where it already
    // is succeeds, even when a force-with-lease expectation does not hold).
    if !spec.delete {
        if let Some(src) = spec.src {
            if remote_current == Some(src) {
                return Ok(no_op(
                    PushRefStatus::UpToDate,
                    remote_current,
                    Some(src),
                    false,
                    None,
                ));
            }
        }
    }

    // Absence lease: a destination that already exists fails the lease.
    if spec.expect_absent && remote_current.is_some() {
        return Ok(no_op(
            PushRefStatus::RejectStale,
            remote_current,
            spec.src,
            spec.delete,
            Some("stale info".to_owned()),
        ));
    }

    // Compare-and-swap (force-with-lease): the remote's current value must match.
    if let Some(expected) = spec.expected_old {
        if remote_current != Some(expected) {
            return Ok(no_op(
                PushRefStatus::RejectStale,
                remote_current,
                spec.src,
                spec.delete,
                Some("stale info".to_owned()),
            ));
        }
    }

    if spec.delete {
        // Deleting a ref the remote does not have is a no-op success; otherwise
        // send the delete command (null new OID) and let the server confirm.
        return Ok(match remote_current {
            Some(_) => PushDecision {
                result: PushRefResult {
                    local_ref: None,
                    remote_ref: spec.dst.clone(),
                    old_oid: remote_current,
                    new_oid: None,
                    forced: false,
                    deletion: true,
                    status: PushRefStatus::Ok,
                    message: None,
                },
                new_tip: None,
                send: true,
            },
            None => no_op(PushRefStatus::UpToDate, None, None, true, None),
        });
    }

    let Some(src) = spec.src else {
        return Err(Error::Message(format!(
            "push to '{}' has no source object and is not a deletion",
            spec.dst
        )));
    };
    if !local_odb.exists(&src) {
        return Err(Error::Message(format!(
            "source object {src} for '{}' is missing from the local object store",
            spec.dst
        )));
    }

    // New ref: nothing on the remote yet — always allowed.
    let Some(old) = remote_current else {
        return Ok(PushDecision {
            result: PushRefResult {
                local_ref: None,
                remote_ref: spec.dst.clone(),
                old_oid: None,
                new_oid: Some(src),
                forced: false,
                deletion: false,
                status: PushRefStatus::Ok,
                message: None,
            },
            new_tip: Some(src),
            send: true,
        });
    };

    // Existing ref: fast-forward when the remote's current commit is an ancestor
    // of the source; otherwise non-fast-forward (allowed only with force).
    let is_ff = local_repo
        .map(|r| crate::merge_base::is_ancestor(r, old, src).unwrap_or(false))
        .unwrap_or(false);

    if is_ff {
        Ok(PushDecision {
            result: PushRefResult {
                local_ref: None,
                remote_ref: spec.dst.clone(),
                old_oid: Some(old),
                new_oid: Some(src),
                forced: false,
                deletion: false,
                status: PushRefStatus::Ok,
                message: None,
            },
            new_tip: Some(src),
            send: true,
        })
    } else if spec.force {
        Ok(PushDecision {
            result: PushRefResult {
                local_ref: None,
                remote_ref: spec.dst.clone(),
                old_oid: Some(old),
                new_oid: Some(src),
                forced: true,
                deletion: false,
                status: PushRefStatus::Ok,
                message: None,
            },
            new_tip: Some(src),
            send: true,
        })
    } else {
        Ok(PushDecision {
            result: PushRefResult {
                local_ref: None,
                remote_ref: spec.dst.clone(),
                old_oid: Some(old),
                new_oid: Some(src),
                forced: false,
                deletion: false,
                status: PushRefStatus::RejectNonFastForward,
                message: Some("non-fast-forward".to_owned()),
            },
            new_tip: None,
            send: false,
        })
    }
}

/// Parse the server's `report-status` / `report-status-v2` stream and fold each
/// per-ref `ok`/`ng` line back into the matching decision.
///
/// The report is:
/// ```text
/// unpack ok\n            (or `unpack <error>\n`)
/// ok <ref>\n             (per accepted ref)
/// ng <ref> <reason>\n    (per rejected ref)
/// ```
/// An `ng` line demotes the decided result to [`PushRefStatus::RemoteRejected`]
/// with the server's reason; an `unpack` failure demotes every sent ref. Lifted
/// from the CLI's `report_has_rejections`, extended to capture the reason and the
/// `unpack` status.
fn apply_report_status(report: &[u8], decisions: &mut [PushDecision]) {
    let mut by_ref: HashMap<&str, usize> = HashMap::new();
    for (i, d) in decisions.iter().enumerate() {
        if d.send {
            by_ref.insert(d.result.remote_ref.as_str(), i);
        }
    }
    // Resolve indices up front to avoid borrow conflicts while mutating.
    let mut unpack_error: Option<String> = None;
    let mut updates: Vec<(usize, Option<String>)> = Vec::new();

    let mut cursor = Cursor::new(report);
    while let Ok(Some(pkt)) = pkt_line::read_packet(&mut cursor) {
        let Packet::Data(line) = pkt else {
            continue;
        };
        let line = line.trim_end();
        if let Some(rest) = line.strip_prefix("unpack ") {
            if rest.trim() != "ok" {
                unpack_error = Some(rest.trim().to_owned());
            }
        } else if let Some(refname) = line.strip_prefix("ok ") {
            // Accepted: keep the decided (Ok/UpToDate) status.
            let _ = by_ref.get(refname.trim());
        } else if let Some(rest) = line.strip_prefix("ng ") {
            // `ng <ref> <reason>`: the remote declined this update.
            let (refname, reason) = rest.split_once(' ').unwrap_or((rest, ""));
            if let Some(&idx) = by_ref.get(refname.trim()) {
                let msg = if reason.trim().is_empty() {
                    None
                } else {
                    Some(reason.trim().to_owned())
                };
                updates.push((idx, msg));
            }
        }
    }

    for (idx, msg) in updates {
        decisions[idx].result.status = PushRefStatus::RemoteRejected;
        decisions[idx].result.message = msg;
    }

    // A failed `unpack` rejects every ref we sent that the server did not
    // already mark as failed.
    if let Some(reason) = unpack_error {
        for d in decisions.iter_mut() {
            if d.send && !matches!(d.result.status, PushRefStatus::RemoteRejected) {
                d.result.status = PushRefStatus::RemoteRejected;
                d.result.message = Some(format!("unpack failed: {reason}"));
            }
        }
    }
}

/// Split a side-band stream: band 1 (report-status) is returned; band 2/3
/// (remote diagnostics) is forwarded to `progress`. Lifted from the CLI's
/// `demux_report_and_remote_messages`, but progress goes to the callback rather
/// than directly to stderr (the public API must not assume stdout/stderr).
fn demux_report_and_remote_messages(
    input: &[u8],
    progress: &mut dyn Progress,
) -> Result<Vec<u8>> {
    let mut report = Vec::new();
    let mut i = 0usize;
    while i + 4 <= input.len() {
        let len = match pkt_line::parse_hex_len(&input[i..i + 4]) {
            Ok(l) => l,
            Err(_) => break,
        };
        i += 4;
        if len == 0 {
            // Flush packet: a delimiter between report sections, keep scanning.
            continue;
        }
        if len < 4 || i + (len - 4) > input.len() {
            break;
        }
        let payload = &input[i..i + (len - 4)];
        i += len - 4;
        if payload.is_empty() {
            continue;
        }
        let band = payload[0];
        let data = &payload[1..];
        match band {
            1 => report.extend_from_slice(data),
            2 | 3 => progress.message(data),
            _ => {}
        }
    }
    Ok(report)
}

/// The bytes of an empty packfile (`PACK`, version 2, zero objects) with its
/// trailing checksum at the repository hash width.
///
/// `git send-pack` always streams a packfile after the ref-update commands, even
/// for deletion-only pushes; the receiving side reads the trailer to know the
/// pack ended.
fn empty_pack_bytes(algo: HashAlgo) -> Vec<u8> {
    let mut pack = Vec::with_capacity(44);
    pack.extend_from_slice(b"PACK");
    pack.extend_from_slice(&2u32.to_be_bytes());
    pack.extend_from_slice(&0u32.to_be_bytes());
    match algo {
        HashAlgo::Sha1 => {
            use sha1::{Digest, Sha1};
            let digest = Sha1::digest(&pack);
            pack.extend_from_slice(&digest);
        }
        HashAlgo::Sha256 => {
            use sha2::{Digest, Sha256};
            let digest = Sha256::digest(&pack);
            pack.extend_from_slice(&digest);
        }
    }
    pack
}

/// Peel `oid` to the commit it ultimately names, following annotated tags, using
/// the local odb. Returns `None` if it is not a commit (or is missing). Provided
/// for symmetry with the CLI's `peel_advertised_commits`; the wire `build_pack`
/// uses the advertised ref/`.have` oids directly as `haves`, so this is exposed
/// for callers that need commit tips.
#[allow(dead_code)]
fn peel_to_commit(odb: &crate::odb::Odb, oid: ObjectId) -> Option<ObjectId> {
    let mut current = oid;
    for _ in 0..16 {
        let obj = odb.read(&current).ok()?;
        match obj.kind {
            ObjectKind::Commit => return Some(current),
            ObjectKind::Tag => current = parse_tag(&obj.data).ok()?.object,
            _ => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_decision(refname: &str, send: bool) -> PushDecision {
        PushDecision {
            result: PushRefResult {
                local_ref: None,
                remote_ref: refname.to_owned(),
                old_oid: None,
                new_oid: None,
                forced: false,
                deletion: false,
                status: PushRefStatus::Ok,
                message: None,
            },
            new_tip: None,
            send,
        }
    }

    fn report_bytes(lines: &[&str]) -> Vec<u8> {
        let mut buf = Vec::new();
        for l in lines {
            pkt_line::write_line_to_vec(&mut buf, l).unwrap();
        }
        buf.extend_from_slice(b"0000");
        buf
    }

    #[test]
    fn empty_pack_has_valid_trailer_widths() {
        assert_eq!(empty_pack_bytes(HashAlgo::Sha1).len(), 12 + 20);
        assert_eq!(empty_pack_bytes(HashAlgo::Sha256).len(), 12 + 32);
        assert!(empty_pack_bytes(HashAlgo::Sha1).starts_with(b"PACK"));
    }

    #[test]
    fn report_ng_demotes_to_remote_rejected() {
        let mut decisions = vec![
            make_decision("refs/heads/main", true),
            make_decision("refs/heads/topic", true),
        ];
        let report = report_bytes(&[
            "unpack ok",
            "ok refs/heads/main",
            "ng refs/heads/topic non-fast-forward",
        ]);
        apply_report_status(&report, &mut decisions);
        assert_eq!(decisions[0].result.status, PushRefStatus::Ok);
        assert_eq!(decisions[1].result.status, PushRefStatus::RemoteRejected);
        assert_eq!(
            decisions[1].result.message.as_deref(),
            Some("non-fast-forward")
        );
    }

    #[test]
    fn report_unpack_failure_rejects_all_sent() {
        let mut decisions = vec![make_decision("refs/heads/main", true)];
        let report = report_bytes(&["unpack index-pack abort"]);
        apply_report_status(&report, &mut decisions);
        assert_eq!(decisions[0].result.status, PushRefStatus::RemoteRejected);
        assert!(decisions[0]
            .result
            .message
            .as_deref()
            .unwrap()
            .starts_with("unpack failed:"));
    }

    #[test]
    fn demux_separates_report_and_progress() {
        struct Cap(Vec<u8>);
        impl Progress for Cap {
            fn message(&mut self, bytes: &[u8]) {
                self.0.extend_from_slice(bytes);
            }
        }
        // Band 1 = report, band 2 = progress.
        let mut wire = Vec::new();
        let mut band1 = vec![1u8];
        band1.extend_from_slice(b"unpack ok\n");
        pkt_line::write_packet_raw(&mut wire, &band1).unwrap();
        let mut band2 = vec![2u8];
        band2.extend_from_slice(b"hello from hook\n");
        pkt_line::write_packet_raw(&mut wire, &band2).unwrap();
        wire.extend_from_slice(b"0000");

        let mut cap = Cap(Vec::new());
        let report = demux_report_and_remote_messages(&wire, &mut cap).unwrap();
        assert_eq!(report, b"unpack ok\n");
        assert_eq!(cap.0, b"hello from hook\n");
    }
}
