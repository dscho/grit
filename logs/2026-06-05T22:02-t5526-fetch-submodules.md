# t5526-fetch-submodules — submodule fetch recursion

Ticket 1d762d. Started 22/56 passing.

## Root causes found & fixed

1. **`git submodule add` of an existing repo detached the source repo's HEAD.**
   For the "Adding existing repo" path, grit ran `checkout_submodule_worktree` which did a
   `checkout <oid>` and detached HEAD (to a bare OID), leaving `refs/heads/<branch>` stale.
   Git (`builtin/submodule--helper.c:add_submodule`) only runs `checkout -f` on the *clone*
   path; the existing-repo path leaves the worktree and its branch HEAD untouched.
   Fix (grit/src/commands/submodule.rs): track `did_clone`; only run the post-add worktree
   checkout when we cloned. This was the big one — with a detached source HEAD, later
   `add_submodule_commits` advanced HEAD but not the branch, so the downstream submodule fetch
   found "no new commits" and printed nothing (subtests 2,4,5,8,... all failed).

2. **`From <url>` header / stored clone URL wrong (`/.` vs verbatim).**
   `git clone .` must store the remote URL as `<cwd>/.` (Git `absolute_pathdup`: prepend cwd,
   no normalization). grit's `setup_origin_remote` / `setup_origin_remote_bare` used
   `source_path.canonicalize()` which stripped the trailing `.` and resolved symlinks. The
   `From` line had a compensating hack in fetch.rs that *appended* `/.` to the canonicalized
   remote git_dir — correct for the super (`From <pwd>/.`) but WRONG for submodules
   (`From <pwd>/submodule/.` instead of `<pwd>/submodule`).
   Fix:
   - clone.rs: new `absolute_clone_source_url()` = `cwd.canonicalize().join(literal source)`,
     used by `setup_origin_remote{,_bare}`. Preserves `.`/`./` like Git; symlink-resolved cwd
     matches Git's getcwd.
   - fetch.rs `resolve_fetch_from_line_url`: just return `normalize_fetch_url_display(raw_url)`
     (the configured URL, trimmed of trailing `/` and `.git`) — Git's actual behavior.

3. Build fix: clone.rs:6723 `UnpackOptions` initializer was missing the new
   `shallow_boundaries` field added by another agent; added `..Default::default()` to match the
   other call sites (bundle.rs, bundle_uri.rs) so the tree builds.

## Result
34/56 passing (was 22).

## NOTE / hazard
fetch.rs is being concurrently edited by another agent — my `resolve_fetch_from_line_url`
change was reverted once mid-run by their commit landing on disk; re-applied. If t5526
regresses to the `/.`-on-submodule failure, re-apply that one-liner.

## Remaining failures (22): 4, 27, 28, 30, 31, 33-36, 38-45, 52-56
Still to diagnose — mostly on-demand recursion edge cases (changed-but-not-in-index,
custom remote names, FETCH_HEAD, name-conflicted submodules, fetch --all recursion,
broken-repo handling).
