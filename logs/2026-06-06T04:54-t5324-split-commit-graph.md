# t5324-split-commit-graph — mop-up round 1 (ticket fba897)

## Starting state
Prior agent reported 39/42 (failing 13, 25, 40), but a fresh run showed 29/42 — a
regression introduced by the shared `target/release/grit` binary being swapped by another
agent mid-effort. Rebuilt and confirmed the genuine failures.

## Root cause of the 29→39 regression (the big find)
Tests 15–26 each `git clone . <dir>` (local clone, hardlinked objects tree). grit's local
clone faithfully **hardlinks** `info/commit-graphs/commit-graph-chain` (and the layer
`*.graph` files), matching Git's `copy_or_link_directory`. The commit-graph writer then
rewrote the chain with `fs::write(&chain_path, ...)`, which truncates the existing file
**in place** — mutating the shared inode and corrupting the clone *source's*
`commit-graph-chain`. After test 15's `merge-2` clone wrote a 3rd layer, the main repo's
chain gained a bogus 3rd line whose `.graph` file did not exist there, so every later clone
of `.` verified with `warning: unable to find all commit-graph files`.

## Fix
`grit/src/commands/commit_graph.rs`: added `write_file_atomic(path, contents)` (temp file in
the same dir + `fs::rename` over the target). `rename` creates a fresh inode, so a hardlink
shared with the clone source is left untouched (this is what Git does — it always renames its
lockfile into place). Routed all fixed-name writes through it:
- split chain file write (the corrupting one)
- split layer-file write (defensive; layer names are content-addressed so rarely collide)
- split-migration chain write
- non-split single `info/commit-graph` write (was `File::create` truncate-in-place)

Dropped the now-unused `BufWriter` import (kept `Write` — still used by verify writeln!s).

## Result
39/42 (failing 13, 25, 40) — recovered the prior agent's count. Committed.

## Remaining (all cross-alternate / deep-clone, as prior agent noted)
- t13: fork with an alternate to a repo that already has a 2-layer chain must produce a
  3-line chain (2 alternate base layers + 1 new tip). `CommitGraphChain::try_load` only reads
  the local objects dir, never the alternate's chain.
- t25: `git commit-graph verify --object-dir=<dir>` — the verify subcommand does not accept
  `--object-dir` (clap rejects it). Plus cross-alternate verify.
- t40: deep multi-clone (mixed-merge-gdat) — the 5th-layer split write sees new_only count
  wrong (reports num_commits 8 instead of 47); likely the merge strategy not absorbing the
  right base layers when the chain spans gdat/non-gdat layers across clones.
