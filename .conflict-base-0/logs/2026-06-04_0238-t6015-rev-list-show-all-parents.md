# t6015 rev-list show all parents

Claimed `t6015-rev-list-show-all-parents.sh` from `t6-plan.md` at 32/38 passing.

Initial focus:

- Run the focused harness to identify the remaining parent-output failures.
- Read the local and upstream tests plus revision documentation for `--parents`, `--children`,
  `--full-history`, and `--show-all`.
- Search existing rev-list parent rewriting/output before changing traversal behavior.

Findings:

- There is no upstream `git/t/t6015-rev-list-show-all-parents.sh`; this is a synthetic local
  fixture.
- The first failing subtest checked out `master` after the harness-created repository started on
  `main`.
- Running the fixture directly with `GIT_TEST_DEFAULT_INITIAL_BRANCH_NAME=master` passed all 38
  subtests, so the failures were setup fallout rather than rev-list parent-output behavior.

Changes:

- Exported `GIT_TEST_DEFAULT_INITIAL_BRANCH_NAME=master` before sourcing `test-lib.sh`, matching
  the branch name hard-coded by this synthetic fixture.

Validation:

- `./scripts/run-tests.sh t6015-rev-list-show-all-parents.sh --verbose` passes 38/38.
- `cargo fmt` completed; unrelated fmt churn was restored.
- `cargo check -p grit-cli` completed with the existing warning backlog.
- `cargo test -p grit-lib --lib` passed 238/238.
- `cargo clippy --fix --allow-dirty` completed with the existing warning backlog; unrelated
  clippy auto-fixes were restored.
