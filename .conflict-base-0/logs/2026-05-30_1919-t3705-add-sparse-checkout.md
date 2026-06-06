# t3705-add-sparse-checkout fix log

Date: 2026-05-30
Branch: wf/p5/t3705-add-sparse-checkout
Base: 52eaa7c05

## Result
t3705-add-sparse-checkout: 15/20 -> 20/20 (green). No regressions in the
audited sibling suites; several improved.

## Failing tests addressed
- 4  - `git add .` does not remove sparse entries
- 14 - do not advice about sparse entries when they do not match the pathspec
- 16 - git add fails outside of sparse-checkout definition
- 17 - add obeys advice.updateSparsePath (cascade of 16a)
- 20 - refuse to add non-skip-worktree file from sparse dir (cascade of 16a)

## Changes

### grit/src/commands/add.rs
1. Test 14: prefixed the two single-pathspec "did not match any files"
   terminal errors (unmerged-entry path + missing-file path) with `fatal: `
   so main.rs re-emits `fatal: pathspec ...` and exits 128 (matches git
   die()). The multi-pathspec arm still matches the substring "did not match
   any files" and keeps its `continue`, so no behavior change there.
2. Test 4: `pathspec_has_unblocked_target` now walks directory pathspecs
   (`.`, `:/`, `dir`) the way git `fill_directory` does. Added
   `dir_has_addable_target` (recursive) + `path_is_ignored` so that an
   untracked *ignored* file is NOT counted as an addable target. Threaded an
   `IgnoreMatcher` + `repo` into `pathspec_has_unblocked_target` /
   `pathspec_only_matches_sparse_blocked` at all four call sites (two in the
   dispatcher; `run_renormalize` and `update_tracked` build a local matcher).
   Also fixed `pathspec_matches_index_path` so `.` resolved to the literal
   "." at the repo root matches the whole tree (`resolve_pathspec` keeps "."
   rather than ""), which lets the skip-worktree `sparse_entry` register the
   `.` advice and exit 1.

### grit/src/commands/sparse_checkout.rs
3. Tests 16a/17/20: rewrote `remove_untracked_outside_sparse` to mirror git
   `clean_tracked_sparse_directories`:
   - non-cone mode cleans nothing (early return, like git's `!use_cone_patterns`);
   - never deletes loose untracked/ignored files;
   - only removes whole *tracked* directory subtrees that are out of cone and
     contain no untracked files (warns "contains untracked files" otherwise).
   This stops `git sparse-checkout set` (no-cone) from deleting the test's
   untracked helper files (sparse_error_header, sparse_hint), which was the
   real cause of 16a and the 17/20 cascades.

### grit/src/commands/reset.rs
4. Test 16b: gated the post-index-rebuild
   `reapply_sparse_checkout_if_configured(repo)` call on
   `needs_worktree_checkout`. MIXED/SOFT resets no longer re-apply sparsity
   (git/builtin/reset.c never does; `preserve_index_cache_flags_from` already
   carries skip-worktree bits forward), so a mixed `git reset` stops
   spuriously deleting the worktree file of a skip-worktree entry. The
   worktree-checkout modes (--hard/--merge/--keep) still re-apply, because
   their checkout materializes the index without honoring skip-worktree
   (needed by t1091 "cone mode: match patterns" / t1091 #57).

## Notes / dead-end explored
An ignore-aware redesign of the cone-mode directory cleanup (counting only
non-ignored untracked files, computing removable "sparse directories" from
the index skip-worktree state) was prototyped to also fix t1091 #51 "cone
mode clears ignored subdirectories". In the *real* suite #51 still fails for
an unrelated reason (`deep/deeper1/deepest/a` reported deleted in status),
and the redesign netted -1 on t1091 (a flaky shared-state test regressed),
so it was dropped in favor of the simpler, git-faithful version above.

## Regression audit (release binary)
- t3705-add-sparse-checkout: 15 -> 20  (target, green)
- t1091-sparse-checkout-builtin: 55 -> 56  (#57 now passes via reset gating)
- t7012-skip-worktree-writing: 10 -> 11
- t3602-rm-sparse-checkout: 7 -> 13
- t7817-grep-sparse-checkout: 4 -> 8
- t6435-merge-sparse: 6 -> 6
- t6435-merge-sparse-directory: 1 -> 1
- t7102-reset: 36 -> 36
- t7104-reset-hard: 3 -> 3
- t7106-reset-unborn-branch: 5 -> 5
- t7011-skip-worktree-reading: 15 -> 15
- t3407-rebase-abort: 13 -> 15
- t3501-revert-cherry-pick: 16 -> 20
- t3700-add: 48 -> 49
- t3702-add-edit: 2 -> 2
- t3704-add-pathspec-file: 11 -> 11
- t0008-ignores: 397 -> 398

## Quality gates
- cargo fmt: clean
- cargo test -p grit-lib --lib: 228 passed, 0 failed
- cargo clippy: grit-lib fails under strict deny(unwrap) lints (pre-existing,
  not touched by this change; all edits are in grit-cli and produced no
  clippy diagnostics on changed lines).
