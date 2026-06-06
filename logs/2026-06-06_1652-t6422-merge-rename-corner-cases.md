# t6422-merge-rename-corner-cases

Ticket: 65325c. Group: merge-ort (thread C).

Start state: 14/26 passing. Failing (non-known-breakage): 9, 16, 18, 19, 25, 26.
(Tests 2, 4, 5, 6, 14, 15 are `test_expect_failure` known breakage.)

## Fixes

### #9 disappearing dir in rename/directory conflict handled
`grit/src/commands/merge.rs`, Case 1 rename pass. When ours renames `sub/file` -> `sub`
and theirs only modified `sub/file` (the rename source), the directory `sub/` disappears
once the rename is consumed; there is no real file/directory conflict. Added
`only_tree_descendant_is()` helper and a `theirs_dir_is_only_rename_source` guard so the
rename handler content-merges the two versions instead of bailing to the D/F pass.

### #16 rename/rename/add-dest merge still knows about conflicting file versions
`grit/src/commands/merge.rs`, Case 2 rename/rename(1to2) staging block. ours renamed
`a`->`c` + added `b`; theirs renamed `a`->`b` + added `c`. The 1to2 logic staged the
add at `ours_target` (theirs's added `c` at stage 3) but missed the symmetric add at
`theirs_new_path` (ours's added `b` at stage 2). Added the symmetric staging plus proper
two-way conflict-marked working-tree content via new `two_way_conflict_blob()` helper
(labels `HEAD` / `their_name`).

## Remaining: 18, 19, 25, 26
- 18 rrdd: rename/rename(2to1)/delete/delete
- 19 mod6: chains of rename/rename(1to2) and rename/rename(2to1)
- 25 rename/rename(1to2) with a binary file
- 26 submodule/directory preliminary conflict
