# t1092-sparse-checkout-compatibility — 47 → 65 / 106

Branch: `wf/p5/t1092-sparse-checkout-compatibility`
Base: `b1215d00a`

## Result
- beforePassing: 47/106
- afterPassing: 65/106 (+18)
- grit-lib lib unit tests: 225 passed, 0 failed
- cargo fmt: clean; clippy: no new warnings on changed lines
- Regression guard t6435-merge-sparse: 6/6 (unchanged)
- Sibling sparse tests unchanged vs baseline: t1091 (21 fail), t3705 (5 fail),
  t1090 (1 fail), t2104 (7/7), t2107 (10/10), t2006 (9/9), t1003 (3/3),
  t7007-show (18/18), t1506-rev-parse (30/30), t8003-blame (30/30).

## Fixes (each its own commit)
1. checkout.rs `warn_sparse_paths_already_present`: only warn when the OLD
   index entry existed AND was skip_worktree (git unpack-trees.c:579
   `was_skip_worktree` gate), not for newly-introduced in-cone paths. Fixed
   the spurious-warning cluster (6,8,18,19,41,47,73).
2. sparse_checkout.rs `apply_sparse_patterns`: stat-fill (ctime/mtime/dev/
   ino/size) after materializing an in-cone file so diff-files/status don't
   report spurious modifications (88,89).
3. grit-lib sparse_checkout `try_parse_with_warnings`: accept multi-segment
   cone parents like `!/deep/deeper1/*/` (slash is not glob-special) (10).
4. grit-lib index.rs collapse: `directory_in_cone` now appends a trailing
   slash so a top-level out-of-cone directory isn't mistaken for an
   always-in-cone top-level file; collapse fully-sparse dirs into a single
   placeholder; refuse to collapse a directory containing a submodule
   (gitlink), matching `convert_to_sparse_rec` (4,29,71-collapse; no
   regression of 55-submodule).
5. update-index.rs `--cacheinfo`: reject tree mode / trailing-slash paths
   (verify_path) (35).
6. update-index.rs directory/missing-path handling per `process_path`:
   trailing-slash dirs print `Ignoring path`; missing path without --remove
   errors `does not exist and --remove not passed`; a directory arg with
   tracked children errors `is a directory - add individual files instead`
   even under --remove (33).
7. read-tree.rs: bind-overlap check (`Entry <x> overlaps with <y>. Cannot
   bind.`) considering sparse placeholders, and accept `--prefix` without a
   trailing slash (40).
8. checkout-index.rs: sparse-aware messages (`has skip-worktree enabled`,
   `is a sparse directory`) using the raw on-disk index to recover the
   collapsed-directory distinction; changed file present without --force
   errors `already exists, no checkout`; --ignore-skip-worktree-bits no
   longer bypasses the force guard (48,49,50,51).
9. blame.rs: working-copy blame (no rev, no --contents) requires the file on
   disk; aborts `fatal: Cannot lstat <p>: No such file or directory` (22).
10. show.rs: treat any `:`-prefixed arg as an object spec so `:deep/`
    (trailing-slash dir spec) routes through rev resolution and yields git's
    error instead of silently showing HEAD (53). (54 rev-parse already passed.)
11. rm.rs: plain pathspec glob crosses `/` like git wildmatch (no WM_PATHNAME),
    so `folder1/*` matches all tracked children (improves 79 sparse-checkout
    half; sparse-index collapsed-output half still needs lazy expansion).
12. status.rs: apply present-file skip-worktree clearing (status loaded the
    raw index directly, bypassing load_index_at's clearing) so a materialized
    out-of-cone file is reported as a normal modification (30, status family).
13. update-index.rs `--again`: replay `--skip-worktree`/`--no-skip-worktree`
    bit changes on each differing path (git do_reupdate -> update_one bit
    modes) instead of skipping skip-worktree entries (34).

## Remaining (not addressed — design/architecture scope)
- Sparse-index lazy-expansion + GIT_TRACE2 `ensure_full_index` region
  emission: 57,58,59,65,70,99,100,101,102,103,104. grit eagerly expands the
  sparse index on every `load_index_at`; making `ensure_not_expanded`/
  `ensure_expanded` pass requires lazy/path-triggered expansion plus trace2
  region emission — a cross-cutting change to index loading.
- Merge/cherry-pick/rebase commit-OID determinism + conflict-outside-cone:
  42,43,44,45,105.
- diff/read-tree-mu stat + sparse routing: 18,19,36,37,38,39,41.
- diff --check --cached must read .gitattributes from the index for
  skip-worktree paths: 97.
- add new out-of-cone file via `add .`: 74. status of present sparse files
  for add: 14,15. Misc: 2 (macOS `cp` AppleDouble noise — passes standalone),
  9 (-p interactive), 26,47,52,56,75,76,80,84,85,93.
