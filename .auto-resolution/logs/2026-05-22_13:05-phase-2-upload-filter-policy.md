# Phase 2 upload-pack filter policy

Task: advance `t5616-partial-clone.sh` upload-pack filter policy coverage.

Changes made:

- Added library validation for `uploadpackfilter.*.allow` and `uploadpackfilter.tree.maxDepth`.
- Wired validation into upload-pack protocol v0/v1, protocol v2 fetch, and local `file://` clone paths.
- Preserved Git-style errors for disabled filters and excessive tree depth filters.

Validation:

- `cargo build --release -p grit-cli` passes with existing warnings.
- Focused `t5616-partial-clone.sh --run=1-28` now passes tests 24-28.
- `./scripts/run-tests.sh t5616-partial-clone.sh` reports 32/47.
