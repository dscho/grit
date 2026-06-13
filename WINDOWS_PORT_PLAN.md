# Windows Compilation Plan — `grit-lib` + `grit-simple`

Goal: get `grit-lib` and `grit-simple` to **compile for Windows**. `grit-cli`
(`grit/`) and the other workspace members (`grit-http-server`, `grit-protocol`,
`grit-utils`, `grit-examples`) are explicitly **out of scope**.

`grit-simple` (the `gi` binary) contains **no platform-specific code of its
own** — it only re-exports `grit-lib` operations through `clap`. So "make
grit-simple build on Windows" reduces entirely to "make `grit-lib` build on
Windows with its default feature set" (`test-tools` and `http-ureq` both off).

## Current state

The port is already ~95% complete. Most platform-specific call sites are
already paired with `#[cfg(unix)]` / `#[cfg(not(unix))]` branches, and the
infrastructure modules are gated:

- `git_date/compat.rs` — fully cross-platform: `time_t`/`tm` and
  `localtime_r`/`mktime`/`strftime` route to `libc` on Unix and the MSVC CRT
  (`_localtime64_s`, `_mktime64`, `strftime`) on Windows. `gmtime` is pure Rust.
- `simple_ipc` and `unix_process` are `#[cfg(unix)]` modules with
  `#[cfg(not(unix))]` stubs in `lib.rs` (Unix-domain-socket IPC and
  `kill(2)`-based process control are Unix-only).
- `test_tool_progress` (uses `AsRawFd`) and `parse_options_test_tool` are gated
  behind the `test-tools` feature, which is **off** in `grit-simple`'s build, so
  they never compile here.
- Already-ported with both branches: `ident_resolve`, `signing`, `mailmap`,
  `index`, `crlf`, `attributes`, `untracked_cache`, `ident_config`, `repo`,
  `split_index`, `shared_repo`, `porcelain/stash`, `porcelain/status`, `odb`,
  `hooks`.

## Verification method

There is no Windows toolchain in this environment, but `cargo check` does not
link, so the Windows **std** target is enough to surface every type/path error:

```sh
rustup target add x86_64-pc-windows-gnu
cargo check --target x86_64-pc-windows-gnu -p grit-simple
```

(`grit-simple` pulls in `grit-lib` with default features, so this checks both.)

## Remaining compile errors and their fixes

A clean cross-check surfaced exactly **8 errors across 3 files** — the last
un-gated Unix call sites. Each is fixed with the same `#[cfg]` pattern already
used throughout the crate.

### 1. `grit-lib/src/porcelain/checkout.rs` (5 errors)

`apply_index_file_mode` and `write_to_worktree` used
`std::os::unix::fs::PermissionsExt::set_mode` and
`std::os::unix::fs::symlink` unconditionally.

- **`apply_index_file_mode`**: gate the `PermissionsExt`/`set_mode` body in
  `#[cfg(unix)]`; on Windows it is a no-op (no POSIX mode bits — the executable
  bit is not represented in the filesystem). Mirrors `porcelain/stash.rs`.
- **`write_to_worktree`**: gate the `symlink` call in `#[cfg(unix)]`; on Windows
  write the symlink target as a regular file (unprivileged symlink creation is
  unavailable) so the worktree stays populated. Gate the executable-bit
  `set_mode` block in `#[cfg(unix)]`.

### 2. `grit-lib/src/diff.rs` (2 errors)

`worktree_content_matches_index_oid` used
`std::os::unix::ffi::OsStrExt::as_bytes` to hash a symlink target.

- Extract a `symlink_target_bytes(&Path) -> Vec<u8>` helper: raw `OsStr` bytes
  on Unix, lossy UTF-8 on Windows (WTF-8 `OsStr` has no stable byte view;
  symlink targets in Git trees are UTF-8 in practice). Mirrors the
  `porcelain/stash.rs` `symlink_target_to_bytes` pattern.

