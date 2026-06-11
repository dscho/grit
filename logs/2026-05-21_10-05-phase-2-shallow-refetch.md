# Phase 2 shallow partial refetch

Task: advance Phase 2 partial clone support around shallow clones and filtered refetch.

Changes made:

- Reproduced `t5616-partial-clone.sh --run=1-17`; initial failure was the depth-1 partial clone reporting 29 missing lines instead of 6.
- Found `.git/shallow` was correct, but `grit-promisor-missing` was seeded by walking full source history before the shallow boundary existed.
- Wrote the shallow boundary before partial-clone marker initialization for normal depth clones, and made clone marker walks stop at shallow boundary commits while still walking the boundary commit tree.
- Applied the same shallow-boundary rule to filtered local fetch object walks.
- Fixed filtered fetch marker maintenance so hydrated/promisor-pack-present objects are trimmed from `grit-promisor-missing`.

Validation:

- `cargo build --release -p grit-cli` passes with existing warnings.
- Focused `t5616-partial-clone.sh --run=1-17` passes.
- `./scripts/run-tests.sh t5616-partial-clone.sh` reports 26/47.
