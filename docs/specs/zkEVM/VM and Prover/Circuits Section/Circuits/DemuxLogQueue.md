# DemuxLogQueue

## DemuxLogQueue PI

### [Input](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/demux_log_queue/input.rs#L49)

```rust
pub struct LogDemuxerInputData<F: SmallField> {
    pub initial_log_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
}
```

### [Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/fsm_input_output/circuit_inputs/main_vm.rs#L33)

```rust
pub struct LogDemuxerOutputData<F: SmallField> {
    pub storage_access_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub events_access_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub l1messages_access_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub keccak256_access_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub sha256_access_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub ecrecover_access_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
}
```

### [FSM Input and FSM Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/demux_log_queue/input.rs#L22)

```rust
pub struct LogDemuxerFSMInputOutput<F: SmallField> {
    pub initial_log_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub storage_access_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub events_access_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub l1messages_access_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub keccak256_access_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub sha256_access_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub ecrecover_access_queue_state: QueueState<F, QUEUE_STATE_WIDTH>,
}
```

## Main circuit logic

The input of Log_Demuxer receives log_queue, consisting of a request to storage, events, L1messages request, and a request to the precompiles ecrecover, sha256, and keccak256. It divides this queue into six new queues. See our diagram.

### Start:

The function of circuits is `demultiplex_storage_logs_enty_point`.  We start for allocation of queue witnesses: 

```rust
let mut structured_input =
    LogDemuxerInputOutput::alloc_ignoring_outputs(cs, closed_form_input.clone());
```

Then we must verify that no elements have already been retrieved from the queue:

```rust
structured_input
    .observable_input
    .initial_log_queue_state
    .enforce_trivial_head(cs);
```

So long as `tail` is some equivalent of the merkel tree root and `head` is an equivalent of the current node hash, we provide some path witness when we pop elements and require that we properly end up in the root. So we must prove that element of head is zero:

```rust
pub fn enforce_trivial_head<CS: ConstraintSystem<F>>(&self, cs: &mut CS) {
    let zero_num = Num::zero(cs);
    for el in self.head.iter() {
        Num::enforce_equal(cs, el, &zero_num);
    }
}
```

Depends on `start_flag` we select which queue `observable_input` or `fsm_input`(internal intermediate queue) we took:

```rust
let state = QueueState::conditionally_select(
    cs,
    structured_input.start_flag,
    &structured_input.observable_input.initial_log_queue_state,
    &structured_input.hidden_fsm_input.initial_log_queue_state,
);
```

Wrap the state and witnesses in `StorageLogQueue`, thereby preparing the input data for `inner` part:

```rust
let mut initial_queue = StorageLogQueue::<F, R>::from_state(cs, state);
use std::sync::Arc;
let initial_queue_witness = CircuitQueueWitness::from_inner_witness(initial_queue_witness);
initial_queue.witness = Arc::new(initial_queue_witness);
```

For the rest, it selects between empty or from FSM: 

```rust
let queue_states_from_fsm = [
&structured_input.hidden_fsm_input.storage_access_queue_state,
&structured_input.hidden_fsm_input.events_access_queue_state,
&structured_input
    .hidden_fsm_input
    .l1messages_access_queue_state,
&structured_input
    .hidden_fsm_input
    .keccak256_access_queue_state,
&structured_input.hidden_fsm_input.sha256_access_queue_state,
&structured_input
    .hidden_fsm_input
    .ecrecover_access_queue_state,
];

let empty_state = QueueState::empty(cs);
let [mut storage_access_queue, mut events_access_queue, mut l1messages_access_queue, mut keccak256_access_queue, mut sha256_access_queue, mut ecrecover_access_queue] =
queue_states_from_fsm.map(|el| {
let state = QueueState::conditionally_select(
        cs,
        structured_input.start_flag,
        &empty_state,
        &el,
    );
    StorageLogQueue::<F, R>::from_state(cs, state)
});
```

Prepared all queues into `input_queues` and call `inner` part: 

```rust
demultiplex_storage_logs_inner(cs, &mut initial_queue, input_queues, limit);
```

The last step is to form the final state. The flag `completed` shows us if `initial_queue` is empty or not. If not, we fill fsm_output. If it is empty, we select observable_output for the different queues. 

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

### Inner part

This is the logic part of the circuit. It depends on the main queue `storage_log_queue`, which separates the other queues. After we have dealt with the initial precompile, we need to allocate constant addresses for `keccak_precompile_address`, `sha256_precompile_address`, `ecrecover_precompile_address` and allocate constants for `STORAGE_AUX_BYTE`, `EVENT_AUX_BYTE`, `L1_MESSAGE_AUX_BYTE`, `PRECOMPILE_AUX_BYTE`. Execution happens when we pop all elements from `storage_log_queue`. We have appropriate flags for this, which depend on each other: 

```rust
let queue_is_empty = storage_log_queue.is_empty(cs);
let execute = queue_is_empty.negated(cs);
```

Here, we choose flags depending on the popped element data: 

```rust
let is_storage_aux_byte = UInt8::equals(cs, &aux_byte_for_storage, &popped.0.aux_byte);
let is_event_aux_byte = UInt8::equals(cs, &aux_byte_for_event, &popped.0.aux_byte);
let is_l1_message_aux_byte =
    UInt8::equals(cs, &aux_byte_for_l1_message, &popped.0.aux_byte);
let is_precompile_aux_byte =
    UInt8::equals(cs, &aux_byte_for_precompile_call, &popped.0.aux_byte);

let is_keccak_address = UInt160::equals(cs, &keccak_precompile_address, &popped.0.address);
let is_sha256_address = UInt160::equals(cs, &sha256_precompile_address, &popped.0.address);
let is_ecrecover_address =
    UInt160::equals(cs, &ecrecover_precompile_address, &popped.0.address);
```

Put up the right flag for shards: 

```rust
let is_rollup_shard = popped.0.shard_id.is_zero(cs);
let is_porter_shard = is_rollup_shard.negated(cs);
```

Execute all and push them into output queues: 

```rust
let execute_rollup_storage = Boolean::multi_and(cs, &[is_storage_aux_byte, is_rollup_shard, execute]);
let execute_porter_storage = Boolean::multi_and(cs, &[is_storage_aux_byte, is_porter_shard, execute]);

let execute_event = Boolean::multi_and(cs, &[is_event_aux_byte, execute]);
let execute_l1_message = Boolean::multi_and(cs, &[is_l1_message_aux_byte, execute]);
let execute_keccak_call = Boolean::multi_and(cs, &[is_precompile_aux_byte, is_keccak_address, execute]);
let execute_sha256_call = Boolean::multi_and(cs, &[is_precompile_aux_byte, is_sha256_address, execute]);
let execute_ecrecover_call = Boolean::multi_and(cs, &[is_precompile_aux_byte, is_ecrecover_address, execute]);

let bitmask = [
    execute_rollup_storage,
    execute_event,
    execute_l1_message,
    execute_keccak_call,
    execute_sha256_call,
    execute_ecrecover_call,
];

push_with_optimize(
  cs,
  [
      rollup_storage_queue,
      events_queue,
      l1_messages_queue,
      keccak_calls_queue,
      sha256_calls_queue,
      ecdsa_calls_queue,
  ],
  bitmask,
  popped.0,
);
```

Note: since we do not have a porter, the flag is automatically set to `false`:

```rust
let boolean_false = Boolean::allocated_constant(cs, false);
Boolean::enforce_equal(cs, &execute_porter_storage, &boolean_false);
```