### 3. `grit-lib/src/difftool.rs` (1 error)

The symlink-backed difftool checkout optimisation called
`std::os::unix::fs::symlink` unconditionally.

- Gate the whole `if use_symlinks { … symlink … }` fast-path in `#[cfg(unix)]`;
  on Windows it falls through to the existing file-copy path below.

> Status: **all three files have been fixed in this branch** and
> `cargo check --target x86_64-pc-windows-gnu -p grit-simple` now completes with
> **no errors** (warnings only). The host build (`cargo check -p grit-simple`)
> remains clean.

## Behavioural caveats on Windows (intentional, document but don't block)

These keep the crate compiling and functional; exact Git semantics for these
edge cases are a follow-up, not a compile blocker:

- **Symlinks**: `checkout`/`stash` materialise mode-`120000` entries as regular
  files containing the target path rather than real symlinks. `difftool` copies
  instead of symlinking.
- **Executable bit**: not stored by the filesystem, so mode application is a
  no-op. The index already records mode `100644`/`100755` from the tree; only
  worktree application differs.
- **`untracked_cache`**: uses `uname(2)` (`nix`) under `#[cfg(unix)]` for cache
  validity; the `#[cfg(not(unix))]` branch already disables/short-circuits it.
- **Default identity**: `ident_config` resolves the user via `getuid`/passwd on
  Unix; the Windows branch already falls back to env-based resolution.

## Follow-up clean-up (not required to compile, but do before enabling `-D warnings` in CI)

The Windows cross-check emits ~13 warnings, all in already-ported files where an
import or binding is only used on Unix. Tidy with `#[cfg(unix)]` on the imports /
`let _ =` bindings or `#[cfg_attr(not(unix), allow(unused))]`:

- `porcelain/stash.rs:22` (`PermissionsExt` import), `:444` (`target` unused)
- `attributes.rs:9`, `ident_resolve.rs:10`, `shared_repo.rs:7`, `mailmap.rs:14`
  (unused imports under non-unix)
- `hooks.rs:282`/`:290` (`repo`/`meta` unused on non-unix)
- `repo.rs:352` (`gitfile` field unused on non-unix)
- `git_date/compat.rs:12` (`time_t` non-camel-case — pre-existing, allow it)

## Optional dependency hygiene

`grit-lib/Cargo.toml` declares `nix` and `libc` as unconditional dependencies.
They currently compile for `x86_64-pc-windows-gnu` (their Windows-relevant
surface is small), so this is **not** a blocker. For cleanliness, move the
Unix-only ones behind a target table so they are never built on Windows:

```toml
[target.'cfg(unix)'.dependencies]
nix = { workspace = true }
```

(`libc` must stay generally available — `git_date/compat.rs` references
`libc::c_char` under `#[cfg(unix)]` only, but a workspace-level move to
`cfg(unix)` is safe since the Windows path uses bare `extern "C"`.) Validate
with the cross-check after moving.

## MSVC vs GNU note

Validation uses `x86_64-pc-windows-gnu` because `cargo check` needs only the
target std, not a linker. The one place that actually **links** against the CRT
is `git_date/compat.rs` (`_localtime64_s`, `_mktime64`, `strftime`). These exist
in both mingw's `msvcrt` and MSVC's `ucrt`, so a real
`x86_64-pc-windows-msvc` **build** (not just check) should be run on a Windows
runner / via `cargo-xwin` as a final gate before declaring the port done.

## Definition of done

1. `cargo check --target x86_64-pc-windows-gnu -p grit-simple` — no errors. ✅ (done in this branch)
2. `cargo check -p grit-simple` (host) — still clean. ✅
3. (Recommended) `cargo build --target x86_64-pc-windows-msvc -p grit-simple` on
   a Windows runner to validate the `git_date` CRT FFI links.
4. (Optional) Warning clean-up + `nix` target-gating per the sections above.
