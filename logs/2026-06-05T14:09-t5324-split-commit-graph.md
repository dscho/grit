# t5324-split-commit-graph — work log

Ticket: fba897. Subsystem: pack-storage (commit-graph machinery).

## Starting state
17/42 passing.

## Root causes found
1. **No split merge strategy.** `commit-graph write --split` never merged base
   layers into the new tip (split_graph_merge_strategy / merge_commit_graphs in
   commit-graph.c). Implemented size-multiple + max-commits + no-merge logic in
   `grit/src/commands/commit_graph.rs::cmd_write`.
2. **Generation-data chunk always written.** Added `write_generation_data`
   parameter to `build_commit_graph_bytes` and conditional GDA2/GDO2 emission.
   Driven by `commitGraph.generationVersion` and the topmost kept base layer.
3. **read-graph read the base layer, not the tip.** `test-tool read-graph` reads
   the last (tip) line of the chain file, not the first. Fixed in main.rs.
4. **Local clone didn't copy `info/commit-graphs`.** `copy_objects` in clone.rs
   skipped the commit-graphs subdir; now recurses into it (real local clone
   copies the whole objects tree). Unblocks all the `git clone . X` verify tests.
5. **Verify was single-file only.** Rewrote `cmd_verify` to load+validate the
   split chain: chain-file size, hash-line validity, missing-layer detection,
   BASE-chunk match, per-layer checksum, ODB cross-check, --shallow + progress.

## Helpers added
- `CommitGraphChain`: `num_layers`, `layer_commit_counts_tip_first`,
  `layer_has_generation_data_tip_first`, `layer_hashes_tip_first`,
  `layer_object_dirs_tip_first`, `layer_oids`, `sub_chain_tip_first`.
- `cmd_write`: `hex_to_hash20`, `parse_expire_time`, `parse_tz_offset`.

## After first commit: 26/42 passing.

## Second batch (33/42)
- Layer identity for chain BASE-chunk match = file TRAILER (Git g->oid), not the
  filename. A corrupted trailer breaks the chain match and surfaces as
  "incorrect checksum" / "chain does not match". Fixed tests 17, 18, 19, 22.
- Local clone of split commit-graph layer files must use writable (0644) perms,
  not the source 0444 (tests corrupt them in place).
- resolve_layer_path is case-SENSITIVE even on case-insensitive FS (macOS APFS):
  a chain line whose hex case is corrupted must be "file not found". Test 22.
- read-graph (test-read-graph.c) prints a FIXED set of known chunks in a fixed
  order, never BASE or "unknown". Fixed parse_graph_file. Tests 37, 38, 39.
- generationVersion=1 forces no GDA2 chunk; a split write atop a non-GDA2 base
  also drops GDA2 (only ever *removes* generation data). Tests 37, 38, 39.

## Remaining failures (9): 13, 15, 25, 26, 31, 33, 34, 40, 42.
- 13, 25: alternates — chain spans an alternate object dir; CommitGraphChain::load
  only reads layer files from the local objects dir, so cross-alternate chains
  don't load/write the right number of layers.
- 31: --split=replace + graph_read_expect (read-graph base count off / chain not
  reduced to 1 as expected).
- 33, 34: core.sharedrepository modebits on the split layer + chain file.
- 40: deep multi-clone dependency; mixed-merge-gdat ends up cloning a flattened
  chain ([103,8]) so the FIFTH-layer write sees new_only=0.
- 42: temporary graph layer must be discarded on write failure (missing parent
  object) and $graphdir left empty.
