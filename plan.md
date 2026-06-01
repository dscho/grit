# PLAN.md — Grit remaining work

**Updated:** 2026-06-01 · **Released:** v0.2.1

This file lists **only work that is still to do**. Everything previously planned through
Phase 7 that is finished has been removed. For the shipped feature set and the explicit v1
non-goals, see [`docs/v1-scope.md`](docs/v1-scope.md).

## Shipped (context, not work)

Worktrees, signing (GPG+SSH), hooks, partial-clone/promisor + backfill, sparse-checkout
(cone+non-cone), non-interactive core workflows (checkout/reset/merge/cherry-pick/rerere/
status/log/gc/repack/maintenance), and the submodule baseline are in and released. CI gate
is green: `cargo fmt`, workspace `clippy` (0 errors), `cargo test -p grit-lib --lib` (229/0).

## How to work

1. Pick one item below; reproduce the failing subtests first (harness gotchas: see the
   `grit-test-harness-gotchas` notes — stale CSV counts, `PERL_PATH=/usr/bin/perl`, run a
   file directly from `tests/` if the wrapper errors).
2. Fix in `grit-lib/src` by default; `grit/src` only for CLI wiring.
3. Verify against a freshly-built **release** binary (the CSV can be stale); guard adjacent
   suites against regression by diffing not-ok lists vs a true-base binary.
4. Update this file as items complete.

---

## 1. Submodules (largest remaining area)

The submodule baseline works (`t7400` 111/124) but the recurse/populate family is blocked
on a design decision.

- [ ] **Gitlink-in-worktree model reconciliation (architectural).** `t2013-checkout-submodule`
  (16/74) and `t7406-submodule-update` (43/70) need a submodule **populate-on-checkout** model,
  but the two viable changes both regress `t7400`'s `deinit` chain (measured: allowing a
  populated submodule dir through the untracked-overwrite check → t2013 +8 but t7400 −8; not
  inline-populating an initialized submodule on plain checkout → breaks t7400 deinit 99–103).
  Unlocking requires making `submodule update --init` re-registration robust (t7400 66/67 leave
  `init` unregistered) so the deinit chain survives regardless of earlier-test state. This is a
  prerequisite for most remaining t2013/t7406 subtests.
  - Harness: `t2013-checkout-submodule` 16/74, `t7406-submodule-update` 43/70.
- [ ] **`t7506-status-submodule`** 28/40 — status across submodule states (depends partly on the
  populate model above).
- [ ] **`t7400-submodule-basic`** 111→124: remaining 13 are cross-subsystem (templateDir+post-checkout
  hook; `git rm` of a gitlink; deep relative `add` URL resolution; a few `update --init` edge cases).
- [ ] Near-green, likely small: **`t7407-submodule-foreach`** 21/23, **`t6437-submodule-merge`** 21/22,
  **`t4059-diff-submodule-not-initialized`** 7/8.

## 2. Sparse index (cone perf path)

- [ ] **`t1092-sparse-checkout-compatibility`** 65/106 — remaining failures need **sparse-index
  lazy expansion**: grit eagerly expands the sparse index on every `load_index_at`, so the
  `ensure_full_index` / GIT_TRACE2 region tests and the merge-OID-determinism / diff-sparse-routing
  cases fail. This is a real index-layer feature (keep sparse-directory entries collapsed; expand
  on demand), not subtest polish.

## 3. Partial clone / promisor tail

- [ ] **`t6421-merge-partial-clone`** 0/3 — needs **merge-ort relevant-rename pruning** so a merge
  fetches the minimal object set (expected 3/6/22 vs current 20/21/25). Implement basename-first
  rename relevance + rename-aware content-merge fetch in `grit-lib/src/diff.rs::detect_renames`.
- [ ] **`t5616-partial-clone`** 44/47 — remaining 2: `restore --recurse-submodules` (submodule recursion)
  and an HTTP v2 multi-round thin-pack negotiation case (grit ingests the unsubstituted filtered pack
  instead of the crafted thin pack).
