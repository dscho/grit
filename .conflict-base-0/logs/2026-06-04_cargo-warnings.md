# Cargo warnings cleanup

Task: remove current Cargo warnings from the workspace.

Actions:
- Ran `cargo check --workspace` and found unused attributes, imports, variables, assignments, dead helper state, and one example target compile issue under broader checks.
- Removed or renamed unused code where behavior was unchanged.
- Fixed `grit-lib/examples/pack_index.rs` to print pack object-id bytes via `pack::oid_bytes_to_hex`.
- Ran `cargo fmt`.

Validation:
- `cargo check --workspace` passed with no warnings.
- `cargo check --workspace --all-targets` passed with no warnings.
- `cargo clippy --workspace --all-targets` was probed and remains blocked by a broad pre-existing Clippy backlog, including denied unwrap/expect use in test targets and hundreds of style warnings.
