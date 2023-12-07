# Main Vm

## MainVm PI

### [Input](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/fsm_input_output/circuit_inputs/main_vm.rs#L9)

```rust
pub struct VmInputData<F: SmallField> {
    pub rollback_queue_tail_for_block: [Num<F>; QUEUE_STATE_WIDTH],
    pub memory_queue_initial_state: QueueTailState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
    pub decommitment_queue_initial_state: QueueTailState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
    pub per_block_context: GlobalContext<F>,
}
```

### [Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/fsm_input_output/circuit_inputs/main_vm.rs#L33)

```rust
pub struct VmOutputData<F: SmallField> {
    pub log_queue_final_state: QueueState<F, QUEUE_STATE_WIDTH>,
    pub memory_queue_final_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
    pub decommitment_queue_final_state: QueueState<F, FULL_SPONGE_QUEUE_STATE_WIDTH>,
}
```

### [FSM Input and FSM Output](https://github.com/matter-labs/era-zkevm_circuits/blob/main/src/base_structures/vm_state/mod.rs#L92)

```rust
pub struct VmLocalState<F: SmallField> {
    pub previous_code_word: UInt256<F>,
    pub registers: [VMRegister<F>; REGISTERS_COUNT],
    pub flags: ArithmeticFlagsPort<F>,
    pub timestamp: UInt32<F>,
    pub memory_page_counter: UInt32<F>,
    pub tx_number_in_block: UInt32<F>,
    pub previous_code_page: UInt32<F>,
    pub previous_super_pc: UInt16<F>,
    pub pending_exception: Boolean<F>,
    pub ergs_per_pubdata_byte: UInt32<F>,
    pub callstack: Callstack<F>,
    pub memory_queue_state: [Num<F>; FULL_SPONGE_QUEUE_STATE_WIDTH],
    pub memory_queue_length: UInt32<F>,
    pub code_decommittment_queue_state: [Num<F>; FULL_SPONGE_QUEUE_STATE_WIDTH],
    pub code_decommittment_queue_length: UInt32<F>,
    pub context_composite_u128: [UInt32<F>; 4],
}
```

## Main circuit logic

Main_vm – is instruction handler. VM circuit only accumulated memory queries using WITNESS provided by (presumably honest) prover. In this sense VM is “local” - it doesn’t have access to full memory space, but only to values of particular queries that it encountered during the execution. RAM circuit sorts all accumulated queries from VM and ENFORCES the general RAM validity as described above. Those two actions together guarantee RAM validity, so for all the descriptions below when we will talk about particular opcodes in VM we will use a language like “Operand number 0 is read from the stack at the offset X” that means that even though such “memory read” technically means using a witness provided by the prover, in practice we can assume that such witness is correct and we can view it as just normal RAM access as one would expect to happen on the standard machine.

We start with the allocation witnesses: 

```rust
let VmCircuitWitness {
        closed_form_input,
        witness_oracle,
    } = witness;

    let mut structured_input =
        VmCircuitInputOutput::alloc_ignoring_outputs(cs, closed_form_input.clone());

    let start_flag = structured_input.start_flag;
    let observable_input = structured_input.observable_input.clone();
    let hidden_fsm_input = structured_input.hidden_fsm_input.clone();

    let VmInputData {
        rollback_queue_tail_for_block,
        memory_queue_initial_state,
        decommitment_queue_initial_state,
        per_block_context,
    } = observable_input;
```

We also need to create the state that reflects the "initial" state for boot process:

```rust
let bootloader_state = initial_bootloader_state(
        cs,
        memory_queue_initial_state.length,
        memory_queue_initial_state.tail,
        decommitment_queue_initial_state.length,
        decommitment_queue_initial_state.tail,
        rollback_queue_tail_for_block,
        round_function,
    );
```

but depending from `start_flag` we should select between states:

```rust
let mut state =
      VmLocalState::conditionally_select(cs, start_flag, &bootloader_state, &hidden_fsm_input);

let synchronized_oracle = SynchronizedWitnessOracle::new(witness_oracle);
```

Here we run the `vm_cycle` :

```rust
for _cycle_idx in 0..limit {
        state = vm_cycle(
            cs,
            state,
            &synchronized_oracle,
            &per_block_context,
            round_function,
        );
    }
```

The VM runs in cycles. For each cycle, 

