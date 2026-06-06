# t6030-bisect-porcelain.sh — ticket e11815

Subsystem group "bisect" (thread C). All bisect logic lives in `grit/src/commands/bisect.rs`.

## Baseline
Fresh run at session start: 84/96 (failing 39, 56, 57, 65-69, 71, 72, 89).

## Root cause
The previous bisect selection used a naive midpoint over the path-filtered rev-list
candidate set plus an ad-hoc skip handler. That diverged from Git's weight-based
`find_bisection` / `managed_skipped` machinery (`git/bisect.c`), giving wrong commits and
wrong "only skipped commits left" lists.

## Fix (all in grit/src/commands/bisect.rs)
1. Ported Git's `find_bisection` + `do_find_bisection` weight computation (per-candidate
   reachable-candidate count via full-ancestry walk), including the `approx_halfway`
   early-out so the chosen midpoint matches `git rev-list --bisect` exactly (verified the
   weight table for linear histories: 3->2, 4->2, 5->2, 6->3, 7->3, 8->4, ...).
2. Ported `managed_skipped` + `filter_skipped` + `skip_away` (`get_prn`/`sqrti`) so the
   "tried"/skip list and skip-away selection match Git. With skips, uses the
   `best_bisection_sorted` order (distance desc, oid asc); without skips uses the non-ALL
   `best_bisection` selection.
3. `error_if_skipped_commits` now driven by the real `tried` list (collected skipped
   commits) rather than an ad-hoc head-ancestor heuristic.
4. `bisect_skipped_commits_log` no longer appends `bad` separately — Git re-walks
   `bad ^good` and lists every yielded commit (the set already includes `bad`).
5. Candidate rev-list switched from `OrderingMode::Default` to `OrderingMode::Topo`:
   grit's path-limited date-order walk currently returns empty for pathspec+merge ranges
   (a rev_list regression another agent is editing in grit-lib/src/rev_list.rs); topo
   ordering sidesteps it and is correct for bisect (order does not affect weight ranking).
6. Added a "No testable commit found" pre-check: with pathspecs whose filtered set is empty
   but whose unfiltered `bad ^good` range is non-empty, emit BISECT_NO_TESTABLE_COMMIT(4)
   before the "was both good and bad" path (Git keeps TREESAME commits in revs.commits).

## Status
94/96 after fix. Remaining: 56 (restricting bisection on one dir and a file) and
69 (demonstrate identification of damage boundary) — both depend on correct pathspec+merge
rev-list candidate generation, which is the rev_list regression noted above.
