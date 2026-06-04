# t6003 rev-list topo order

Claimed `t6003-rev-list-topo-order.sh` from `t6-plan.md` at 23/36 passing.

Initial focus:

- Re-run the current harness to identify the first failing topo-order behavior.
- Read the upstream test and rev-list documentation for the exact ordering semantics.
- Search existing revision traversal code before deciding where to fix the behavior.

Implementation:

- Replaced plain `--topo-order`'s date min-heap with a Git-style graph-order topo stack: initial
  tips are ordered by committer date like the revision pending list, while newly ready parents are
  pushed in parent order and popped LIFO.
- Left author-date topo ordering on the existing priority path.
- Added `--max-age` / `--min-age` option forms and made bare numeric date cutoffs parse as raw Unix
  timestamps before fuzzy date parsing.

Validation:

- Direct debug run with `PATH=/Users/schacon/grit-t6/target/debug:$PATH bash
  ./t6003-rev-list-topo-order.sh -i -v` passes 36/36.
- Official harness: `cargo build --release -p grit-cli && ./scripts/run-tests.sh
  t6003-rev-list-topo-order.sh` records 36/36 and refreshes `data/test-files.csv` plus dashboards.
- Nearby regression check: `./scripts/run-tests.sh t6012-rev-list-simplify.sh` remains 42/42.
- Standard checks: `cargo fmt`, `cargo check -p grit-cli`, `cargo build -p grit-cli`,
  `cargo build --release -p grit-cli`, `cargo clippy --fix --allow-dirty`, and
  `cargo test -p grit-lib --lib`; existing warning backlog remains and unrelated auto-fixes were
  reverted.