1. Start in a prestate - perform all common operations for every opcode, namely deal with exceptions, resources, edge cases like end of execution, select opcods, compute common values. Within the zkEVM framework, numerous entities identified as "opcodes" in the EVM paradigm are elegantly manifested as mere function calls. This modification is rooted in the succinct observation that, from the perspective of an external caller, an inlined function (analogous to an opcode) is inherently indistinguishable from an internal function call.

```rust
let (draft_next_state, common_opcode_state, opcode_carry_parts) =
        create_prestate(cs, current_state, witness_oracle, round_function);
```

1. Compute state diffs for every opcode. List of opcodes:

```rust
pub enum Opcode {
    Invalid(InvalidOpcode),
    Nop(NopOpcode),
    Add(AddOpcode),
    Sub(SubOpcode),
    Mul(MulOpcode),
    Div(DivOpcode),
    Jump(JumpOpcode),
    Context(ContextOpcode),
    Shift(ShiftOpcode),
    Binop(BinopOpcode),
    Ptr(PtrOpcode),
    NearCall(NearCallOpcode),
    Log(LogOpcode),
    FarCall(FarCallOpcode),
    Ret(RetOpcode),
    UMA(UMAOpcode),
}
```

 VM cycle calls such functions for different class of opcodes: nop, add_sup, jump, bind, context, ptr, log, calls_and_ret, mul_div.

 Here we briefly mention all opcodes defined in the system. Each logical "opcode" comes with modifiers, categorized into "exclusive" modifiers (where only one can be applied) and "flags" or "non-exclusive" modifiers (where multiple can be activated simultaneously). The number of permissible "flags" can vary depending on the specific "exclusive" modifier chosen. All data from opcodes we write to StateDiffsAccumulator:

```rust
pub struct StateDiffsAccumulator<F: SmallField> {
    // dst0 candidates
    pub dst_0_values: Vec<(bool, Boolean<F>, VMRegister<F>)>,
    // dst1 candidates
    pub dst_1_values: Vec<(Boolean<F>, VMRegister<F>)>,
    // flags candidates
    pub flags: Vec<(Boolean<F>, ArithmeticFlagsPort<F>)>,
    // specific register updates
    pub specific_registers_updates: [Vec<(Boolean<F>, VMRegister<F>)>; REGISTERS_COUNT],
    // zero out specific registers
    pub specific_registers_zeroing: [Vec<Boolean<F>>; REGISTERS_COUNT],
    // remove ptr markers on specific registers
    pub remove_ptr_on_specific_registers: [Vec<Boolean<F>>; REGISTERS_COUNT],
    // pending exceptions, to be resolved next cycle. Should be masked by opcode applicability already
    pub pending_exceptions: Vec<Boolean<F>>,
    // ergs left, PC
    // new ergs left if it's not one available after decoding
    pub new_ergs_left_candidates: Vec<(Boolean<F>, UInt32<F>)>,
    // new PC in case if it's not just PC+1
    pub new_pc_candidates: Vec<(Boolean<F>, UInt16<F>)>,
    // other meta parameters of VM
    pub new_tx_number: Option<(Boolean<F>, UInt32<F>)>,
    pub new_ergs_per_pubdata: Option<(Boolean<F>, UInt32<F>)>,
    // memory bouds
    pub new_heap_bounds: Vec<(Boolean<F>, UInt32<F>)>,
    pub new_aux_heap_bounds: Vec<(Boolean<F>, UInt32<F>)>,
    // u128 special register, one from context, another from call/ret
    pub context_u128_candidates: Vec<(Boolean<F>, [UInt32<F>; 4])>,
    // internal machinery
    pub callstacks: Vec<(Boolean<F>, Callstack<F>)>,
    // memory page counter
    pub memory_page_counters: Option<UInt32<F>>,
    // decommittment queue
    pub decommitment_queue_candidates: Option<(
        Boolean<F>,
        UInt32<F>,
        [Num<F>; FULL_SPONGE_QUEUE_STATE_WIDTH],
    )>,
    // memory queue
    pub memory_queue_candidates: Vec<(
        Boolean<F>,
        UInt32<F>,
        [Num<F>; FULL_SPONGE_QUEUE_STATE_WIDTH],
    )>,
    // forward piece of log queue
    pub log_queue_forward_candidates: Vec<(Boolean<F>, UInt32<F>, [Num<F>; QUEUE_STATE_WIDTH])>,
    // rollback piece of log queue
    pub log_queue_rollback_candidates: Vec<(Boolean<F>, UInt32<F>, [Num<F>; QUEUE_STATE_WIDTH])>,
    // sponges to run. Should not include common sponges for src/dst operands
    pub sponge_candidates_to_run: Vec<(
        bool,
        bool,
        Boolean<F>,
        ArrayVec<
            (
                Boolean<F>,
                [Num<F>; FULL_SPONGE_QUEUE_STATE_WIDTH],
                [Num<F>; FULL_SPONGE_QUEUE_STATE_WIDTH],
            ),
            MAX_SPONGES_PER_CYCLE,
        >,
    )>,
    // add/sub relations to enforce
    pub add_sub_relations: Vec<(
        Boolean<F>,
        ArrayVec<AddSubRelation<F>, MAX_ADD_SUB_RELATIONS_PER_CYCLE>,
    )>,
    // mul/div relations to enforce
    pub mul_div_relations: Vec<(
        Boolean<F>,
        ArrayVec<MulDivRelation<F>, MAX_MUL_DIV_RELATIONS_PER_CYCLE>,
    )>,
}
```

