# t1011-read-tree-sparse-checkout: 21/23 -> 23/23

## Summary
Fixed the two failing subtests in `tests/t1011-read-tree-sparse-checkout.sh` by
correcting non-cone sparse-checkout behavior in grit-lib. No test files were edited.

## Subtest 9 - "match directories with negated patterns (2)"
Sparse file `/*` + `!sub` + `sub/added` (non-cone). Expected `H subsub/added`,
grit produced `S subsub/added`.

Root cause: `grit-lib/src/ignore.rs` `sparse_pattern_matches` had an over-broad early
guard `if p.nodir && as_directory { return false; }`. The anchored root glob `/*` is
classified `nodir=true` (after stripping the leading `/`, body `*` has no slash). During
the parent-directory walk for `subsub`, that guard rejected `/*`, leaving `subsub`
UNDECIDED -> excluded (S). Real Git never gates NODIR / non-MUSTBEDIR patterns on dtype:
`match_basename` / `match_pathname` run regardless of `DT_DIR` (git/dir.c), so `/*` matches
the directory parent `subsub` and the path is included (H).

Fix: removed the `p.nodir && as_directory` guard. With it gone:
- `!sub` (genuinely nodir) still matches the directory `sub` via the unanchored basename
  branch, so `sub/addedtoo` stays S.
- `/*` matches the directory parent `subsub` via the `!pathname.contains('/')` branch, so
  `subsub/added` becomes H.
- `!/*/` (directory-only anchored `*`) still shadows `/*` for nested dirs via reverse-order
  first-match (the directory-only branch), so t7817 / t1091 are preserved.

## Subtest 21 - "print warnings when some worktree updates disabled"
Sparse file is the single non-cone pattern `sub`; `core.sparseCheckout=true` but
`core.sparseCheckoutCone` is never set. grit leaked two spurious stderr lines
(`warning: unrecognized pattern: 'sub'` and `warning: disabling cone pattern matching`)
into the captured `actual`.

Root cause: `grit-lib/src/sparse_checkout.rs` `apply_sparse_checkout_skip_worktree`
defaulted `cone_config` to `true` when the config key was absent, so it asked
`load_sparse_checkout_with_warnings` to cone-parse the non-cone file and emit warnings.
Real Git defaults `core.sparseCheckoutCone` to false (git/environment.c zero-init).

Fix: changed the absent-key default from `.unwrap_or(true)` to `.unwrap_or(false)` in
`apply_sparse_checkout_skip_worktree` only. Matching is unaffected since `effective_cone`
already requires `cone_struct.is_some()`. Scoped strictly to this function; the other
`.unwrap_or(true)` cone defaults elsewhere were left untouched.

## Files changed
- grit-lib/src/ignore.rs
- grit-lib/src/sparse_checkout.rs

## Verification
- t1011-read-tree-sparse-checkout.sh: 23/23 (was 21/23).
- Regression guards (all OK, none regressed):
  - t7817-grep-sparse-checkout: 8/8 (baseline 4/8 -> improved)
  - t1091-sparse-checkout-builtin: 57/77 (baseline 55 -> improved by 2)
  - t6435-merge-sparse: 6/6
  - t6435-merge-sparse-directory: 1/2 (unchanged baseline)
- cargo test -p grit-lib --lib: 228 passed, 0 failed (incl. non_cone_default_init_patterns,
  path_in_expanded_cone_tests, cone_directory_inputs_for_add_tests).
- cargo fmt clean; cargo clippy: no new warnings on changed lines.
