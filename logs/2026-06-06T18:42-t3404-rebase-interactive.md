# t3404-rebase-interactive (ticket 9e2eff)

Subsystem: rebase-core (interactive rebase / sequencer machinery).

## Key environment gotcha (do NOT trust direct `sh ./t3404...` runs)

Running `sh ./t3404-rebase-interactive.sh` directly leaks the agent shell's
profile env (notably `GIT_EDITOR=true`), which makes grit resolve the sequence
editor to `true` and silently skip all fake-editor edits — producing a flood of
*false* failures (tests 3,4,5,...). The official `scripts/run-tests.sh` (and
`/tmp/run3404.sh`, which replicates its `env -u GIT_EDITOR ...` invocation) gives
the true result. Always reproduce via `/tmp/run3404.sh`.

## Fix 1: interactive todo missing the help/instruction comment block

`run_interactive_rebase` (plain `rebase -i` path, grit/src/commands/rebase.rs)
wrote the todo as bare pick lines with no trailing blank line + `# Rebase ... onto
... (N commands)` help block. Real git appends that block (preceded by a blank
line). Several `--exec` tests (65-72) calibrate `sed 1,Nd` against that layout
(the blank line survives the fake editor's `grep -v '^#'`), so grit's output was
off by one line.

Fix: `run_interactive_rebase` now takes a `revs_onto` arg and calls
`append_rebase_todo_help` (same helper the `--rebase-merges` path already used),
computing `revs = <short-upstream>..<short-orig-head>` and `onto = <short-onto>`.

Result: 76 -> 81 / 132.

## Remaining failures (51) — clusters to investigate
- 122-129: --update-refs (label/update-ref command generation + application)
- 100-111: rebase.missingCommitsCheck warn/error + static checks of bad command/SHA
- 75-80: rebase -i --root (sentinel/fixup/reword)
- 84,85,92,107: core.commentchar / core.abbrev / abbreviateCommands
- 94-96: commits that overwrite untracked files
- 113,114: --gpg-sign
- 117-120: post-commit hook / empty pick errors / onto hash
- 18,35,43,45,46,47,48,50,54,57,69,70,72,81,91,108,109,130,131 misc

## Fix 2: static todo check (`todo_list_parse_insn_buffer` + `todo_list_check`)

grit had no upfront validation of the edited interactive todo. Added
`validate_edited_interactive_todo` (grit/src/commands/rebase.rs) mirroring git's
`rebase-interactive.c`: unknown command / bad SHA -> `error: invalid line N: <line>`;
leading fixup -> `error: cannot 'fixup' without a previous commit`; plus the
`rebase.missingCommitsCheck` warn/error path (warn continues, error aborts) printing
the exact `Warning: some commits...` block. All failures use
`explicit_exit::SilentNonZeroExit{code:1}` so no extra `error:` line leaks past the
git advice. It runs in `do_rebase` AFTER `checkout_onto` (git rewinds HEAD first, then
checks) so `--edit-todo` + `--continue` recovery works, AND in `do_edit_todo` comparing
against `git-rebase-todo.backup` (which the validator now always writes).

In-ISOLATION now passing: 100,101,102,108,109 (and 99,103? - check). Full-run count
stuck at 81 because the cascade: failing tests 94-96 (overwrite-untracked) leave a
rebase in progress that breaks the later missingCommitsCheck tests 100-106 when run in
sequence. MUST fix 94-96 (and 84/85/91/92) to unlock the sequence.

## KEY: full-run vs isolation divergence
Many tests pass with `--run=1,N` but fail in the full sequential run because an EARLIER
failing test leaves a rebase-in-progress / wrong branch. `/tmp/run3404.sh` replicates the
real runner env (CRITICAL: avoids the GIT_EDITOR=true profile leak). Use
`sh /tmp/run3404.sh --run=1,A-B` to test a window. Fixing the earliest failures in a
cluster unblocks the rest.
