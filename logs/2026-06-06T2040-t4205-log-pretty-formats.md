# t4205-log-pretty-formats — work log (ticket a9cb4f)

Start: 113/125 passing. Failing real subtests: 17-23 (NUL reflog), 43 (graph absolute
column + i18n encoding), 101 (%S --bisect), 116 (magical wrapping). 16 and 125 are
TODO known breakage (`test_expect_failure`).

## Fixes landed

### 17-23: NUL termination with --reflog --pretty=<fmt>
Root cause was two bugs:
1. `rev-list --reflog` and `log --reflog` disagreed on commit order when commits share a
   committer timestamp. Both seeded reflog tips via `all_reflog_oids` (a `HashSet`, no order)
   then hex-sorted. The default date-priority walk (`date_order_walk` in
   `grit-lib/src/rev_list.rs`) seeded only in-degree-zero commits and broke equal-date ties by
   OID in a max-heap, never by insertion order. Git's `limit_list` seeds every explicit tip up
   front and breaks equal-date ties by FIFO insertion order (`prio_queue`).
   - Added `grit_lib::reflog::all_reflog_oids_ordered` returning OIDs in Git's
     `add_reflogs_to_pending` scan order (ref-name sorted, each reflog oldest-first, old then
     new, first-seen wins).
   - `rev-list` and `log` now feed reflog tips through that ordered function.
   - `date_order_walk` now takes a `tip_order: &[ObjectId]`; when non-empty it seeds the explicit
     tips immediately with FIFO seq (matching Git), else falls back to the old in-degree-zero
     seeding (path-walk). `path_walk.rs` passes `&[]`.
2. `-z` framing for builtin pretty formats: the inter-entry blank line was emitted as `\n`
   instead of `\0`. Patched all four log output loops + the `email`/`mboxrd` case
   (`log_z_needs_email_separator`) + oneline (terminate each entry with `\0`).
3. `log --pretty=email` was completely broken (printed literal "email"): added an `email`/`mboxrd`
   arm to `format_commit`.
4. `show -s --pretty=<full|fuller|raw>` emitted an extra trailing blank line vs `log`; gated that
   blank by `root_diff_shown` so `show -s` matches `log` (and real git).

### 43: right alignment at nth column with --graph + i18n.logOutputEncoding
- `write_graph_interleaved_commit_msg` decoded the body via `String::from_utf8` and errored on
  non-UTF-8 (ISO-8859-1) output. Rewrote it to operate on raw bytes (line-split on `\n` only).
- `%>|(N)` absolute-column math ignored the graph prefix width, over-padding by 2. Added a
  `GRAPH_PREFIX_WIDTH` thread-local set by the graph renderer and added into the absolute-column
  computation in `apply_format_string`.

### 101: %S with --bisect labels commits with refs/bisect/bad ref
`git log --bisect` was a no-op in the log command (rev-list had `append_bisect_ref_specs` but log
did not). Added `collect_bisect_ref_tips` + a `--bisect` block in the log walk: `refs/bisect/bad*`
become positive tips (named for `%S` via `build_named_source_map`, which already gives nearest-tip
labels), `refs/bisect/good*` become negative excludes. Also added `args.bisect` to
`rev_input_given`. Now matches the expected 3-commit range with correct `%S`.

## Remaining
- 116: magical wrapping directives `%w(1)%+d%+w(2)`. grit's `%w(...)` directive is parsed but
  ignored (no actual word-wrapping implemented), and the trailing `%+w(2)` must stay literal.
  Implementing real `%w()` wrapping is a sizable feature; deferred.

## Note (not mine)
t6007 test 7 (`rev-list --left-right --count`) fails expecting a 3rd `0` column; the rev-list
command only prints the 3rd (equivalent) column under `--cherry-mark`. This is in
`grit/src/commands/rev_list.rs` count formatting, untouched by this ticket — another agent's area.