There will be no implementation details here because the code is commented step by step and is understandable. Short description:

Apply opcodes, for DST0 it's possible to have opcode-constrainted updates only into registers, apply `StateDiffsAccumulator`, update the memory, update the registers, apply changes to VM state, such as ergs left, etc. push data to queues for other circuits. If an event has rollback then create the same event data but with `rollback` flag, enforce sponges. There are only 2 outcomes:

- we have dst0 write (and may be src0 read), that we taken care above
- opcode itself modified memory queue, based on outcome of src0 read in parallel opcodes either
- do not use sponges and only rely on src0/dst0
- can not have src0/dst0 in memory, but use sponges (UMA, near_call, far call, ret)

No longer in the cyclical part `VM` we set up different queues:

  

1. Memory:

```rust
let memory_queue_current_tail = QueueTailState {
        tail: final_state.memory_queue_state,
        length: final_state.memory_queue_length,
  };
let memory_queue_final_tail = QueueTailState::conditionally_select(
    cs,
    structured_input.completion_flag,
    &memory_queue_current_tail,
    &full_empty_state_large.tail,
);
```

1. Code decommit:

```rust
let decommitment_queue_current_tail = QueueTailState {
        tail: final_state.code_decommittment_queue_state,
        length: final_state.code_decommittment_queue_length,
  };
let decommitment_queue_final_tail = QueueTailState::conditionally_select(
    cs,
    structured_input.completion_flag,
    &decommitment_queue_current_tail,
    &full_empty_state_large.tail,
);
```

1. Log:

```rust
let final_log_state_tail = final_state.callstack.current_context.log_queue_forward_tail;
    let final_log_state_length = final_state
        .callstack
        .current_context
      .log_queue_forward_part_length;

// but we CAN still check that it's potentially mergeable, basically to check that witness generation is good
for (a, b) in final_log_state_tail.iter().zip(
    final_state
        .callstack
        .current_context
        .saved_context
        .reverted_queue_head
        .iter(),
) {
    Num::conditionally_enforce_equal(cs, structured_input.completion_flag, a, b);
}
let full_empty_state_small = QueueState::<F, QUEUE_STATE_WIDTH>::empty(cs);

let log_queue_current_tail = QueueTailState {
    tail: final_log_state_tail,
    length: final_log_state_length,
};
let log_queue_final_tail = QueueTailState::conditionally_select(
    cs,
    structured_input.completion_flag,
    &log_queue_current_tail,
    &full_empty_state_small.tail,
);
```

Wrap them: 

```rust
observable_output.log_queue_final_state.tail = log_queue_final_tail;
observable_output.memory_queue_final_state.tail = memory_queue_final_tail;
observable_output.decommitment_queue_final_state.tail = decommitment_queue_final_tail;

structured_input.observable_output = observable_output;
structured_input.hidden_fsm_output = final_state;
```

Finally, we compute a commitment to PublicInput and allocate it as witness variables.

```rust
let compact_form =
      ClosedFormInputCompactForm::from_full_form(cs, &structured_input, round_function);

let input_commitment: [_; INPUT_OUTPUT_COMMITMENT_LENGTH] =
    commit_variable_length_encodable_item(cs, &compact_form, round_function);
for el in input_commitment.iter() {
    let gate = PublicInputGate::new(el.get_variable());
    gate.add_to_cs(cs);
}
```