# Proof System Deeper Overview

The purpose of this section is to explain our new proof system from an engineering standpoint. We will examine the code
examples and how the libraries communicate.

Let's begin by discussing our constraint system. In the previous prover, we utilized the Bellman repository. However, in
the new prover, the constraint system is implemented in Boojum.

## Constraint system

If you look at boojum repo (src/cs/traits/cs.rs):

```rust
pub trait ConstraintSystem<F: SmallField>: Send + Sync {

    ...
    fn alloc_variable() -> Variable;
    fn alloc_witness_without_value(&mut self) -> Witness;
    fn place_gate<G: Gate<F>>(&mut self, gate: &G, row: usize);
    ...
}
```

We have three main components: `Variable`, `Witness`, and `Gate`.

To understand the constraint system, imagine it as a list of "placeholders" called Variables. We define rules, referred
to as "gates" for these Variables. The Witness represents a specific assignment of values to these Variables, ensuring
that the rules still hold true.

Conceptually, this is similar to how we implement functions. Consider the following example:

```
fn fun(x) {
  y = x + A;
  z = y * B;
  w = if y { z } else { y }
}
```

In this code snippet, `A`, `B`, `y`, `z`, and `w` are Variables (with `A` and `B` being constants). We establish rules,
or gates, specifying that the Variable `z` must equal `y` multiplied by the Variable `B`.

Example Witness assignment would be:

```
 x = 1; A = 3; y = 3; B = 0; z = 0; w = 3;
```

Gates can become more complex. For instance, the `w` case demonstrates a "selection" gate, which chooses one of two
options depending on a condition.

Now, let's delve into this gate for a more detailed examination:

### Selection gate

The code is in boojum/src/cs/gates/selection_gate.rs

Let's delve deeper into the concept. Our goal is to create a gate that implements the logic
`result = if selector == true a else b;`. To accomplish this, we will require four variables.

```rust
pub struct SelectionGate {
    pub a: Variable,
    pub b: Variable,
    pub selector: Variable,
    pub result: Variable,
}
```

Internally the `Variable` object is `pub struct Variable(pub(crate) u64);` - so it is an index to the position within
the constraint system object.

And now let's see how we can add this gate into the system.

```rust
pub fn select<F: SmallField, CS: ConstraintSystem<F>>(
    cs: &mut CS,
    a: Variable,
    b: Variable,
    selector: Variable,
) -> Variable {
  //  First, let's allocate the output variable:
  let output_variable = cs.alloc_variable_without_value();
  ...
}
```

