# t7505-prepare-commit-msg-hook (ticket 5911b7)

Mop-up round. Fresh run after other agents' fixes: 21/23.

Failing subtests:
- 11 "with hook (editor)"
- 16 "with hook (rebase -i)"

## Subtest 11 — FIXED

`GIT_EDITOR=fake-editor git commit` (no -m). Hook replaces line 1 of the message
file with `default` via `sed -e "1s/.*/$source/"`.

Root cause: grit ran `prepare-commit-msg` *after* the editor, on the already
comment-stripped (empty) message buffer. `sed 1s/.*/default/` on an empty file
produces nothing, so the commit aborted "due to empty commit message".

Upstream `builtin/commit.c:prepare_to_commit` runs the hook on the *full template*
buffer (status comments included) BEFORE launching the editor (commit.c:1116 hook,
then 1120 launch_editor).

Fix (grit/src/commands/commit.rs):
- Added `run_prepare_commit_msg_hook_on(repo, args, index_path, use_editor, msg_file)`
  helper (hook on a given path; bails on non-zero exit).
- `prepare_commit_message` now takes a `run_prepare_hook: &dyn Fn(&Path)->Result<()>`
  closure and calls it immediately before each `launch_commit_editor` (6 sites).
- Caller builds the closure and passes it; the post-message hook block now only runs
  for the non-editor path (`if !use_editor_for_message`), since editor commits already
  ran the hook before the editor.

After build: subtest 11 passes. 22/23.

## Subtest 16 — in progress

`with hook (rebase -i)`: rebase replay + edit/squash/reword interplay. Investigating.
