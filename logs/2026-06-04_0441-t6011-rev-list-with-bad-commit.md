# t6011 rev-list with bad commit

Claimed `t6011-rev-list-with-bad-commit.sh` from `t6-plan.md` at 5/6 passing.

Initial focus:

- Run the focused harness to identify the remaining bad-object detection failure.
- Compare local and upstream tests and inspect pack/object validation behavior.
- Search existing `rev-list`, `fsck`, and `repack` handling before changing code.

Findings:

- Local and upstream `t6011-rev-list-with-bad-commit.sh` are identical.
- The plan had stale 5/6 data; the current rebuilt release reported 3/6 because the corrupted
  packed commit was not rejected by fsck/rev-list/repack paths.
- Packed object lookup decompressed entries without checking that the canonical object bytes hashed
  back to the OID named by the pack index.
- `Odb::read` also masked corrupt pack read errors while searching alternates, turning them into
  `ObjectNotFound`.
- `fsck --full` collected packed IDs from indexes but did not force pack object verification.

Changes:

- Verify packed object hashes after delta resolution in `read_object_from_pack_bytes`.
- Propagate corrupt pack read errors from `Odb::read` instead of falling through to alternates.
- Make `fsck` verify local pack/index pairs while collecting pack state, reporting corrupt packs
  as errors.

Validation:

- `cargo build --release -p grit-cli` completed with the existing warning backlog.
- Focus harness: `./scripts/run-tests.sh t6011-rev-list-with-bad-commit.sh --verbose` passes 6/6.
- Regression harness: `./scripts/run-tests.sh t6011-rev-list-with-bad-commit.sh t6022-rev-list-missing.sh t6010-merge-base.sh t6101-rev-parse-parents.sh t7700-repack.sh --verbose --timeout 180` passes t6011 6/6, t6022 40/40, t6010 12/12, and t6101 38/38. `t7700-repack.sh` remains at its pre-existing tracked baseline of 40/47.
- `cargo fmt`, `cargo check -p grit-cli`, `cargo test -p grit-lib --lib`, and
  `cargo clippy --fix --allow-dirty` completed with the existing warning backlog; unrelated
  fmt/clippy auto-fixes were restored.
