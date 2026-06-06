# t6013 rev-list reverse parents

Claimed `t6013-rev-list-reverse-parents.sh` from `t6-plan.md` at 2/3 passing.

Initial focus:

- Run the focused harness to identify the remaining reverse/parents/boundary failure.
- Compare local and upstream tests and inspect `rev-list` reverse ordering with boundary commits.
- Search existing parent rewriting and boundary rendering behavior before changing code.

Findings:

- The remaining failure was only the `--boundary` case.
- The non-reverse command prints normal commits followed by boundary commits.
- The `--reverse --boundary` command must match reversing that whole stream, so the boundary
  line belongs before the reversed commit stream.

Implementation:

- Factored CLI boundary output in `grit/src/commands/rev_list.rs`.
- For non-quiet `--reverse --boundary`, emit boundary commits before normal commit/object output.
- Preserved the previous `--quiet --boundary` placement to keep this fix focused.

Validation:

- `cargo build --release -p grit-cli` completed with the existing warning backlog.
- `./scripts/run-tests.sh t6013-rev-list-reverse-parents.sh --verbose` passes 3/3.
- Regression harness `./scripts/run-tests.sh t6013-rev-list-reverse-parents.sh t6138-rev-list-boundary.sh t6001-rev-list-graft.sh t6101-rev-parse-parents.sh t6011-rev-list-with-bad-commit.sh t6012-rev-list-simplify.sh --verbose --timeout 180`
  passes 3/3, 29/29, 14/14, 38/38, 6/6, and 42/42.
- `cargo fmt`, `cargo check -p grit-cli`, `cargo test -p grit-lib --lib`, and
  `cargo clippy --fix --allow-dirty` completed with the existing warning backlog.
- Restored unrelated clippy auto-fixes before commit.
