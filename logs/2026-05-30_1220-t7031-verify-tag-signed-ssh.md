# t7031-verify-tag-signed-ssh: 4/14 -> 14/14

Date: 2026-05-30
Branch: wf/p3/t7031-verify-tag-signed-ssh
Base: 8c7fff01624aecfa18489ec68621dbafed8c08d9

## Problem
Annotated-tag signing + `verify-tag` for the ssh format was broken end to end:
- `grit tag` had no `-u`/`--local-user` flag (clap rejected the setup subtests).
- Signed tags carried a FAKE `GRITTAGSIGV1 ...` blob, never a real
  `ssh-keygen -Y sign` signature, so they could never verify.
- `grit verify-tag` was a stub: it only read `refs/tags/<name>`, printed the
  body, and always returned Ok. No real verification, no `--raw`, no `--format`,
  no raw-OID support.
- `git rev-list --format=%G?` emitted the literal `%G?` (the rev-list format
  expander had no signature placeholder support).

## Changes
- grit/src/commands/tag.rs
  - Added `-u`/`--local-user <KEY-ID>` (implies `-s`, per git/builtin/tag.c:507).
  - Replaced the fake pseudo-signature (and deleted
    `pseudo_tag_signature_payload`/`render_tag_signature_block`) with a real
    `signing::sign_buffer()` call; the armored signature is appended directly
    after the serialized tag body (no `gpgsig` header, no indentation —
    git/builtin/tag.c:191). Signing key resolved via
    `cfg.resolve_signing_key(local_user, committer_default)`.
- grit-lib/src/signing.rs (added new pub fns only, to avoid conflicts)
  - `parse_signed_buffer(buf) -> Option<(payload, signature)>`: port of
    gpg-interface.c `parse_signed_buffer`/`parse_signature` (last armor line).
  - `verify_tag(cfg, raw_tag)`: mirrors `verify_commit` but uses the appended-
    signature splitter; reuses `verify_ssh_signed_buffer`/`parse_gpg_output`.
  - Two unit tests for `parse_signed_buffer`.
- grit/src/commands/verify_tag.rs (rewritten on the verify-commit template)
  - Resolve each arg via `rev_parse::resolve_revision` (raw OIDs now work),
    require a Tag object, run `signing::verify_tag`, emit `--raw`/human output
    to stderr, verbose payload to stdout, and `--format` with a minimal
    `%(tag)` expander printed only on successful verify (forged tag -> silent).
- grit-lib/src/rev_list.rs
  - Added `%G?`/`%GS`/`%GK`/`%GF`/`%GP`/`%GT`/`%GG` support to
    `render_commit_with_color`, computing the signature lazily only when the
    format contains `%G` (mirrors log.rs).

## Results
- t7031-verify-tag-signed-ssh: 14/14 (was 4/14).
- Regression guards (run directly; same counts new vs base binary):
  - t7030-verify-tag 16/16, t7510-signed-commit 28/28 (0 fail),
    t7528-signed-commit-ssh 26 pass (0 fail).
  - t7004-tag 169/231 (identical to baseline — no regression).
  - t4202-log 68/149, t6300-for-each-ref 347/429, t6113 13/14 — all identical
    to baseline (no regression from the %G? change).
- cargo test -p grit-lib --lib: 221 passed, 0 failed.
- cargo fmt clean; no new clippy warnings on changed lines.
