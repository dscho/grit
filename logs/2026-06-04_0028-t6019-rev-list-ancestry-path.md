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
