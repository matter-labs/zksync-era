# LogSorter

`LogSorter` is one circuit that is used as both `EventsSorter` and `L1MessagesSorter`.

## LogSorter PI

### [Input](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/log_sorter/input.rs#L57)

```rust
pub struct EventsDeduplicatorInputData<F: SmallField> {
    pub initial_log_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub intermediate_sorted_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
}
```

### [Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/log_sorter/input.rs#L74)

```rust
pub struct EventsDeduplicatorOutputData<F: SmallField> {
    pub final_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
}
```

### [FSM Input and FSM Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/log_sorter/input.rs#L28)

```rust
pub struct EventsDeduplicatorFSMInputOutput<F: SmallField> {
    pub lhs_accumulator: [Num<F>; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS],
    pub rhs_accumulator: [Num<F>; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS],
    pub initial_unsorted_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub intermediate_sorted_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub final_result_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub previous_key: UInt32<F>,
    pub previous_item: LogQuery<F>,
}
```

## Main circuit logic

The main logic of this circuit is sorting and deduplicating logs from  `initial_log_queue_state`. The result is pushed to `final_queue_state`.

With sorting, we get 2 queues – a simple one, and a sorted one.

We start with the witness allocation:

```rust
let mut structured_input =
        EventsDeduplicatorInputOutput::alloc_ignoring_outputs(cs, closed_form_input.clone());
```

Now the scheme is familiar.

Check if we didn't take elements from the queue: 

```rust
unsorted_queue_from_passthrough_state.enforce_trivial_head(cs);
```

Judging by the flag, we choose a queue:

```rust
let state = QueueState::conditionally_select(
        cs,
        structured_input.start_flag,
        &unsorted_queue_from_passthrough_state,
        &unsorted_queue_from_fsm_input_state,
    );
```

Wrap the state and witnesses for it in `StorageLogQueue`, thereby preparing the input data for `inner`:

```rust
let mut unsorted_queue = StorageLogQueue::<F, R>::from_state(cs, state);

  use std::sync::Arc;
  let initial_queue_witness = CircuitQueueWitness::from_inner_witness(initial_queue_witness);
  unsorted_queue.witness = Arc::new(initial_queue_witness);

  let intermediate_sorted_queue_from_passthrough_state = structured_input
      .observable_input
      .intermediate_sorted_queue_state;
```

For `sorted_queue`, it is the same procedure.