And then there is a block of code for witness evaluation (let's skip it for now), and the final block that adds the gate
to the constraint system `cs`:

```rust
    if <CS::Config as CSConfig>::SetupConfig::KEEP_SETUP {
        let gate = Self {
            a,
            b,
            selector,
            result: output_variable,
        };
        gate.add_to_cs(cs);
    }

    output_variable
```

So to recap - we took 3 'Variables', created the output one, created a `SelectionGate` object out of them, which we
added to the system (by calling `add_to_cs`) - and the finally returned the output variable.

But where is the 'logic'? Where do we actually enforce the constraint?

For this, we have to look at the `Evaluator`:

```rust
impl<F: SmallField> Gate<F> for SelectionGate {
    type Evaluator = SelectionGateConstraitEvaluator;

    #[inline]
    fn evaluator(&self) -> Self::Evaluator {
        SelectionGateConstraitEvaluator
    }
}
```

```rust
impl<F: PrimeField> GateConstraitEvaluator<F> for SelectionGateConstraitEvaluator {
  fn evaluate_once{
    let a = trace_source.get_variable_value(0);
    let b = trace_source.get_variable_value(1);
    let selector = trace_source.get_variable_value(2);
    let result = trace_source.get_variable_value(3);

    // contribution = a * selector
    let mut contribution = a;
    contribution.mul_assign(&selector, ctx);

    // tmp = 1 - selector
    let mut tmp = P::one(ctx);
    tmp.sub_assign(&selector, ctx);

    // contribution += tmp * b
    // So:
    // contribution = a*selector + (1-selector) * b
    P::mul_and_accumulate_into(&mut contribution, &tmp, &b, ctx);

    // contribution = a*selector + (1-selector) * b - result
    contribution.sub_assign(&result, ctx);

    // And if we're successful, the contribution == 0.
    // Because result == a * selector + (1-selector) * b
    destination.push_evaluation_result(contribution, ctx);
    }
}
```

This evaluator is actually operating on top of the `Field` objects, trying to build & evaluate the correct polynomials.
The details of it will be covered in a separate article.

Congratulations, you hopefully understood the code for the first gate. To recap - we created the 'output' Variable, and
added the Gate to the CS system. Later when CS system 'computes' all the dependencies, it will run the constraint
evaluator, to add the 'raw' dependency (which is basically an equation) to the list.

You can look into other files in `src/cs/gates` to see other examples.

## Structures

Now, that we handled the basic variables, let's see what we can do with more complex structures. Boojum has added a
bunch of derive macros, to make development easier.

Let's look at the example:

```rust
#[derive(Derivative, CSSelectable, CSAllocatable, CSVarLengthEncodable, WitnessHookable)]
pub struct VmLocalState<F: SmallField> {
    pub previous_code_word: UInt256<F>,
    pub registers: [VMRegister<F>; REGISTERS_COUNT],
    pub flags: ArithmeticFlagsPort<F>,
    pub timestamp: UInt32<F>,
    pub memory_page_counter: UInt32<F>,
```

First - all the UInt that you see above, are actually implemented in Boojum:

```rust
pub struct UInt32<F: SmallField> {
    pub(crate) variable: Variable,
}
impl<F: SmallField> CSAllocatable<F> for UInt32<F> {
    // So the 'witness' type (concrete value) for U32 is u32 - no surprises ;-)
    type Witness = u32;
    ...
}

pub struct UInt256<F: SmallField> {
    pub inner: [UInt32<F>; 8],
}
```

### WitnessHookable

In the example above, the Witness type for U32, was u32 - easy. But what should we do, when we have more complex struct
(like VmLocalState)?

This derive will automatically create a new struct named XXWitness (in example above `VmLocalStateWitness`), that can be
filled with concrete values.

### CsAllocatable

Implements CsAllocatable - which allows you to directly 'allocate' this struct within constraint system (similarly to
how we were operating on regular 'Variables' above).

### CSSelectable

Implements the `Selectable` trait - that allows this struct to participate in operations like conditionally select (so
it can be used as 'a' or 'b' in the Select gate example above).

### CSVarLengthEncodable

Implements CircuitVarLengthEncodable - which allows encoding the struct into a vector of variables (think about it as
serializing to Bytes).

### Summary

Now with the tools above, we can do operations on our constraint system using more complex structures. So we have gates
as 'complex operators' and structures as complex object. Now we're ready to start taking it to the next level: Circuits.

## Circuits

Circuit's definitions are spread across 2 separate repositories: `zkevm_circuits` and `zkevm_test_harness`.

While we have around 9 different circuits (log_sorter, ram_permutation etc) - in this article we'll focus only on the
one: MainVM - which is responsible for handling almost all of the VM operations (other circuits are used to handle some
of the precompiles, and operations that happen after VM was run - like preparing pubdata etc).

Looking at zkevm_test_harness, we can see the definition:

```rust
pub type VMMainCircuit<F, W, R> =
    ZkSyncUniformCircuitInstance<F, VmMainInstanceSynthesisFunction<F, W, R>>;
```

### So what is a circuit

```rust
pub struct ZkSyncUniformCircuitInstance<F: SmallField, S: ZkSyncUniformSynthesisFunction<F>> {
    // Assignment of values to all the Variables.
    pub witness: AtomicCell<Option<S::Witness>>,

    // Configuration - that is circuit specific, in case of MainVM - the configuration
    // is simply the amount of opcodes that we put within 1 circuit.
    pub config: std::sync::Arc<S::Config>,

    // Circuit 'friendly' hash function.
    pub round_function: std::sync::Arc<S::RoundFunction>,

    // Inputs to the circuits.
    pub expected_public_input: Option<[F; INPUT_OUTPUT_COMMITMENT_LENGTH]>,
}
```

Notice - that circuit doesn't have any 'explicit' outputs.

Where ZkSyncUniformCircuitInstance is a proxy, so let's look deeper, into the main function:

```rust
impl VmMainInstanceSynthesisFunction {
    fn synthesize_into_cs_inner<CS: ConstraintSystem<F>>(
        cs: &mut CS,
        witness: Self::Witness,
        round_function: &Self::RoundFunction,
        config: Self::Config,
    ) -> [Num<F>; INPUT_OUTPUT_COMMITMENT_LENGTH] {
        main_vm_entry_point(cs, witness, round_function, config)
    }
}

```

This is the main logic, that takes the witness (remember - Witness is a concrete assignment of values to Variables) -
and returns the public input.

If we look deeper into 'main_vm_entry_point' (which is already in zkevm_circuits repo), we can see:

```rust
pub fn main_vm_entry_point(
    cs: &mut CS,
    witness: VmCircuitWitness<F, W>,
    round_function: &R,
    limit: usize,
) -> [Num<F>; INPUT_OUTPUT_COMMITMENT_LENGTH]
```

And in this function we do following operations:

```rust
    // Prepare current 'state'
    //
    // First - unpack the witness
    let VmCircuitWitness {
        closed_form_input,
        witness_oracle,
    } = witness;

    // And add it to the constraint system
    let mut structured_input =
        VmCircuitInputOutput::alloc_ignoring_outputs(cs, closed_form_input.clone());

    let mut state =
        VmLocalState::conditionally_select(cs, start_flag, &bootloader_state, &hidden_fsm_input);

    // Notice, that now state is a VmLocalState object -- which contains 'Variables' inside.

    // And now run the cycles
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

The `vm_cycle` method is where the magic is - it takes a given opcode, and creates all the necessary gates, temporary
Variables etc inside the Constraint system. This method is around 800 lines long, so I'd encourage you to take a sneak
peek if you're interested.

Now that we've added all the constraints for the 'limit' number of opcodes, we have to do some additional housekeeping -
like storing the Queue hashes (for memory, code decommitment etc).

And then we're ready to prepare the result of this method (input_commitment).

```rust
    // Prepare compact form (that contains just the hashes of values, rather than full values).
    let compact_form =
        ClosedFormInputCompactForm::from_full_form(cs, &structured_input, round_function);

    // And serialize it.
    let input_commitment: [_; INPUT_OUTPUT_COMMITMENT_LENGTH] =
        commit_variable_length_encodable_item(cs, &compact_form, round_function);
    input_commitment
```

## And now putting it all together

Now let's look at the zkevm_test_harness repo, '/src/external_calls.rs' run method. This is used in many tests, and
tries to execute the whole flow end to end.

And while the signature is quite scary - let's walk through this together:

```rust
pub fn run<
    F: SmallField,
    R: BuildableCircuitRoundFunction<F, 8, 12, 4> + AlgebraicRoundFunction<F, 8, 12, 4> + serde::Serialize + serde::de::DeserializeOwned,
    H: RecursiveTreeHasher<F, Num<F>>,
    EXT: FieldExtension<2, BaseField = F>,
    S: Storage
>(
caller: Address, // for real block must be zero
entry_point_address: Address, // for real block must be the bootloader
entry_point_code: Vec<[u8; 32]>, // for read lobkc must be a bootloader code
initial_heap_content: Vec<u8>, // bootloader starts with non-deterministic heap
    zk_porter_is_available: bool,
    default_aa_code_hash: U256,
used_bytecodes: std::collections::HashMap<U256, Vec<[u8; 32]>>, // auxiliary information to avoid passing a full set of all used codes
ram_verification_queries: Vec<(u32, U256)>, // we may need to check that after the bootloader's memory is filled
    cycle_limit: usize,
round_function: R, // used for all queues implementation
    geometry: GeometryConfig,
    storage: S,
    tree: &mut impl BinarySparseStorageTree<256, 32, 32, 8, 32, Blake2s256, ZkSyncStorageLeaf>,
) -> (
    BlockBasicCircuits<F, R>,
    BlockBasicCircuitsPublicInputs<F>,
    BlockBasicCircuitsPublicCompactFormsWitnesses<F>,
    SchedulerCircuitInstanceWitness<F, H, EXT>,
    BlockAuxilaryOutputWitness<F>,
)
    where [(); <crate::zkevm_circuits::base_structures::log_query::LogQuery<F> as CSAllocatableExt<F>>::INTERNAL_STRUCT_LEN]:,
    [(); <crate::zkevm_circuits::base_structures::memory_query::MemoryQuery<F> as CSAllocatableExt<F>>::INTERNAL_STRUCT_LEN]:,
    [(); <crate::zkevm_circuits::base_structures::decommit_query::DecommitQuery<F> as CSAllocatableExt<F>>::INTERNAL_STRUCT_LEN]:,
    [(); <crate::boojum::gadgets::u256::UInt256<F> as CSAllocatableExt<F>>::INTERNAL_STRUCT_LEN]:,
    [(); <crate::boojum::gadgets::u256::UInt256<F> as CSAllocatableExt<F>>::INTERNAL_STRUCT_LEN + 1]:,
    [(); <crate::zkevm_circuits::base_structures::vm_state::saved_context::ExecutionContextRecord<F> as CSAllocatableExt<F>>::INTERNAL_STRUCT_LEN]:,
    [(); <crate::zkevm_circuits::storage_validity_by_grand_product::TimestampedStorageLogRecord<F> as CSAllocatableExt<F>>::INTERNAL_STRUCT_LEN]:,

```

The first section, is adding some decommitments (explain later).

Then we create a vm:

```rust
    let mut out_of_circuit_vm =
        create_out_of_circuit_vm(&mut tools, &block_properties, caller, entry_point_address);
```

And we'll run it over all the operands:

```rust
    for _cycle in 0..cycle_limit {

        out_of_circuit_vm
            .cycle(&mut tracer)
            .expect("cycle should finish successfully");
    }
```

While doing it, we collect 'snapshots' - the detailed information of the state of the system between each operand.

Then we create a `Vec<VmInstanceWitness>` - let's see what's inside:

```rust
pub struct VmInstanceWitness<F: SmallField, O: WitnessOracle<F>> {
    // we need everything to start a circuit from this point of time

    // initial state (state of registers etc)
    pub initial_state: VmLocalState,
    pub witness_oracle: O,
    pub auxilary_initial_parameters: VmInCircuitAuxilaryParameters<F>,
    pub cycles_range: std::ops::Range<u32>,

    // final state for test purposes
    pub final_state: VmLocalState,
    pub auxilary_final_parameters: VmInCircuitAuxilaryParameters<F>,
}
```

With this, let's finally start creating circuits (via `create_leaf_level_circuits_and_scheduler_witness`)

```rust

    for (instance_idx, vm_instance) in vm_instances_witness.into_iter().enumerate() {
         let instance = VMMainCircuit {
            witness: AtomicCell::new(Some(circuit_input)),
            config: Arc::new(geometry.cycles_per_vm_snapshot as usize),
            round_function: round_function.clone(),
            expected_public_input: Some(proof_system_input),
        };

        main_vm_circuits.push(instance);
    }
```
