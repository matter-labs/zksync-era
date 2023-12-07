# StorageApplication

## StorageApplication PI

### [Input](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/storage_application/input.rs#L56)

```rust
pub struct StorageApplicationInputData<F: SmallField> {
    pub shard: UInt8<F>,
    pub initial_root_hash: [UInt8<F>; 32],
    pub initial_next_enumeration_counter: [UInt32<F>; 2],
    pub storage_application_log_state: QueueState<F, QUEUE_STATE_WIDTH>,
}
```

### [Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/storage_application/input.rs#L77)

```rust
pub struct StorageApplicationOutputData<F: SmallField> {
    pub new_root_hash: [UInt8<F>; 32],
    pub new_next_enumeration_counter: [UInt32<F>; 2],
    pub state_diffs_keccak256_hash: [UInt8<F>; 32],
}
```

### [FSM Input and FSM Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/storage_application/input.rs#L29)

```rust
pub struct StorageApplicationFSMInputOutput<F: SmallField> {
    pub current_root_hash: [UInt8<F>; 32],
    pub next_enumeration_counter: [UInt32<F>; 2],
    pub current_storage_application_log_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub current_diffs_keccak_accumulator_state:
        [[[UInt8<F>; keccak256::BYTES_PER_WORD]; keccak256::LANE_WIDTH]; keccak256::LANE_WIDTH],
}
```

## Main circuit logic

This circuit takes storage requests from `storage_application_log_state`. Then for each query, it verifies the read value and updates the `root_hash` is needed. Also, it outputs the hash of storage diffs. Shard_id if enforces to be 0 for now, because we have only one shard.

### [First part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/storage_application/mod.rs#L281)

The circuit begins with allocating input part of the PI.

```rust
let StorageApplicationCircuitInstanceWitness {
    closed_form_input,
    storage_queue_witness,
    merkle_paths,
    leaf_indexes_for_reads,
} = witness;

let mut structured_input =
    StorageApplicationInputOutput::alloc_ignoring_outputs(cs, closed_form_input.clone());
```

We chose what `storage_application_log_state`, `root_hash` and other fields to continue to work with.

```rust
let mut current_root_hash = UInt8::<F>::parallel_select(
    cs,
    start_flag,
    &structured_input.observable_input.initial_root_hash,
    &structured_input.hidden_fsm_input.current_root_hash,
);

let storage_accesses_queue_state = QueueState::conditionally_select(
    cs,
    start_flag,
    &storage_queue_state_from_input,
    &storage_queue_state_from_fsm,
);

...
```

### [Main part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/storage_application/mod.rs#L393)

Here’s the part, where all the main logic is implemented. Firstly, we take a new storage request if needed.

```rust
let (storage_log, _) = storage_accesses_queue.pop_front(cs, parse_next_queue_elem);
```

Now we can parse it and do some checks.

```rust
let LogQuery {
    address,
    key,
    read_value,
    written_value,
    rw_flag,
    shard_id,
    ..
} = storage_log;
```

We need a merkle path for executing query.

```rust
for _ in 0..STORAGE_DEPTH {
    let wit = merkle_path_witness_allocator.conditionally_allocate_biased(
        cs,
        parse_next_queue_elem,
        bias_variable,
    );
    bias_variable = wit.inner[0].get_variable();
    new_merkle_path_witness.push(wit);
}
```

Also, we update `state_diffs` data.

```rust
state_diff_data.address = UInt8::parallel_select(
    cs,
    parse_next_queue_elem,
    &address_bytes,
    &state_diff_data.address,
);
state_diff_data.key =
    UInt8::parallel_select(cs, parse_next_queue_elem, &key_bytes, &state_diff_data.key);
state_diff_data.derived_key = UInt8::parallel_select(
    cs,
    parse_next_queue_elem,
    &derived_key,
    &state_diff_data.derived_key,
);
...
```

Finally, we compute a new merkel path.

```rust
let mut current_hash = blake2s(cs, &leaf_bytes);

for (path_bit, path_witness) in path_selectors
    .into_iter()
    .zip(merkle_path_witness.into_iter())
{
    let left = UInt8::parallel_select(cs, path_bit, &path_witness, &current_hash);
    let right = UInt8::parallel_select(cs, path_bit, &current_hash, &path_witness);
    let mut input = [zero_u8; 64];
    input[0..32].copy_from_slice(&left);
    input[32..64].copy_from_slice(&right);

    current_hash = blake2s(cs, &input);
}
```

If it was a write request, then we update the `root_hash`. Otherwise, we enforce that it’s still the same.

```rust
current_root_hash = UInt8::parallel_select(
    cs,
    write_stage_in_progress,
    &current_hash,
    &current_root_hash,
);

for (a, b) in current_root_hash.iter().zip(current_hash.iter()) {
    Num::conditionally_enforce_equal(
        cs,
        should_compare_roots,
        &Num::from_variable(a.get_variable()),
        &Num::from_variable(b.get_variable()),
    );
}
```

In the end, we update `state_diffs` state.

```rust
for block in
    extended_state_diff_encoding.array_chunks::<{ keccak256::KECCAK_RATE_BYTES }>()
{
    keccak256_conditionally_absorb_and_run_permutation(
        cs,
        write_stage_in_progress,
        &mut diffs_keccak_accumulator_state,
        block,
    );
}
```

### [Final part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/storage_application/mod.rs#L643)

We need to run padding and one more permutation for final output.

```rust
keccak256_conditionally_absorb_and_run_permutation(
    cs,
    boolean_true,
    &mut diffs_keccak_accumulator_state,
    &padding_block,
);
```

Now we update PI output parts and compute a commitment. Then we allocate it as public variables.

```rust
let compact_form =
    ClosedFormInputCompactForm::from_full_form(cs, &structured_input, round_function);
let input_commitment = commit_variable_length_encodable_item(cs, &compact_form, round_function);
for el in input_commitment.iter() {
    let gate = PublicInputGate::new(el.get_variable());
    gate.add_to_cs(cs);
}
```