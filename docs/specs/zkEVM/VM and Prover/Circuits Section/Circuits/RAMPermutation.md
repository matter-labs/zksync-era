# RAMPermutation

## RAMPermutation PI

### [Input](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/ram_permutation/input.rs#L27)

```rust
pub struct RamPermutationInputData<F: SmallField> {
    pub unsorted_queue_initial_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
    pub sorted_queue_initial_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
    pub non_deterministic_bootloader_memory_snapshot_length: UInt32<F>,
}
```

### Output

```rust
()
```

### [FSM Input and FSM Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/ram_permutation/input.rs#L52)

```rust
pub struct RamPermutationFSMInputOutput<F: SmallField> {
    pub lhs_accumulator: [Num<F>; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS],
    pub rhs_accumulator: [Num<F>; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS],
    pub current_unsorted_queue_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
    pub current_sorted_queue_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
    pub previous_sorting_key: [UInt32<F>; RAM_SORTING_KEY_LENGTH],
    pub previous_full_key: [UInt32<F>; RAM_FULL_KEY_LENGTH],
    pub previous_value: UInt256<F>,
    pub previous_is_ptr: Boolean<F>,
    pub num_nondeterministic_writes: UInt32<F>,
}
```

## Main circuit logic

The circuit starts [here](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/ram_permutation/mod.rs#L30). This function allocate PI inputs that call the inner function, where all the main logic is implemented. In the end, it forms the fsm output and compute PI commitment. The main purpose of this circuit is enforcing that memory queries are executed correctly.

### [First part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/ram_permutation/mod.rs#L43)

We start, as usually, with allocating input fields from PI.

```rust
let RamPermutationCircuitInstanceWitness {
    closed_form_input,
    unsorted_queue_witness,
    sorted_queue_witness,
} = closed_form_input_witness;

let mut structured_input =
    RamPermutationCycleInputOutput::alloc_ignoring_outputs(cs, closed_form_input.clone());

let start_flag = structured_input.start_flag;
let observable_input = structured_input.observable_input.clone();
let hidden_fsm_input = structured_input.hidden_fsm_input.clone();
```

Some field, like `unsorted_queue_initial_state` and `current_unsorted_queue_state` represents the same value. So we should decide whether we take a new queue from `Input` or continue working with current one from `FSM Input`. We use `start_flag` for such purpose.

```rust
let unsorted_queue_state = QueueState::conditionally_select(
    cs,
    start_flag,
    &observable_input.unsorted_queue_initial_state,
    &hidden_fsm_input.current_unsorted_queue_state,
);

let sorted_queue_state = QueueState::conditionally_select(
    cs,
    start_flag,
    &observable_input.sorted_queue_initial_state,
    &hidden_fsm_input.current_sorted_queue_state,
);
```

Also, we generate challenges and accumulators for permutation argument. The detailed explanation can be found [here](https://github.com/code-423n4/2023-10-zksync/blob/c3ff020df5d11fe91209bd99d7fb0ec1272dc387/docs/Circuits%20Section/Circuits/Sorting.md).

```rust
let fs_challenges = crate::utils::produce_fs_challenges(
    cs,
    observable_input.unsorted_queue_initial_state.tail,
    observable_input.sorted_queue_initial_state.tail,
    round_function,
);

let mut lhs = <[Num<F>; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS]>::conditionally_select(
    cs,
    start_flag,
    &[num_one; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS],
    &hidden_fsm_input.lhs_accumulator,
);
let mut rhs = <[Num<F>; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS]>::conditionally_select(
    cs,
    start_flag,
    &[num_one; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS],
    &hidden_fsm_input.rhs_accumulator,
);
```

### [Main part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/ram_permutation/mod.rs#L211)

We call the inner function, where the main logic is implemented.

Firstly, we check non-deterministic writes. These should be in the beginning of `sorted_queue`. We also count the number of such writes.

```rust
let is_nondeterministic_write = Boolean::multi_and(
    cs,
    &[
        can_pop,
        ts_is_zero,
        page_is_bootloader_heap,
        is_write,
        not_ptr,
    ],
);

*num_nondeterministic_writes = UInt32::conditionally_select(
    cs,
    is_nondeterministic_write,
    &num_nondeterministic_writes_incremented,
    &num_nondeterministic_writes,
);
```

For every new memory query from `sorted_queue` we enforce sorting by (`memory_page`, `index` and `timestamp`).

```rust
let sorting_key = [
        sorted_item.timestamp,
        sorted_item.index,
        sorted_item.memory_page,
    ];

let (_keys_are_equal, previous_key_is_smaller) =
    unpacked_long_comparison(cs, &sorting_key, previous_sorting_key);
```

Then, if the query is read one, we have two cases:

- should enforce that the value is the same as in the previous value, if it has the same `memory_page` and `index`
- should enforce that the value is zero otherwise

```rust
let value_equal = UInt256::equals(cs, &sorted_item.value, &previous_element_value);
let value_is_zero = UInt256::equals(cs, &sorted_item.value, &uint256_zero);
```

In the end, we compute permutation argument contributions to accumulators. The code is [here](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/ram_permutation/mod.rs#L363). The detailed explanation can be found [here](https://github.com/code-423n4/2023-10-zksync/blob/c3ff020df5d11fe91209bd99d7fb0ec1272dc387/docs/Circuits%20Section/Circuits/Sorting.md).

### [Final part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/ram_permutation/mod.rs#L159)

If the queues are empty now, that means that this instance should be the last one.

```rust
let completed = unsorted_queue.length.is_zero(cs); 
```

If so, we should check that permutation argument accumulators are equal and number of nondeterministic writes is correct.

```rust
for (lhs, rhs) in lhs.iter().zip(rhs.iter()) {
    Num::conditionally_enforce_equal(cs, completed, lhs, rhs);
}

let num_nondeterministic_writes_equal = UInt32::equals(
    cs,
    &num_nondeterministic_writes,
    &observable_input.non_deterministic_bootloader_memory_snapshot_length,
);
num_nondeterministic_writes_equal.conditionally_enforce_true(cs, completed);
```

Finally, we form the output part of PI and compute a commitment to PI and allocate it as witness variables.

```rust
let input_commitment = commit_variable_length_encodable_item(cs, &compact_form, round_function);
for el in input_commitment.iter() {
    let gate = PublicInputGate::new(el.get_variable());
    gate.add_to_cs(cs);
}
```