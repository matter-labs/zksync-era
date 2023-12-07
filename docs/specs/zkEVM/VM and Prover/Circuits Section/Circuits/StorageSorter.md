# StorageSorter

## StorageSorter PI

### [Input](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/storage_validity_by_grand_product/input.rs#L84C57-L84C57)

```rust
pub struct StorageDeduplicatorInputData<F: SmallField> {
    pub shard_id_to_process: UInt8<F>,
    pub unsorted_log_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub intermediate_sorted_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
}
```

### [Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/storage_validity_by_grand_product/input.rs#L103)

```rust
pub struct StorageDeduplicatorOutputData<F: SmallField> {
    pub final_sorted_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
}
```

### [FSM Input and FSM Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/storage_validity_by_grand_product/input.rs#L37)

```rust
pub struct StorageDeduplicatorFSMInputOutput<F: SmallField> {
    pub lhs_accumulator: [Num<F>; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS],
    pub rhs_accumulator: [Num<F>; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS],
    pub current_unsorted_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub current_intermediate_sorted_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub current_final_sorted_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub cycle_idx: UInt32<F>,
    pub previous_packed_key: [UInt32<F>; PACKED_KEY_LENGTH],
    pub previous_key: UInt256<F>,
    pub previous_address: UInt160<F>,
    pub previous_timestamp: UInt32<F>,
    pub this_cell_has_explicit_read_and_rollback_depth_zero: Boolean<F>,
    pub this_cell_base_value: UInt256<F>,
    pub this_cell_current_value: UInt256<F>,
    pub this_cell_current_depth: UInt32<F>,
}
```

## Main circuit logic

The main logic of this circuit is sorting and deduplicating storage requests from `unsorted_log_queue_state`. The result storage requests are pushed to `final_sorted_queue_state`.

### [First part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/storage_validity_by_grand_product/mod.rs#L177)

We start, as usually, with allocating input fields from PI.

```rust
let mut structured_input = StorageDeduplicatorInputOutput::alloc_ignoring_outputs(
    cs,
    structured_input_witness.clone(),
);
```

In this part, we should decide what `unsorted_queue_state` to use (the one from `Input` or the other one from `FSM Input`). We do the same for sorted queue.

```rust
let state = QueueState::conditionally_select(
    cs,
    structured_input.start_flag,
    &unsorted_queue_from_passthrough_state,
    &unsorted_queue_from_fsm_input_state,
);

let state = QueueState::conditionally_select(
    cs,
    structured_input.start_flag,
    &intermediate_sorted_queue_from_passthrough.into_state(),
    &intermediate_sorted_queue_from_fsm_input.into_state(),
);
```

Also, we decide to create a new queue for the output, or continue working with the existing one.

```rust
let state = QueueState::conditionally_select(
    cs,
    structured_input.start_flag,
    &empty_final_sorted_queue.into_state(),
    &final_sorted_queue_from_fsm_input.into_state(),
);
```

Now we need to generate challenges for permutation argument.

```rust
let challenges = crate::utils::produce_fs_challenges::<
    F,
    CS,
    R,
    QUEUE_STATE_WIDTH,
    { TIMESTAMPED_STORAGE_LOG_ENCODING_LEN + 1 },
    DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS,
>(
    cs,
    structured_input
        .observable_input
        .unsorted_log_queue_state
        .tail,
    structured_input
        .observable_input
        .intermediate_sorted_queue_state
        .tail,
    round_function,
);
```

And decide whether we generate new accumulators for permutation argument or use existing ones.

```rust
let initial_lhs =
    <[Num<F>; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS]>::conditionally_select(
        cs,
        structured_input.start_flag,
        &[one; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS],
        &structured_input.hidden_fsm_input.lhs_accumulator,
    );

let initial_rhs =
    <[Num<F>; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS]>::conditionally_select(
        cs,
        structured_input.start_flag,
        &[one; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS],
        &structured_input.hidden_fsm_input.rhs_accumulator,
    );
```

### [Main part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/storage_validity_by_grand_product/mod.rs#L558)

Here we implement the main logic of the circuit. We run a cycle where on each iteration we try to pop a new element.

```rust
let (_, original_encoding) = original_queue.pop_front(cs, should_pop);
let (sorted_item, sorted_encoding) = intermediate_sorted_queue.pop_front(cs, should_pop);
```

