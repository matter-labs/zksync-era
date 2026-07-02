# batch_splitter

Splits a **sealed** L1 batch `N` into two independently-provable Airbender pieces, so that batches which Boojum can
prove but which exceed Airbender's geometry can still be proven by cutting them in half (and, by the caller's retry
loop, in quarters, eighths, …) until each piece fits.

This tool is **non-invasive**: it never writes to the canonical chain (`l1_batches` / `miniblocks` / `transactions`),
never re-commits to L1, never re-numbers batches, and never touches the node's Merkle-tree RocksDB. It reads only
Postgres (MVCC) and immutable object-store blobs, and writes two new object-store blobs the prover loop can pick up.

## Why "split" means "re-execute", not "slice"

A witness input is not sliceable at an L2-block boundary:

- `VMRunWitnessInputData.initial_heap_content` is the bootloader memory for the **whole** batch as a single program.
- `storage_refunds` / `pubdata_costs` / `merkle_paths` are flat per-VM-op arrays in execution order.

What _is_ per-block is `AirbenderVerifierInput.l2_blocks_execution_data: Vec<L2BlockExecutionData>` (the transactions of
each L2 block) plus the storage snapshot inside `vm_run_data.witness_block_state`. Those are exactly the inputs an
execution needs, so each half is produced by **re-running the bootloader over its block range** — the same thing
`BasicWitnessInputProducer` and `AirbenderVerifierInput::verify()` already do.

## The split

For batch `N` spanning L2 blocks `[first, last]`, cut at `mid = first + ceil(n/2)` (even by block count, whole blocks
only — see "fictive block" below):

| Field                          | Piece A `[first, mid)`                         | Piece B `[mid, last]`                                          |
| ------------------------------ | ---------------------------------------------- | -------------------------------------------------------------- |
| `l2_blocks_execution_data`     | blocks `[first, mid)`                          | blocks `[mid, last]`                                           |
| `l1_batch_env`                 | = N's (same start, same `previous_batch_hash`) | `previous_batch_hash = root_A`; `first_l2_block` = block `mid` |
| `system_env`, `pubdata_params` | = N's                                          | = N's                                                          |
| `vm_run_data`                  | regenerate by re-exec of A                     | regenerate by re-exec of B                                     |
| `merkle_paths`                 | regenerate from N's multiproof (N-1 state)     | regenerate from N's multiproof (N-1 + A's writes)              |
| `commitment_input`             | = N's (`prev_*` from batch N-1)                | computed from **piece A's** commitment                         |

`root_A` is piece A's intermediate state root — it is never committed anywhere; it only links the two pieces.
Composition holds: `old_root_N → root_A` (A) then `root_A → new_root_N` (B) = `old_root_N → new_root_N` (N).

### Built-in correctness self-check

After regenerating B's Merkle paths we get `root_B`. It **must equal batch N's committed root hash**. The tool asserts
this and fails loudly otherwise — it is a cheap, total check that the re-execution + sparse reconstruction reproduced N
exactly.

## The hard sub-problems (and how each is handled)

1. **Storage chaining for piece B.** B re-executes from a snapshot that is _N's start state overlaid with A's writes_.
   We seed B's `StorageSnapshot` from the full-batch `witness_block_state` (a superset of both halves' reads) and
   overlay the writes from piece A's `FinishedL1Batch.final_execution_state. deduplicated_storage_logs`. The
   `is_write_initial` flag for B drops any slot piece A inserted (it is no longer a first write). See `reexec.rs`.

2. **Leaf-index assignment.** New (inserted) leaves get enumeration indices `start, start+1, …`. Piece A starts at N's
   start index (`original merkle_paths.next_enumeration_index()`); piece B continues at `start + (#inserts in A)` — the
   same order N used, which is why the final root matches. Handled inside `sparse_tree.rs`.

3. **Merkle paths — fully non-invasive.** The tool never touches the node's Merkle-tree RocksDB (opening it would take
   its lock; snapshotting a live DB races the writer). Instead it reconstructs a sparse binary Merkle tree from batch
   N's own immutable `WitnessInputMerklePaths` blob (the only non-invasive source of sibling hashes), re-applies A's
   then B's writes, and regenerates each piece's paths + `root_A`. See `sparse_tree.rs`. Why it works: each touched
   key's N-1 value comes from its first occurrence in N's proof, and sibling subtrees with no touched leaf are invariant
   during N (so their hashes can be read from any op). Hashing reuses `Blake2Hasher`'s
   `hash_leaf`/`hash_branch`/`empty_subtree_hash`, so roots/paths match exactly. Two root self-checks bracket it: the
   reconstructed N-1 root must equal N's `previous_batch_hash`, and the post-B root must equal N's committed root.

4. **Synthetic commitment for piece B.** `commitment_input` for B needs piece A's Airbender commitment
   (`prev_batch_commitment`, `prev_aux_hash`, `prev_meta_hash`). This is assembled from A's re-execution output the same
   way `commitment_generator` does for a real batch (`AirbenderCommitmentComputer` +
   `L1BatchCommitment::airbender_artifacts`). This is the deepest module and is gated behind `--commitment-chaining`
   (off by default); without it the tool emits B with `commitment_input: None` (enough to test that each half _fits
   Airbender's geometry_, which is the immediate goal), and the full commitment chain can be turned on once validated.

## Usage

```
batch_splitter \
  --l1-batch <N> \
  --database-url <postgres://…> \
  --artifacts-path <object-store-base-path> \
  --l2-chain-id <id> \
  [--commitment-chaining]
```

No tree-DB path is needed — Merkle paths are reconstructed from N's witness blob.

Output: two blobs in the witness-input bucket, `airbender_split_input_{N}_a.cbor` and
`airbender_split_input_{N}_b.cbor`, each a CBOR-encoded `AirbenderVerifierInput`.

## Key references in the tree

- Full-batch assembly mirrored here: `core/node/airbender_proof_data_handler/src/airbender_request_processor.rs`
  (`airbender_verifier_input_for_existing_batch`).
- Re-execution pattern: `core/lib/airbender_verifier/src/lib.rs` (`verify`, `execute_vm`).
- Witness-data shape: `core/node/vm_runner/src/impls/bwip.rs`.
- Merkle hashing reused for the sparse reconstruction: `core/lib/merkle_tree/src/hasher/mod.rs` (`Blake2Hasher` as
  `HashTree`).
- Airbender commitment: `core/node/commitment_generator/src/{lib,utils}.rs`, `core/lib/types/src/commitment/mod.rs`
  (`airbender_artifacts`).
