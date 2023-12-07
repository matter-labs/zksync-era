# CodeDecommitter

## CodeDecommitter PI

### [Input](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/code_unpacker_sha256/input.rs#L80)

```rust
pub struct CodeDecommitterInputData<F: SmallField> {
    pub memory_queue_initial_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
    pub sorted_requests_queue_initial_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
}
```

### [Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/code_unpacker_sha256/input.rs#L100)

```rust
pub struct CodeDecommitterOutputData<F: SmallField> {
    pub memory_queue_final_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
}
```

### [FSM Input and FSM Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/code_unpacker_sha256/input.rs#L61)

```rust
pub struct CodeDecommitterFSMInputOutput<F: SmallField> {
    pub internal_fsm: CodeDecommittmentFSM<F>,
    pub decommittment_requests_queue_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
    pub memory_queue_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
}

pub struct CodeDecommittmentFSM<F: SmallField> {
    pub sha256_inner_state: [UInt32<F>; 8], // 8 uint32 words of internal sha256 state
    pub hash_to_compare_against: UInt256<F>,
    pub current_index: UInt32<F>,
    pub current_page: UInt32<F>,
    pub timestamp: UInt32<F>,
    pub num_rounds_left: UInt16<F>,
    pub length_in_bits: UInt32<F>,
    pub state_get_from_queue: Boolean<F>,
    pub state_decommit: Boolean<F>,
    pub finished: Boolean<F>,
}
```

## Main circuit logic

This circuit takes a queue of decommit requests for DecommitSorter circuit. For each decommit request, it checks that the linear hash of all opcodes will be equal to this hash that is stored in the decommit request. Also, it writes code to the corresponding memory page. Briefly, it unpacks the queue from the opcode and updates the memory queue and check correctness.

### [First part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/code_unpacker_sha256/mod.rs#L48)

The circuit begins with allocating input part of the PI.

```rust
let CodeDecommitterCircuitInstanceWitness {
    closed_form_input,
    sorted_requests_queue_witness,
    code_words,
} = witness;

let mut structured_input =
    CodeDecommitterCycleInputOutput::alloc_ignoring_outputs(cs, closed_form_input.clone());
```

We chose what `memory_queue` state and `deccomitments_queue` state to continue to work with.

```rust
let requests_queue_state = QueueState::conditionally_select(
    cs,
    structured_input.start_flag,
    &structured_input
        .observable_input
        .sorted_requests_queue_initial_state,
    &structured_input
        .hidden_fsm_input
        .decommittment_requests_queue_state,
);

let memory_queue_state = QueueState::conditionally_select(
    cs,
    structured_input.start_flag,
    &structured_input.observable_input.memory_queue_initial_state,
    &structured_input.hidden_fsm_input.memory_queue_state,
);
```

We do the same with inner FSM part.

```rust
let initial_state = CodeDecommittmentFSM::conditionally_select(
    cs,
    structured_input.start_flag,
    &starting_fsm_state,
    &structured_input.hidden_fsm_input.internal_fsm,
);
```

### [Main part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/code_unpacker_sha256/mod.rs#L168)

Here’s the part, where all the main logic is implemented. Firstly, we take a new decommit request if the queue is not empty yet.

```rust
let (may_be_new_request, _) =
    unpack_requests_queue.pop_front(cs, state.state_get_from_queue);
```

Then we update the state of the circuit.

```rust
state.num_rounds_left = UInt16::conditionally_select(
    cs,
    state.state_get_from_queue,
    &length_in_rounds,
    &state.num_rounds_left,
);
...
```

Then we create two write memory queries and push them to memory queue.

```rust
let mem_query_0 = MemoryQuery {
    timestamp: state.timestamp,
    memory_page: state.current_page,
    index: state.current_index,
    rw_flag: boolean_true,
    value: code_word_0,
    is_ptr: boolean_false,
};

let mem_query_1 = MemoryQuery {
    timestamp: state.timestamp,
    memory_page: state.current_page,
    index: state.current_index,
    rw_flag: boolean_true,
    value: code_word_1,
    is_ptr: boolean_false,
};

memory_queue.push(cs, mem_query_0, state.state_decommit);
memory_queue.push(cs, mem_query_1, process_second_word);
```

Now we create a new input for hash to be absorbed.

```rust
let mut sha256_input = [zero_u32; 16];
for (dst, src) in sha256_input.iter_mut().zip(
    code_word_0_be_bytes
        .array_chunks::<4>()
        .chain(code_word_1_be_bytes.array_chunks::<4>()),
) {
    *dst = UInt32::from_be_bytes(cs, *src);
}
```

And absorb it to current state.

```rust
let mut new_internal_state = state.sha256_inner_state;
round_function_over_uint32(cs, &mut new_internal_state, &sha256_input);
```

Also, we update current state.

```rust
state.sha256_inner_state = <[UInt32<F>; 8]>::conditionally_select(
    cs,
    state.state_decommit,
    &new_internal_state,
    &state.sha256_inner_state,
);
```

Finally, we check the hash if necessary.

```rust
for (part_of_first, part_of_second) in hash
    .inner
    .iter()
    .zip(state.hash_to_compare_against.inner.iter())
{
    Num::conditionally_enforce_equal(
        cs,
        finalize,
        &part_of_first.into_num(),
        &part_of_second.into_num(),
    );
}
```

### [Final part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/code_unpacker_sha256/mod.rs#L111)

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