Then we accumulate encodings for permutation argument. You can read more about it [here](https://github.com/code-423n4/2023-10-zksync/blob/c3ff020df5d11fe91209bd99d7fb0ec1272dc387/docs/Circuits%20Section/Circuits/Sorting.md).

```rust
for (((lhs_dst, rhs_dst), challenges), additive_part) in lhs
    .iter_mut()
    .zip(rhs.iter_mut())
    .zip(fs_challenges.iter())
    .zip(additive_parts.iter())
{
    lhs_lc.clear();
    rhs_lc.clear();

    for ((original_el, sorted_el), challenge) in extended_original_encoding
        .iter()
        .zip(sorted_encoding.iter())
        .zip(challenges.iter())
    {
        let lhs_contribution = original_el.mul(cs, &challenge);
        let rhs_contribution = sorted_el.mul(cs, &challenge);

        lhs_lc.push((lhs_contribution.get_variable(), F::ONE));
        rhs_lc.push((rhs_contribution.get_variable(), F::ONE));
    }

    lhs_lc.push((additive_part.get_variable(), F::ONE));
    rhs_lc.push((additive_part.get_variable(), F::ONE));

    let lhs_lc = Num::linear_combination(cs, &lhs_lc);
    let rhs_lc = Num::linear_combination(cs, &rhs_lc);

    let lhs_candidate = lhs_dst.mul(cs, &lhs_lc);
    let rhs_candidate = rhs_dst.mul(cs, &rhs_lc);

    *lhs_dst = Num::conditionally_select(cs, should_pop, &lhs_candidate, &*lhs_dst);
    *rhs_dst = Num::conditionally_select(cs, should_pop, &rhs_candidate, &*rhs_dst);
}
```

Now we enforce sorting.

```rust
previous_key_is_greater.conditionally_enforce_false(cs, not_item_is_trivial);
```

Maybe we should push the old query if the new key is different. So we push if at least one of these conditions holds:

- there was a read at depth 0;
- the sell is changes;
- write that was declined, but not by a rollback.

```rust
let query = LogQuery {
    address: previous_address,
    key: previous_key,
    read_value: this_cell_base_value,
    written_value: this_cell_current_value,
    rw_flag: should_write,
    aux_byte: UInt8::zero(cs),
    rollback: Boolean::allocated_constant(cs, false),
    is_service: Boolean::allocated_constant(cs, false),
    shard_id: shard_id_to_process,
    tx_number_in_block: UInt32::zero(cs),
    timestamp: UInt32::zero(cs),
};

sorted_queue.push(cs, query, should_push);
```

After that, we update some inner variables.

```rust
let meaningful_value = UInt256::conditionally_select(
    cs,
    record.rw_flag,
    &record.written_value,
    &record.read_value,
);

this_cell_base_value = UInt256::conditionally_select(
    cs,
    new_non_trivial_cell,
    &record.read_value,
    &this_cell_base_value,
);

...
```

Now we continue working with current query. We check that the read field is correct.

```rust
let read_is_equal_to_current =
    UInt256::equals(cs, &this_cell_current_value, &record.read_value);
read_is_equal_to_current.conditionally_enforce_true(cs, check_read_consistency);
```

After that, we do some other variable updates.

After the main cycle, we do one more iteration if we took the last query from the queue during the last cycle.

```rust
let query = LogQuery {
    address: previous_address,
    key: previous_key,
    read_value: this_cell_base_value,
    written_value: this_cell_current_value,
    rw_flag: should_write,
    aux_byte: UInt8::zero(cs),
    rollback: Boolean::allocated_constant(cs, false),
    is_service: Boolean::allocated_constant(cs, false),
    shard_id: shard_id_to_process,
    tx_number_in_block: UInt32::zero(cs),
    timestamp: UInt32::zero(cs),
};

sorted_queue.push(cs, query, should_push);
```

### [Final part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/storage_validity_by_grand_product/mod.rs#L424)

If the queues are empty, we check the permutation argument accumulators equality.

```rust
let completed = unsorted_is_empty.and(cs, sorted_is_empty);
new_lhs.iter().zip(new_rhs).for_each(|(l, r)| {
    Num::conditionally_enforce_equal(cs, completed, &l, &r);
});
```

Now we update PI output parts and compute a commitment. Then we allocate it as public variables.

```rust
let input_committment =
    commit_variable_length_encodable_item(cs, &compact_form, round_function);
for el in input_committment.iter() {
    let gate = PublicInputGate::new(el.get_variable());
    gate.add_to_cs(cs);
}
```