- [ ] **`t5537-fetch-shallow`** 14/16 — subtest 16 (connectivity check before writing the shallow file);
  subtest 12 is submodule-shallow (depends on §1).

## 4. Log / diff formats

- [ ] **`t4202-log`** 90/149 — script-facing `log` formats; was `skip` until this cycle, so the gap is
  large. Triage the failing pretty/format/decoration cases (non-graph, non-interactive only).

## 5. Repack / maintenance / gc tail

- [ ] **`t7700-repack`** 39/47 — remaining 8 (bitmap/midx/filter edge cases).
- [ ] **`t7900-maintenance`** 71/72 — subtest 24 (`maintenance.loose-objects.batchSize`) needs
  `git fast-import` to emit a packfile under `fastimport.unpacklimit=0` (a fast-import feature, scoped out unless cheap).
- [ ] **`t6500-gc`** 34/35 — subtest 32 (background auto-gc `gc.log` recency behavior).

## 6. Small tails (1–5 subtests each)

- [ ] **`t7508-status`** 121/126 — 5 left (commit-template/order-dependent + macOS `chmtime` BSD-vs-GNU,
  the latter blocked by the test harness's `test-tool chmtime`).
- [ ] **`t7600-merge`** 81/83 — subtest 71 (annotated/signed-tag `pull --no-ff`: needs fetch to record
  annotated tags in FETCH_HEAD as `tag '<name>'` + merge forcing `--no-ff`). Subtest 70 is env-blocked (see §9).
- [ ] **`t7505-prepare-commit-msg-hook`** 22/23 — subtest 16 is the `rebase -i` conflict/continue engine
  looping, not a hook bug.
- [ ] **`t3501-revert-cherry-pick`** 20/21 — subtest 13 (dirty renamed file) needs better rename detection.

## 7. Phase 0.2 — transport into `grit-lib` (refactor, not test-driven)

- [ ] Move fetch/push wire-negotiation out of the binary into `grit-lib::transport`: `fetch_transport.rs`
  → `grit-lib::fetch_protocol`, `http_smart.rs` → `grit-lib::smart_http`, `http_push_smart.rs` →
  `grit-lib::receive_pack`/`send_pack`. Plan and call sites are in
  [`plans/phase0-0.2-transport-in-lib-assessment.md`](plans/phase0-0.2-transport-in-lib-assessment.md).
  Regression guard: `t5551-http-fetch-smart`, `t5541-http-push-smart` keep their current counts.

## 8. Release gate leftovers (Phase 8)

- [ ] **Public API review** — document the stable `grit-lib` entry points in crate rustdoc; mark
  experimental modules `#[doc(hidden)]`. (`fmt`/`clippy`/lib-tests gate items are already green.)
- [ ] **Performance** — benchmark pack indexing, status, and diff on large trees (no fsmonitor) and
  fix hot paths. Needs target tree sizes / thresholds defined first.

---

## 9. Blocked / not grit-fixable in this environment (document, do not chase)

These fail due to the sandbox/harness, not grit code. Track but don't treat as actionable:

- **`t5551-http-fetch-smart`** 36/37 (SHA-256 over HTTP) and **`t7600-merge`** 70 (`--no-ff --edit`):
  executed by/affected by the host system git (Apple git 2.39.5) or a leaked `GIT_EDITOR=true` from
  the harness env, not grit.
- **`t5541-http-push-smart`** 14/15 (`push --all/--mirror` to repo with alternates): fail at the base
  commit too; not a regression.
- **`t5407-post-rewrite-hook`** 11/17: 16/17 in an upstream-sanitized env; the 5-test gap is the grit
  copy of `tests/test-lib.sh` not unsetting `GIT_EDITOR`/`VISUAL` (editing `tests/` is forbidden).

---

## Tracking

- Harness dashboard: `data/test-files.csv` (refresh with `./scripts/run-tests.sh`); treat counts as a
  hint and re-run to confirm.
- v1 scope / exclusions: `docs/v1-scope.md`.
