# grit-simple

`grit-simple` provides `gi`, a small opinionated command line interface backed by [`grit-lib`](https://crates.io/crates/grit-lib).

It is not intended to be a drop-in replacement for Git. For Git-compatible command behavior, use the `grit` binary from the `grit-cli` crate. `gi` is a simpler interface for workflows built on Grit's Rust implementation.

## Install

```sh
cargo install grit-simple
```

This installs the `gi` executable.

## Commands

`gi` favors one obvious way to do the common thing, plain-language output, and a
status screen that doubles as the home base.

### `gi` / `gi status`

Running `gi` with no arguments shows the dashboard: the current branch, a
shortlog of the commits you're ahead of the target branch by, your staged and
unstaged changes, untracked files, and a hint for what to do next.

```sh
gi
# explicit form / alias:
gi status
gi st
```

```text
On feature/example  ·  2 ahead of origin/main

  abc1234  Add example
  fed9876  Refine output

Staged
  +  new-file.txt                      new
  ~  src/main.rs                       modified

Changed (not staged)
  ~  README.md                         modified

Untracked
  ?  notes.md

→ gi add <file> to stage  ·  gi commit "message" to commit
```

### `gi add`

Stage changes. With no paths, stages **everything** that `gi status` reports as
changed — modifications, deletions, and untracked files alike. Pass paths to
stage a subset.

```sh
gi add            # stage all changes
gi add src/ a.txt # stage only these paths
```

### `gi commit`

Record the staged changes as a new commit. The message can be a positional
argument or `-m`; `-a` stages every change first.

```sh
gi commit "what changed"
gi commit -m "what changed"
gi commit -a "stage everything, then commit"
```

Author/committer identity comes from `user.name` / `user.email` (honoring the
`GIT_AUTHOR_DATE` / `GIT_COMMITTER_DATE` overrides).

### `gi shortlog`

Show the current branch, the target branch, and commits that are reachable from
`HEAD` but not from the target branch.

```sh
gi shortlog
# alias:
gi sl
```

Target branch lookup uses the first available value from:

1. `target.branch` in Git config
2. `origin/master`
3. `origin/main`
4. `master`
5. `main`

Example:

```text
On feature/example
Ahead of origin/main by 2 commits
abc1234 Add example
fed9876 Refine output
```
