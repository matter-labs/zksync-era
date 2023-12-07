# Sha256RoundFunction

## Sha256RoundFunction PI

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
pub struct Sha256RoundFunctionFSMInputOutput<F: SmallField> {
    pub internal_fsm: Sha256RoundFunctionFSM<F>,
    pub log_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub memory_queue_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
}

pub struct Sha256RoundFunctionFSM<F: SmallField> {
    pub read_precompile_call: Boolean<F>,
    pub read_words_for_round: Boolean<F>,
    pub completed: Boolean<F>,
    pub sha256_inner_state: [UInt32<F>; 8],
    pub timestamp_to_use_for_read: UInt32<F>,
    pub timestamp_to_use_for_write: UInt32<F>,
    pub precompile_call_params: Sha256PrecompileCallParams<F>,
}
```

## Main circuit logic

This is a precompile for the SHA256 hash function’s round function.

We start from witness allocation:

```rust
let Sha256RoundFunctionCircuitInstanceWitness {
        closed_form_input,
        requests_queue_witness,
        memory_reads_witness,
  } = witness;

let mut structured_input = Sha256RoundFunctionCircuitInputOutput::alloc_ignoring_outputs(
    cs,
    closed_form_input.clone(),
);

let start_flag = structured_input.start_flag;

let requests_queue_state_from_input = structured_input.observable_input.initial_log_queue_state;
```

Check if `requests_queue_state_from_input` is trivial ( we didn't pop elements yet) and choose between input and `fsm` queue state:

```rust
requests_queue_state_from_input.enforce_trivial_head(cs);

let requests_queue_state_from_fsm = structured_input.hidden_fsm_input.log_queue_state;

let requests_queue_state = QueueState::conditionally_select(
    cs,
    start_flag,
    &requests_queue_state_from_input,
    &requests_queue_state_from_fsm,
);
```

the same procedure we do for `memory_queue`:

```rust
let memory_queue_state_from_input =
      structured_input.observable_input.initial_memory_queue_state;

// it must be trivial
memory_queue_state_from_input.enforce_trivial_head(cs);

let memory_queue_state_from_fsm = structured_input.hidden_fsm_input.memory_queue_state;

let memory_queue_state = QueueState::conditionally_select(
    cs,
    start_flag,
    &memory_queue_state_from_input,
    &memory_queue_state_from_fsm,
);
```

Call `inner` part where is main logic:

```rust
let final_state = sha256_precompile_inner::<F, CS, R>(
        cs,
        &mut memory_queue,
        &mut requests_queue,
        read_queries_allocator,
        initial_state,
        round_function,
        limit,
    );
```

Form the final state (depending on flag we choose between states):

```rust

    let done = final_state.completed;
    structured_input.completion_flag = done;
    structured_input.observable_output = PrecompileFunctionOutputData::placeholder(cs);

    structured_input.observable_output.final_memory_state = QueueState::conditionally_select(
        cs,
        structured_input.completion_flag,
        &final_memory_state,
        &structured_input.observable_output.final_memory_state,
    );

    structured_input.hidden_fsm_output.internal_fsm = final_state;
    structured_input.hidden_fsm_output.log_queue_state = final_requets_state;
    structured_input.hidden_fsm_output.memory_queue_state = final_memory_state;
```

Finally, we compute a commitment to PublicInput and allocate it as witness variables.

```rust
let compact_form =
        ClosedFormInputCompactForm::from_full_form(cs, &structured_input, round_function);
    let input_commitment = commit_variable_length_encodable_item(cs, &compact_form, round_function);
    for el in input_commitment.iter() {
        let gate = PublicInputGate::new(el.get_variable());
        gate.add_to_cs(cs);
    }
```

### Inner part:

Start for set up different flags: `precompile_address`, `aux_byte_for_precompile`, and plugs: 

```rust
let precompile_address = UInt160::allocated_constant(
        cs,
        *zkevm_opcode_defs::system_params::SHA256_ROUND_FUNCTION_PRECOMPILE_FORMAL_ADDRESS,
  );
let aux_byte_for_precompile = UInt8::allocated_constant(cs, PRECOMPILE_AUX_BYTE);

let boolean_false = Boolean::allocated_constant(cs, false);
let boolean_true = Boolean::allocated_constant(cs, true);
let zero_u32 = UInt32::zero(cs);
let zero_u256 = UInt256::zero(cs);
```

We can have a degenerate case when the queue is empty, but it's the first circuit in the queue, so we take default `FSM` state that has `state.read_precompile_call = true`, we can only skip the full circuit if we are not in any form of progress:

```rust
let input_queue_is_empty = precompile_calls_queue.is_empty(cs);
let can_finish_immediatelly =
    Boolean::multi_and(cs, &[state.read_precompile_call, input_queue_is_empty]);
```

Main work cycle:

Check income data with constants(precompile addresses aux byte for precompile and must match):

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

Create parameters that describe the call itself:

```rust
let params_encoding = precompile_call.key;
let call_params = Sha256PrecompileCallParams::from_encoding(cs, params_encoding);

