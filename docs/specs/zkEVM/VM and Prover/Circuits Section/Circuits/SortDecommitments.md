# SortDecommitments

## SortDecommitments PI

### [Input](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/sort_decommittment_requests/input.rs#L62)

```rust
pub struct CodeDecommittmentsDeduplicatorInputData<F: SmallField> {
    pub initial_queue_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
    pub sorted_queue_initial_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
}
```

### [Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/sort_decommittment_requests/input.rs#L81)

```rust
pub struct CodeDecommittmentsDeduplicatorOutputData<F: SmallField> {
    pub final_queue_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
}
```

### [FSM Input and FSM Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/sort_decommittment_requests/input.rs#L26)

```rust
pub struct CodeDecommittmentsDeduplicatorFSMInputOutput<F: SmallField> {
    pub initial_queue_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
    pub sorted_queue_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
    pub final_queue_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,

    pub lhs_accumulator: [Num<F>; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS],
    pub rhs_accumulator: [Num<F>; DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS],

    pub previous_packed_key: [UInt32<F>; PACKED_KEY_LENGTH],
    pub first_encountered_timestamp: UInt32<F>,
    pub previous_record: DecommitQuery<F>,
}
```

## Main circuit logic

This circuit handles the sorting and deduplication of code cancellation requests. Before starting, during the pre-start phase, the first decommiter queue is generated. To decommiter a code, the input will receive the hash root of the code, the length of the code, the code hash of the opcode, the number of opcodes and the code of the page. Next, it sorts the queue and, in the process, identifies and removes identical requests, serving as a filtering mechanism in case the same contract is called several times.

The detailed explanation of sorting and deduplicating can be found [here](https://github.com/code-423n4/2023-10-zksync/blob/c3ff020df5d11fe91209bd99d7fb0ec1272dc387/docs/Circuits%20Section/Circuits/Sorting.md).

### [First part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/sort_decommittment_requests/mod.rs#L51)

The circuit begins with allocating input part of the PI.

```rust
let CodeDecommittmentsDeduplicatorInstanceWitness {
    closed_form_input,
    initial_queue_witness,
    sorted_queue_witness,
} = witness;

let mut structured_input = CodeDecommittmentsDeduplicatorInputOutput::alloc_ignoring_outputs(
    cs,
    closed_form_input.clone(),
);
```

In this part, we should decide what `initial_queue_state` to use (the one from `Input` or the other one from `FSM Input`). We do the same for sorted queue.

```rust
let state = QueueState::conditionally_select(
    cs,
    structured_input.start_flag,
    &initial_queue_from_passthrough_state,
    &initial_log_queue_state_from_fsm_state,
);
```

Also, we decide to create a new result queue or use one from the previous circuit.

```rust
let state = QueueState::conditionally_select(
    cs,
    structured_input.start_flag,
    &empty_state,
    &final_sorted_queue_from_fsm_state,
);
```

Now we need to generate challenges for permutation argument.

```rust
let challenges = crate::utils::produce_fs_challenges::<
    F,
    CS,
    R,
    FULL_SPONGE_QUEUE_STATE_WIDTH,
    { DECOMMIT_QUERY_PACKED_WIDTH + 1 },
    DEFAULT_NUM_PERMUTATION_ARGUMENT_REPETITIONS,
>(
    cs,
    structured_input.observable_input.initial_queue_state.tail,
    structured_input
        .observable_input
        .sorted_queue_initial_state
        .tail,
    round_function,
);
```

And decide whether we generate new accumulators for permutation argument or use existing ones.

```rust
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

Also, we make other parts of FSM state based on `start_flag`.

```rust
let mut previous_record = DecommitQuery::conditionally_select(
    cs,
    structured_input.start_flag,
    &trivial_record,
    &structured_input.hidden_fsm_input.previous_record,
);

let mut previous_packed_key = <[UInt32<F>; PACKED_KEY_LENGTH]>::conditionally_select(
    cs,
    structured_input.start_flag,
    &[zero_u32; PACKED_KEY_LENGTH],
    &structured_input.hidden_fsm_input.previous_packed_key,
);

let mut first_encountered_timestamp = UInt32::conditionally_select(
    cs,
    structured_input.start_flag,
    &zero_u32,
    &structured_input
        .hidden_fsm_input
        .first_encountered_timestamp,
);
```

### [Main part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/sort_decommittment_requests/mod.rs#L234)

Here we implement the main logic of the circuit. We run a cycle where on each iteration we try to pop a new element.

```rust
let (_, original_encoding) = original_queue.pop_front(cs, should_pop);
let (sorted_item, sorted_encoding) = sorted_queue.pop_front(cs, should_pop);
```

We compute contribution to permutation argument accumulators.

```rust
for ((challenges, lhs), rhs) in fs_challenges.iter().zip(lhs.iter_mut()).zip(rhs.iter_mut())
{
		...
}
```

After, we enforce that elements from sorted queue are actually sorted.

```rust
new_key_is_greater.conditionally_enforce_true(cs, should_pop);
```

Also, we need to deduplicate some decommit requests if there are the same ones.

```rust
// decide if we should add the PREVIOUS into the queue
let add_to_the_queue = Boolean::multi_and(cs, &[previous_is_non_trivial, different_hash]);

result_queue.push(cs, record_to_add, add_to_the_queue);
```

Now we update inner variables.

```rust
previous_item_is_trivial = is_trivial;
// may be update the timestamp
*first_encountered_timestamp = UInt32::conditionally_select(
    cs,
    same_hash,
    &first_encountered_timestamp,
    &sorted_item.timestamp,
);
*previous_record = sorted_item;
*previous_packed_key = packed_key;
```

In the end, if the queues are empty, and we have taken the last element, we push it immediately.

```rust
let add_to_the_queue = Boolean::multi_and(cs, &[previous_is_non_trivial, completed]);

result_queue.push(cs, record_to_add, add_to_the_queue);
```

### [Final part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/sort_decommittment_requests/mod.rs#L191C1-L191C1)

We check that permutation accumulators are equal, if the queues are already empty.

```rust
for (lhs, rhs) in new_lhs.iter().zip(new_rhs.iter()) {
    Num::conditionally_enforce_equal(cs, completed, lhs, rhs);
}
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