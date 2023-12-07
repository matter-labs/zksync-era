# KeccakRoundFunction

## KeccakRoundFunction PI

### [Input](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/fsm_input_output/circuit_inputs/main_vm.rs#L9)

```rust
pub struct PrecompileFunctionInputData<F: SmallField> {
    pub initial_log_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub initial_memory_queue_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
}
```

### [Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/base_structures/precompile_input_outputs/mod.rs#L42)

```rust
pub struct PrecompileFunctionOutputData<F: SmallField> {
    pub final_memory_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
}
```

### [FSM Input and FSM Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/keccak256_round_function/input.rs#L59)

```rust
pub struct Keccak256RoundFunctionFSMInputOutput<F: SmallField> {
    pub internal_fsm: Keccak256RoundFunctionFSM<F>,
    pub log_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub memory_queue_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
}

pub struct Keccak256RoundFunctionFSM<F: SmallField> {
    pub read_precompile_call: Boolean<F>,
    pub read_unaligned_words_for_round: Boolean<F>,
    pub completed: Boolean<F>,
    pub keccak_internal_state: [[[UInt8<F>; BYTES_PER_WORD]; LANE_WIDTH]; LANE_WIDTH],
    pub timestamp_to_use_for_read: UInt32<F>,
    pub timestamp_to_use_for_write: UInt32<F>,
    pub precompile_call_params: Keccak256PrecompileCallParams<F>,
    pub u8_words_buffer: [UInt8<F>; BYTES_BUFFER_SIZE],
    pub u64_words_buffer_markers: [Boolean<F>; BUFFER_SIZE_IN_U64_WORDS],
}
```

## Main circuit logic

Keccak is a precompile for the keccak hash function, and is responsible for hashing any input data sent in by contract executions. Roughly speaking, the keccak circuit will receive metadata about queued up precompile calls, and ensure that the first-in-line call is indeed a call to the keccak precompile. The circuit then collects some metadata about the call itself, which tells the circuit at which memory position the input can be found, and at which memory position the output should be written, along with some peripheral data like the timestamp of the hash.

Next, the circuit will take data from another queue, which contains memory queries. This will give the circuit witnesses to push into the keccak buffer.

Learn more about Keccak here: https://keccak.team/keccak.html.

### [First part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/keccak256_round_function/mod.rs#L423)

The circuit begins with allocating input part of the PI.

```rust
let Keccak256RoundFunctionCircuitInstanceWitness {
    closed_form_input,
    requests_queue_witness,
    memory_reads_witness,
} = witness;

let mut structured_input = Keccak256RoundFunctionCircuitInputOutput::alloc_ignoring_outputs(
    cs,
    closed_form_input.clone(),
);
```

We chose what `memory_queue` state and `log_queue` state to continue to work with.

```rust
let requests_queue_state = QueueState::conditionally_select(
    cs,
    start_flag,
    &requests_queue_state_from_input,
    &requests_queue_state_from_fsm,
);

let memory_queue_state = QueueState::conditionally_select(
    cs,
    start_flag,
    &memory_queue_state_from_input,
    &memory_queue_state_from_fsm,
);
```

We do the same with inner FSM part.

```rust
let initial_state = Keccak256RoundFunctionFSM::conditionally_select(
    cs,
    start_flag,
    &starting_fsm_state,
    &structured_input.hidden_fsm_input.internal_fsm,
);
```

### [Main part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/keccak256_round_function/mod.rs#L114)

Our main cycle starts with getting a new precompile request from the queue.

```rust
let (precompile_call, _) = precompile_calls_queue.pop_front(cs, state.read_precompile_call);
```

We check that fields are correct.

```rust
Num::conditionally_enforce_equal(
    cs,
    state.read_precompile_call,
    &Num::from_variable(precompile_call.aux_byte.get_variable()),
    &Num::from_variable(aux_byte_for_precompile.get_variable()),
);
for (a, b) in precompile_call
    .address
    .inner
    .iter()
    .zip(precompile_address.inner.iter())
{
    Num::conditionally_enforce_equal(
        cs,
        state.read_precompile_call,
        &Num::from_variable(a.get_variable()),
        &Num::from_variable(b.get_variable()),
    );
}
```

Also, we prepare some additional information for the call.

```rust
state.precompile_call_params = Keccak256PrecompileCallParams::conditionally_select(
    cs,
    state.read_precompile_call,
    &call_params,
    &state.precompile_call_params,
);
...
```

Then we do some memory queries to read data that needed to be hashed.

```rust
let read_query = MemoryQuery {
    timestamp: state.timestamp_to_use_for_read,
    memory_page: state.precompile_call_params.input_page,
    index: state.precompile_call_params.input_offset,
    rw_flag: boolean_false,
    is_ptr: boolean_false,
    value: read_query_value,
};

memory_queue.push(cs, read_query, should_read);
```

After some another preparations, we are ready to create a full input.

```rust
let mut input = [zero_u8; keccak256::KECCAK_RATE_BYTES];
    input.copy_from_slice(&state.u8_words_buffer[..keccak256::KECCAK_RATE_BYTES]);
```

And run the round function.

```rust
let squeezed =
    keccak256_absorb_and_run_permutation(cs, &mut state.keccak_internal_state, &input);
```

Now, if it was the last round, we can make a write memory query of the result.

```rust
let write_query = MemoryQuery {
    timestamp: state.timestamp_to_use_for_write,
    memory_page: state.precompile_call_params.output_page,
    index: state.precompile_call_params.output_offset,
    rw_flag: boolean_true,
    is_ptr: boolean_false,
    value: result,
};

memory_queue.push(cs, write_query, write_result);
```

### [Final part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/keccak256_round_function/mod.rs#L495)

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