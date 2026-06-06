# t7406-submodule-update

Ticket: d7ea3d

## Starting state
58/70 (regression from recorded 67/70; a shared submodule URL-resolution change degraded it).
Failing at first run: 5, 27, 48, 49, 51, 52, 58, 62, 63, 64, 65, 66.

## Fix 1: relative-URL resolution for nested submodules
`resolve_submodule_super_url` (grit/src/commands/submodule.rs) used the submodule worktree
*path* as the base when resolving `../foo` relative URLs for nested repos, instead of the
submodule's own `remote.<default>.url`. C Git's `resolve_relative_url` always resolves against
`remote.<default>.url` (cwd only as fallback). Changed the nested branch to use
`default_remote_url_raw(repo_git_dir)` with a worktree fallback.
Fixed tests 5, 27, 49, 52, 62, 63.

## Fix 2: relative submodule gitlinks
Submodule clones (both `submodule update` clone path in
grit/src/commands/_submodule_run_update_inner.rs.inc and `clone --recurse-submodules` path in
grit/src/commands/clone.rs) used `grit clone --separate-git-dir`, which writes an *absolute*
`gitdir:` path (correct for top-level clone, t5601). C Git's submodule machinery
(`connect_work_tree_and_git_dir`, dir.c) writes a *relative* gitlink so a copied/moved
superproject keeps its submodule pointing at the copy. After each submodule clone, rewrite the
`.git` gitlink to relative via `write_submodule_gitfile`.
Fixed tests 64, 65, 66 (cp -r top-clean top-cloned then operating on the copy).

## Remaining: 48, 51, 58
- 48: submodule add places git-dir in superprojects git-dir recursive
- 51: submodule update properly revives a moved submodule
- 58: submodule update --quiet passes quietness to merge/rebase
