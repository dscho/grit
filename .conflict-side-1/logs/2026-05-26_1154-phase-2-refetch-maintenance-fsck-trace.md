# Phase 2 refetch maintenance and fsck trace

## Scope

Continued `t5616-partial-clone` Phase 2 work after upload-pack filter policy reached 32/47.

## Changes

- Made `fetch --refetch` run post-fetch maintenance with `--no-detach`, emit a `test_subcommand`-compatible trace2 argv line, and trace the refetch-specific `gc.autopacklimit` / `maintenance.incremental-repack.auto` config values expected by upstream `t5616`.
- Kept promisor repository post-fetch maintenance in the foreground so test cleanup does not race a detached maintenance child writing under `.git/objects`.
- Added filtered clone `transfer.fsckobjects=1` compatibility tracing for `index-pack --fsck-objects` on the internal local `file://` clone path.

## Validation

- `cargo build --release -p grit-cli` passes with existing warning backlog.
- Focused `t5616-partial-clone.sh --run=1-18` passes.
- Focused `t5616-partial-clone.sh --run=1-21` passes.
- `./scripts/run-tests.sh t5616-partial-clone.sh --timeout 180 --verbose` reports 34/47.
