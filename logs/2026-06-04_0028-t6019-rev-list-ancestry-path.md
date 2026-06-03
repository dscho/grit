# t6019 rev-list ancestry path

Claimed `t6019-rev-list-ancestry-path.sh` from `t6-plan.md` at 5/18 passing.

Initial focus:

- Re-run the current harness and direct verbose test to identify the first failing ancestry-path
  behavior.
- Read the upstream test and rev-list documentation for `--ancestry-path` semantics.
- Search existing revision traversal code before deciding whether the fix belongs in pruning,
  parent rewriting, or CLI option parsing.

First increment:

- `git log` now accepts `--ancestry-path=<rev>` and repeated explicit ancestry-path pivots, matching
  the rev-list option shape.
- The CLI normalizes these values into existing `RevListOptions::ancestry_path_bottoms` so the
  library traversal API remains typed.

Validation:

- Direct debug run with `PATH=/Users/schacon/grit-t6/target/debug:$PATH bash
  ./t6019-rev-list-ancestry-path.sh -i -v` advances through test 11; the next failure is the
  symmetric-diff ancestry case `--ancestry-path F...I`, where `G` is still included.
- Official harness: `cargo build --release -p grit-cli && ./scripts/run-tests.sh
  t6019-rev-list-ancestry-path.sh` improves from 5/18 to 12/18 and refreshes
  `data/test-files.csv` plus dashboards.
- Standard checks: `cargo fmt`, `cargo check -p grit-cli`, `cargo build -p grit-cli`,
  `cargo build --release -p grit-cli`, `cargo clippy --fix --allow-dirty`, and
  `cargo test -p grit-lib --lib`; existing warning backlog remains and unrelated auto-fixes were
  reverted.

Final increment:

- Changed ancestry filtering to mark descendants only through commits selected by the original
  range, while still using excluded bottom commits as propagation seeds. This drops commits like
  `G` from `--ancestry-path F...I` instead of walking through excluded merge bases.
- Passed ancestry-path options through the pathless symmetric-log shortcut so `git log F...I`
  shares the corrected rev-list behavior.
- Fixed path-limited ancestry simplification so merges TREESAME to the ancestry-bottom side are
  pruned, while simplify-merges preserves merges that differ on the ancestry-bottom side.
- Allowed `checkout -b <name> <start> --` by ignoring the separator itself when validating branch
  creation positional counts; this unblocks the criss-cross setup at the end of t6019.

Validation:

- Direct debug run: `cd tests && ./t6019-rev-list-ancestry-path.sh -i -v` passes all 18 tests.
- Official harness: `./scripts/run-tests.sh t6019-rev-list-ancestry-path.sh` records 18/18 and
  refreshes `data/test-files.csv` plus dashboards.
- Adjacent official harness check: `./scripts/run-tests.sh t6019-rev-list-ancestry-path.sh
  t6012-rev-list-simplify.sh t6111-rev-list-treesame.sh` records 18/18, 42/42, and 65/65 after
  splitting ancestry-side TREESAME handling for ordinary output vs parent rewriting.
