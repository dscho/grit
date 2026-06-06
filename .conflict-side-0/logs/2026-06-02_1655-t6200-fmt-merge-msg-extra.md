## t6200-fmt-merge-msg-extra

Goal: make `tests/t6200-fmt-merge-msg-extra.sh` fully pass as the next tracking/refs/ref-format t6 item.

Initial CSV state: 23 tests, 22 passed, 1 failing.

Worktree: `/private/tmp/grit-t6-family` on branch `wf/t6-family`.

Notes:
- Starting after `t6300-for-each-ref.sh` reached 429/429.
- Harness failure was setup-only: `run-tests.sh` exports
  `GIT_TEST_DEFAULT_INITIAL_BRANCH_NAME=main`, while the synthetic fixture later checked out
  `master`.
- Patched the fixture to use `grit init --initial-branch=master repo`.
- Verification: `./scripts/run-tests.sh t6200-fmt-merge-msg-extra.sh --verbose` passes 23/23 and
  refreshed `data/test-files.csv` plus dashboards.
