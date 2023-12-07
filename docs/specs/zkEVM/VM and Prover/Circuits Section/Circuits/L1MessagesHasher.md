# L1MessagesHasher

## L1MessagesHasher PI

### [Input](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/linear_hasher/input.rs#L27)

```rust
pub struct LinearHasherInputData<F: SmallField> {
    pub queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
}
```

### [Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/linear_hasher/input.rs#L42)

```rust
pub struct LinearHasherOutputData<F: SmallField> {
    pub keccak256_hash: [UInt8<F>; 32],
}
```

### FSM Input and FSM Output

```rust
() // this circuit has big capacity, so we don't need several instances
```

## Main circuit logic

It takes a queue of L1 messages and hash everything with keccak.

The main logic is implemented in `linear_hasher_entry_point` function [here](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/linear_hasher/mod.rs#L35).

It can be spited into 3 parts: 

### [First part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/linear_hasher/mod.rs#L54)

Firstly, we allocate the “input” part of PI (`start flag`, `Input` and `FSM Input`):

```rust
let mut structured_input =
    LinearHasherInputOutput::alloc_ignoring_outputs(cs, closed_form_input.clone());

let start_flag = structured_input.start_flag;
let queue_state_from_input = structured_input.observable_input.queue_state;

let mut queue = StorageLogQueue::<F, R>::from_state(cs, queue_state_from_input);
let queue_witness = CircuitQueueWitness::from_inner_witness(queue_witness);
queue.witness = Arc::new(queue_witness);
```

Also, we do some checks for them and allocate empty hash state:

```rust
let keccak_accumulator_state =
    [[[zero_u8; keccak256::BYTES_PER_WORD]; keccak256::LANE_WIDTH]; keccak256::LANE_WIDTH];

let mut keccak_accumulator_state =
    keccak_accumulator_state.map(|el| el.map(|el| el.map(|el| el.get_variable())));
```

### [Main part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/linear_hasher/mod.rs#L105)

This part is the main one. We run a loop with some limit, where on each iteration we try to pop the next element from the queue, if it’s not empty.

```rust
let queue_is_empty = queue.is_empty(cs);
let should_pop = queue_is_empty.negated(cs);

let (storage_log, _) = queue.pop_front(cs, should_pop);
```

Then we absorb it to the buffer, and if it’s full we run a round function.

```rust
if buffer.len() >= 136 {
    let buffer_for_round: [UInt8<F>; KECCAK_RATE_BYTES] = buffer[..136].try_into().unwrap();
    let buffer_for_round = buffer_for_round.map(|el| el.get_variable());
    let carry_on = buffer[136..].to_vec();

    buffer = carry_on;

    // absorb if we are not done yet
    keccak256_conditionally_absorb_and_run_permutation(
        cs,
        continue_to_absorb,
        &mut keccak_accumulator_state,
        &buffer_for_round,
    );
}
```

If this element was the last one, we create a padding and run a round function.

```rust
if tail_len == KECCAK_RATE_BYTES - 1 {
    // unreachable, but we set it for completeness
    last_round_buffer[tail_len] = UInt8::allocated_constant(cs, 0x81);
} else {
    last_round_buffer[tail_len] = UInt8::allocated_constant(cs, 0x01);
    last_round_buffer[KECCAK_RATE_BYTES - 1] = UInt8::allocated_constant(cs, 0x80);
}

let last_round_buffer = last_round_buffer.map(|el| el.get_variable());

// absorb if it's the last round
keccak256_conditionally_absorb_and_run_permutation(
    cs,
    absorb_as_last_round,
    &mut keccak_accumulator_state,
    &last_round_buffer,
);
```

### [Final part](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/linear_hasher/mod.rs#L169)

Firstly, we verify that the queue is empty now.

```rust
let completed = queue.is_empty(cs);
Boolean::enforce_equal(cs, &completed, &boolean_true);
```

Then we compute the final hash and create an output.

```rust
// squeeze
let mut keccak256_hash = [MaybeUninit::<UInt8<F>>::uninit(); keccak256::KECCAK256_DIGEST_SIZE];
for (i, dst) in keccak256_hash.array_chunks_mut::<8>().enumerate() {
    for (dst, src) in dst.iter_mut().zip(keccak_accumulator_state[i][0].iter()) {
        let tmp = unsafe { UInt8::from_variable_unchecked(*src) };
        dst.write(tmp);
    }
}

let mut observable_output = LinearHasherOutputData::placeholder(cs);
observable_output.keccak256_hash = keccak256_hash;
```

Finally, we compute a commitment to PI and allocate it as witness variables.

```rust
let input_commitment = commit_variable_length_encodable_item(cs, &compact_form, round_function);
for el in input_commitment.iter() {
    let gate = PublicInputGate::new(el.get_variable());
    gate.add_to_cs(cs);
}
```