We generate challenges and accumulators for the permutation argument. A detailed explanation can be found [here](https://github.com/code-423n4/2023-10-zksync/blob/c3ff020df5d11fe91209bd99d7fb0ec1272dc387/docs/Circuits%20Section/Circuits/Sorting.md).

```rust
let challenges = crate::utils::produce_fs_challenges::<
        F,
        CS,
        R,
        QUEUE_STATE_WIDTH,
        { MEMORY_QUERY_PACKED_WIDTH + 1 },
        DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS,
    >(
        cs,
        structured_input
            .observable_input
            .initial_log_queue_state
            .tail,
        structured_input
            .observable_input
            .intermediate_sorted_queue_state
            .tail,
        round_function,
    );
```

Again, if it is not the rest cycle (`start_flag == false`), we should choose fsm:

```rust
let one = Num::allocated_constant(cs, F::ONE);
let initial_lhs = Num::parallel_select(
    cs,
    structured_input.start_flag,
    &[one; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS],
    &structured_input.hidden_fsm_input.lhs_accumulator,
);

let initial_rhs = Num::parallel_select(
    cs,
    structured_input.start_flag,
    &[one; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS],
    &structured_input.hidden_fsm_input.rhs_accumulator,
);
```

Depending on the flag, we prepare all the information for `inner` part:

```rust
let zero_u32 = UInt32::zero(cs);
let previous_key = UInt32::conditionally_select(
    cs,
    structured_input.start_flag,
    &zero_u32,
    &structured_input.hidden_fsm_input.previous_key,
);
```

```rust
let empty_storage = LogQuery::placeholder(cs);
let previous_item = LogQuery::conditionally_select(
    cs,
    structured_input.start_flag,
    &empty_storage,
    &structured_input.hidden_fsm_input.previous_item,
);
```

After `inner` part we check `unsorted_queue` and `intermediate_sorted_queue`.:

```rust
let unsorted_is_empty = unsorted_queue.is_empty(cs);
let sorted_is_empty = intermediate_sorted_queue.is_empty(cs);

Boolean::enforce_equal(cs, &unsorted_is_empty, &sorted_is_empty);
```

We check that permutation accumulators are equal and if the queues are already empty:

```rust
let completed = unsorted_queue.length.is_zero(cs);
    for (lhs, rhs) in new_lhs.iter().zip(new_rhs.iter()) {
        Num::conditionally_enforce_equal(cs, completed, lhs, rhs);
    }
```

Finally, we compute a commitment to PublicInput and allocate it as witness variables.

```rust
let input_commitment = commit_variable_length_encodable_item(cs, &compact_form, round_function);
for el in input_commitment.iter() {
    let gate = PublicInputGate::new(el.get_variable());
    gate.add_to_cs(cs);
}
```

### Inner part:

Note: we have specific logic for rollback. When we have an event of some function and then that function makes a return then we should cancel this event. Inside the VM, we create exactly the same event: same key, block number, timestamp, etc. the only change is that the rollback flag is now true. In the inner part, first sort and look for these pairs and self-destruct them.

There are two cases: when `unsorted_queue` is empty, but it's the only circuit, in this case. Otherwise, we continue, and then it's not trivial.

```rust
let no_work = unsorted_queue.is_empty(cs);
let mut previous_is_trivial = Boolean::multi_or(cs, &[no_work, is_start]);
```

Additional checks for length. We should always check whether the sorted queue and the normal queue are of the same length.

```rust
let unsorted_queue_lenght = Num::from_variable(unsorted_queue.length.get_variable());
let intermediate_sorted_queue_lenght =
    Num::from_variable(intermediate_sorted_queue.length.get_variable());

Num::enforce_equal(
    cs,
    &unsorted_queue_lenght,
    &intermediate_sorted_queue_lenght,
);
```

We can pop elements if unsorted_queue is empty. That’s why every time we set up the flags `original_is_empty`, `sorted_is_empty`. We also ensure that items are "write" unless it's a padding.

```rust
let original_is_empty = unsorted_queue.is_empty(cs);
let sorted_is_empty = intermediate_sorted_queue.is_empty(cs);
Boolean::enforce_equal(cs, &original_is_empty, &sorted_is_empty);

let should_pop = original_is_empty.negated(cs);
let is_trivial = original_is_empty;

let (_, original_encoding) = unsorted_queue.pop_front(cs, should_pop);
let (sorted_item, sorted_encoding) = intermediate_sorted_queue.pop_front(cs, should_pop);
```

The next block of code is sorting. You can find the main idea [here](https://github.com/code-423n4/2023-10-zksync/blob/c3ff020df5d11fe91209bd99d7fb0ec1272dc387/docs/Circuits%20Section/Circuits/Sorting.md).

Check if keys are equal and check a value. We compare timestamps and then resolve logic over rollbacks, so the only way when keys are equal can be when we do a rollback. Ensure sorting for uniqueness timestamp and rollback flag. We know that timestamps are unique across logs, and are also the same between write and rollback. Keys are always ordered no matter what, and are never equal unless it's padding:

```rust
let sorting_key = sorted_item.timestamp;
let (keys_are_equal, new_key_is_smaller) =
                unpacked_long_comparison(cs, &[previous_key], &[sorting_key]);
new_key_is_smaller.conditionally_enforce_false(cs, should_pop);
```

There are only two cases when keys are equal:

- it's a padding element
- it's a rollback

It's enough to compare timestamps, as the VM circuit guarantees uniqueness if it's not a padding. Now ensure sorting:

```rust
let previous_is_not_rollback = previous_item.rollback.negated(cs);
let enforce_sequential_rollback = Boolean::multi_and(
    cs,
    &[previous_is_not_rollback, sorted_item.rollback, should_pop],
);
keys_are_equal.conditionally_enforce_true(cs, enforce_sequential_rollback);

let same_log = UInt32::equals(cs, &sorted_item.timestamp, &previous_item.timestamp);

let values_are_equal =
    UInt256::equals(cs, &sorted_item.written_value, &previous_item.written_value);

let negate_previous_is_trivial = previous_is_trivial.negated(cs);
let should_enforce = Boolean::multi_and(cs, &[same_log, negate_previous_is_trivial]);

values_are_equal.conditionally_enforce_true(cs, should_enforce);

let this_item_is_non_trivial_rollback =
    Boolean::multi_and(cs, &[sorted_item.rollback, should_pop]);
let negate_previous_item_rollback = previous_item.rollback.negated(cs);
let prevous_item_is_non_trivial_write = Boolean::multi_and(
    cs,
    &[negate_previous_item_rollback, negate_previous_is_trivial],
);
let is_sequential_rollback = Boolean::multi_and(
    cs,
    &[
        this_item_is_non_trivial_rollback,
        prevous_item_is_non_trivial_write,
    ],
);
same_log.conditionally_enforce_true(cs, is_sequential_rollback);
```

Decide if we should add the previous into the queue. We add only if the previous one is not trivial, it had a different key, and it wasn't rolled back:

```rust
let negate_same_log = same_log.and(cs, should_pop).negated(cs);
let add_to_the_queue = Boolean::multi_and(
    cs,
    &[
        negate_previous_is_trivial,
        negate_same_log,
        negate_previous_item_rollback,
    ],
);
```

Further, we don't need in our `LogQueue` some fields, so we just clean up:

```rust
let boolean_false = Boolean::allocated_constant(cs, false);
let query_to_add = LogQuery {
    address: previous_item.address,
    key: previous_item.key,
    read_value: UInt256::zero(cs),
    written_value: previous_item.written_value,
    rw_flag: boolean_false,
    aux_byte: UInt8::zero(cs),
    rollback: boolean_false,
    is_service: previous_item.is_service,
    shard_id: previous_item.shard_id,
    tx_number_in_block: previous_item.tx_number_in_block,
    timestamp: UInt32::zero(cs),
};
```

Finalization step - same way, check if the last item is not a rollback:

```rust
let now_empty = unsorted_queue.is_empty(cs);

let negate_previous_is_trivial = previous_is_trivial.negated(cs);
let negate_previous_item_rollback = previous_item.rollback.negated(cs);
let add_to_the_queue = Boolean::multi_and(
    cs,
    &[
        negate_previous_is_trivial,
        negate_previous_item_rollback,
        now_empty,
    ],
);
let boolean_false = Boolean::allocated_constant(cs, false);
let query_to_add = LogQuery {
    address: previous_item.address,
    key: previous_item.key,
    read_value: UInt256::zero(cs),
    written_value: previous_item.written_value,
    rw_flag: boolean_false,
    aux_byte: UInt8::zero(cs),
    rollback: boolean_false,
    is_service: previous_item.is_service,
    shard_id: previous_item.shard_id,
    tx_number_in_block: previous_item.tx_number_in_block,
    timestamp: UInt32::zero(cs),
};

result_queue.push(cs, query_to_add, add_to_the_queue);

unsorted_queue.enforce_consistency(cs);
intermediate_sorted_queue.enforce_consistency(cs);
```