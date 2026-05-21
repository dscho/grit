# Phase 2 promisor fetch and blame

## Scope

- Continued `t5616-partial-clone.sh` after the clone/fetch filter work.
- Focused on dynamic object fetch paths and filtered upload-pack refetch behavior.

## Changes

- Added promisor lazy-fetch fallback to `blame` object reads so missing historical blobs hydrate on demand.
- Added `fetch-pack --stdin` support for local/file remotes, copying requested object IDs into the local ODB.
- Extended `pack-objects --filter` handling from `blob:none` to `blob:limit=<n>` and combined blob filters.
- Made upload-pack `--refetch` fetches still want already-tracked branch tips and suppress local `have` negotiation so filtered packs are resent.
- Avoided pruning previously materialized local objects for filtered local fetch bookkeeping.

## Verification

- `cargo build --release -p grit-cli`: pass with existing warnings.
- Focused `t5616-partial-clone.sh --run=1-10`: pass.
- Focused `t5616-partial-clone.sh --run=1-18`: tests 1-16 pass; remaining failures start at shallow partial clone and trace2 maintenance checks.
- `./scripts/run-tests.sh t5616-partial-clone.sh`: 24/47.