state.precompile_call_params = Sha256PrecompileCallParams::conditionally_select(
    cs,
    state.read_precompile_call,
    &call_params,
    &state.precompile_call_params,
);
```

- `input_page` – memory page for `read_queue`
- `input_offset` – page index`read_queue`
- `output_page` –  memory page for `write_queue`
- `output_offset` – page index`write_queue`
- `num_rounds` –  number of rounds for hash function

```rust
pub struct Sha256PrecompileCallParams<F: SmallField> {
    pub input_page: UInt32<F>,
    pub input_offset: UInt32<F>,
    pub output_page: UInt32<F>,
    pub output_offset: UInt32<F>,
    pub num_rounds: UInt32<F>,
}
```

Set up `timestamp:` 

```rust
state.timestamp_to_use_for_read = UInt32::conditionally_select(
            cs,
            state.read_precompile_call,
            &precompile_call.timestamp,
            &state.timestamp_to_use_for_read,
  );

// timestamps have large space, so this can be expected
let timestamp_to_use_for_write =
    unsafe { state.timestamp_to_use_for_read.increment_unchecked(cs) };
state.timestamp_to_use_for_write = UInt32::conditionally_select(
    cs,
    state.read_precompile_call,
    &timestamp_to_use_for_write,
    &state.timestamp_to_use_for_write,
);
```

Reset buffer if needed: 

```rust
let reset_buffer = Boolean::multi_or(cs, &[state.read_precompile_call, state.completed]);
state.read_words_for_round = Boolean::multi_or(
    cs,
    &[state.read_precompile_call, state.read_words_for_round],
);
state.read_precompile_call = boolean_false;
```

Now perform a few memory queries to read content:

```rust
let zero_rounds_left = state.precompile_call_params.num_rounds.is_zero(cs);

let mut memory_queries_as_u32_words = [zero_u32; 8 * MEMORY_READ_QUERIES_PER_CYCLE];
let should_read = zero_rounds_left.negated(cs);
let mut bias_variable = should_read.get_variable();
for dst in memory_queries_as_u32_words.array_chunks_mut::<8>() {
    let read_query_value =
        memory_read_witness.conditionally_allocate_biased(cs, should_read, bias_variable);
    bias_variable = read_query_value.inner[0].get_variable();

    let read_query = MemoryQuery {
        timestamp: state.timestamp_to_use_for_read,
        memory_page: state.precompile_call_params.input_page,
        index: state.precompile_call_params.input_offset,
        rw_flag: boolean_false,
        is_ptr: boolean_false,
        value: read_query_value,
    };

    let may_be_new_offset = unsafe {
        state
            .precompile_call_params
            .input_offset
            .increment_unchecked(cs)
    };
    state.precompile_call_params.input_offset = UInt32::conditionally_select(
        cs,
        state.read_words_for_round,
        &may_be_new_offset,
        &state.precompile_call_params.input_offset,
    );

    // perform read
    memory_queue.push(cs, read_query, should_read);
```

We need to change endianness. Memory is BE, and each of the 4-byte chunks should be interpreted as BE u32 for sha256:

```rust
let be_bytes = read_query_value.to_be_bytes(cs);
for (dst, src) in dst.iter_mut().zip(be_bytes.array_chunks::<4>()) {
    let as_u32 = UInt32::from_be_bytes(cs, *src);
    *dst = as_u32;
}
```

get the initial state for `SHA256`:

```rust
let sha256_empty_internal_state = sha256::ivs_as_uint32(cs);
let mut current_sha256_state = <[UInt32<F>; 8]>::conditionally_select(
            cs,
            reset_buffer,
            &sha256_empty_internal_state,
            &state.sha256_inner_state,
        );
```

finally, compute sha256 and write into memory if we completed all hash rounds. BTW `SHA256` algorithm you can read [here](https://eips.ethereum.org/assets/eip-2680/sha256-384-512.pdf):

```rust
let sha256_output = sha256::round_function::round_function_over_uint32(
            cs,
            &mut current_sha256_state,
            &memory_queries_as_u32_words,
);
state.sha256_inner_state = current_sha256_state;

let no_rounds_left = state.precompile_call_params.num_rounds.is_zero(cs);
let write_result = Boolean::multi_and(cs, &[state.read_words_for_round, no_rounds_left]);

let mut write_word = zero_u256;
// some endianess magic
for (dst, src) in write_word
  .inner
  .iter_mut()
  .rev()
  .zip(sha256_output.array_chunks::<4>())
{
  *dst = UInt32::from_le_bytes(cs, *src);
}

let write_query = MemoryQuery {
  timestamp: state.timestamp_to_use_for_write,
  memory_page: state.precompile_call_params.output_page,
  index: state.precompile_call_params.output_offset,
  rw_flag: boolean_true,
  is_ptr: boolean_false,
  value: write_word,
};
```

Update state: 

```rust
let input_is_empty = precompile_calls_queue.is_empty(cs);
let input_is_not_empty = input_is_empty.negated(cs);
let nothing_left = Boolean::multi_and(cs, &[write_result, input_is_empty]);
let process_next = Boolean::multi_and(cs, &[write_result, input_is_not_empty]);

state.read_precompile_call = process_next;
state.completed = Boolean::multi_or(cs, &[nothing_left, state.completed]);
let t = Boolean::multi_or(cs, &[state.read_precompile_call, state.completed]);
state.read_words_for_round = t.negated(cs);
```