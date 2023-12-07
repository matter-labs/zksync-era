# Intuition guide to ZK in zkSync

**WARNING**: This guide simplifies the complex details of how we use ZK in our systems, just to give you a better
understanding. We're leaving out a lot of details to keep things brief.

## What is the 'Zero Knowledge'

In our case, the prover takes public input and witness (which is huge - you'll see below), and produces a proof, but the
verifier takes (public input, proof) only, without witness. This means that the huge witness doesn't have to be
submitted to L1. This property can be used for many things, like privacy, but here we use it to implement an efficient
rollup that publishes the least required amount of data to L1.

## Basic overview

Let’s break down the basic steps involved when a transaction is made within our ZK system:

- **Execute transaction in State Keeper & Seal the block:** This part has been discussed in other articles.
- **Generate witness:** What’s that? Let's find out below!
- **Generate proof:** This is where some fancy math and computing power comes in.
- **Verify proof on L1:** This means checking that the fancy math was done right on the Ethereum network (referred to as
  L1).

## Generate Witness - What Does It Mean

When our State Keeper processes a transaction, it carries out a bunch of operations and assumes certain conditions
without openly stating them. However, when it comes to ZK, we need to show clear evidence that these conditions hold.

Take this simple example where we have a command that retrieves some data from storage and assigns it to a variable.

`a := SLOAD(0x100)`

In normal circumstances, the system would just read the data from storage and assign it. But in ZK, we need to provide
evidence of what specific data was fetched and that it was indeed present in the storage beforehand.

From the ZK point of view, this looks like:

```
circuit inputs:
* current_state_hash = 0x1234;
* read_value: 44
* merkle_path proving that (0x100, 44) exists in tree with storage hash 0x1234
circuit outputs:
* new state hash (that includes the leaf saying that variable 'a' has value 44)
```

**Note**: In reality, we also use multiple Queues with hashes (together with merkle trees), to track all the memory &
storage accesses.

So, in our example, what seems like a simple action actually requires us to create a bunch of hashes and merkle paths.
This is precisely what the Witness Generator does. It processes the transactions, one operation at a time, and generates
the necessary data that will be used later in circuits.

### A Closer Look

Now let’s dive into a specific example [witness_example]:

```rust=
pub fn compute_decommitter_circuit_snapshots<
    E: Engine,
    R: CircuitArithmeticRoundFunction<E, 2, 3>,
>(
...
) -> (
    Vec<CodeDecommitterCircuitInstanceWitness<E>>,
    CodeDecommittmentsDeduplicatorInstanceWitness<E>,
)
```

In this code snippet, we're looking at a function named `compute_decommitter_circuit_snapshots`. It uses some technical
terms and concepts that may seem daunting, but let's break them down:

**Engine:** This is a trait that specifically handles complex mathematical curves, called Elliptic curves. It’s like
your uint64 on steroids!

**CircuitArithmeticRoundFunction:** This is a special kind of hashing function that's more suited for the circuits we
are using than the regular ones like keccak. In our case, we use Franklin and Rescue from [franklin repo].

The function returns Witness classes, that contain queues such as `FixedWidthEncodingSpongeLikeQueueWitness` which hold
the hashes we mentioned earlier. This is similar merkle paths that we discussed above.

### Where is the Code

The job of generating witnesses, which we discussed earlier, is handled by a the witness generator. Initially, this was
located in a module [zksync core witness]. However, for the new proof system, the team began to shift this function to a
new location called [separate witness binary].

Inside this new location, after the necessary data is fetched from storage, the witness generator calls another piece of
code from [zkevm_test_harness witness] named `run_with_fixed_params`. This code is responsible for creating the
witnesses themselves (which can get really HUGE).

## Generating the Proof

Once we have the witness data lined up, it’s time to crunch the numbers and create the proofs.

The main goal of this step is to take an operation (for example, a calculation called `ecrecover`) and break it down
into smaller pieces. Then, we represent this information as a special mathematical expression called a polynomial.

To construct these polynomials, we use something called a `ConstraintSystem`. The specific type that we use is called
zkSNARK, and our custom version of it is named bellman. You can find our code for this in the [bellman repo].
Additionally, we have an optimized version that's designed to run faster on certain types of hardware (using CUDA
technology), which you can find in the [bellman cuda repo].

An [example ecrecover circuit] might give you a clearer picture of what this looks like in practice.

The proof itself is generated by evaluating this polynomial expression at many different points. Because this involves
heavy calculations, we use GPUs to speed things up.

### Where is the Code

The main code that utilizes the GPUs to create proofs is located in a repository named [heavy_ops_service repo]. This
code combines elements from the [bellman cuda repo] that we mentioned earlier, along with a huge amount of data
generated by the witness, to produce the final proofs.

## What Does "Verify Proof on L1" Mean

Finally, we reach the stage where we have to verify the proof on L1. But what does that really mean?

We need to ensure that four specific values match:

- **C**: This is a value that represents our circuits, also known as verification keys. It’s like a fingerprint of the
  circuit code and is hard-coded into the contract. Whenever the circuit changes, this value changes too.
- **In**: This represents the root hash before the transaction block.
- **Out**: This represents the root hash after the transaction block.
- **P**: This is the proof provided by the prover.

The logic behind this is that there can only be a matching proof 'P' if `C(In) == Out`. In simple terms, it means that
the proof 'P' will only make sense if the values before and after the transaction block are consistent according to the
circuit represented by 'C'.

If you're eager to dive into the nitty-gritty, you can find the code in the [verifier] repository. Also, if you're
interested in learning even more, you can look up KZG commitments.

## A Heads-Up About Code Versions

Please be aware that there are multiple versions of the proving systems, such as v1.3.1, v1.3.2, and so on. When you're
looking through the code, make sure you're checking the version that’s relevant to what you’re working on. At the time
this guide was written, the latest version was 1.3.4, but there was also ongoing development on a new proof system in
version 1.4.0.

[witness_example]:
  https://github.com/matter-labs/era-zkevm_test_harness/tree/main/src/witness/individual_circuits/decommit_code.rs#L24
[verifier]: https://github.com/matter-labs/era-contracts/blob/main/ethereum/contracts/zksync/Verifier.sol
[bellman repo]: https://github.com/matter-labs/bellman
[bellman cuda repo]: https://github.com/matter-labs/era-bellman-cuda
[example ecrecover circuit]:
  https://github.com/matter-labs/sync_vm/blob/683ade0bbb445f3e2ceb82dd3f4346a0c5d16a78/src/glue/ecrecover_circuit/mod.rs#L157
[zksync core witness]:
  https://github.com/matter-labs/zksync-era/blob/main/core/lib/zksync_core/src/witness_generator/mod.rs
[separate witness binary]: https://github.com/matter-labs/zksync-era/blob/main/prover/witness_generator/src/main.rs
[zkevm_test_harness witness]:
  https://github.com/matter-labs/zkevm_test_harness/blob/0c17bc7baa4e0b64634d414942ef4200d8613bbd/src/external_calls.rs#L575
[heavy_ops_service repo]: https://github.com/matter-labs/heavy-ops-service/tree/v1.3.2
[franklin repo]: https://github.com/matter-labs/franklin-crypto

# Prover and keys

You might have come across terms like "prover", "keys", and phrases like "regenerating the verification key" or "why is
the key 8GB in size?" or even "the proof failed because the verification key hash was different." It can all seem a bit
complex, but don't worry, we're here to make it simple.

In this article, we’re going to break down the different types of keys, explain their purposes, and show you how they
are created.

Our main focus will be on the boojum, a new proof system. But if you're familiar with the old proof system, the
principles we'll discuss apply there as well.

## Circuits

![circuits](https://user-images.githubusercontent.com/128217157/275817097-0a543476-52e5-437b-a7d3-10603d5833fa.png)

We offer 13 distinct types of **'base' circuits**, including Vm, Decommitter, and others, which you can view in the
[full list here][basic_circuit_list]. In addition, there are 15 **'recursive' circuits**. Out of these, 13 are 'leaves,'
each corresponding to a basic type, while one is a 'node,' and another is a 'scheduler' that oversees all others. You
can find more details in the [full list here][recursive_circuit_list].

In our new proof system, there's also a final element known as the compressor, or **snark wrapper**, representing an
additional type of circuit.

It's essential to note that each circuit type requires its unique set of keys.

Also, the base circuits, leaves, node and scheduler are STARK based with FRI commitments, while the snark wrapper is
SNARK based with KZG commitment. This results in slightly different contents of the keys, but their role stays the same.

## Keys

### Setup key (big, 14GB)

> In the following [CPU](https://github.com/matter-labs/zksync-era/blob/main/prover/setup-data-cpu-keys.json) and
> [GPU](https://github.com/matter-labs/zksync-era/blob/main/prover/setup-data-gpu-keys.json) links, you'll find GCS
> buckets containing the latest keys.

The primary key for a given circuit is called `setup key`. These keys can be substantial in size - approximately 14GB
for our circuits. Due to their size, we don't store them directly on GitHub; instead, they need to be generated.

If you’re wondering what these setup keys contain, think of them as the 'source code of the circuit.'

This implies that any modifications to a circuit necessitate the regeneration of the setup keys to align with the
changes made.

### Verification key (small, 8kb)

To generate the proof, we need the setup key. However, to verify the proof, a much smaller key, known as the
`verification key`, is required.

These verification keys are available on GitHub, and you can view them [here][verification_key_list]. Each verification
key is stored in a separate file. They are named in the format `verification_X_Y_key.json`, for example,
`verification_basic_4_key.json`.

Comparing these files with the list of circuits mentioned earlier, you'll notice there are 13 files named
`verification_basic_Y`, 15 files for leaf, one each for node and scheduler, and an additional one for wrapper.

In simpler terms, each verification key contains multiple 'hashes' or commitments, derived from different sections of
the setup key. These hashes enable the proof verification process.

### Verification key hash (very small, 32 bytes)

The hash of the verification key serves a quick reference to ensure that both parties involved are using the same keys.
For instance:

- Our state keeper uses this hash to confirm that the L1 contract possesses the correct key.
- The witness generator refers to this hash to determine which jobs it should take on.

Typically, we embed these hashes directly into an environment variable for easy access. You can find an example of this
[here for SNARK_WRAPPER_VK_HASH][env_variables_for_hash].

## CRS files (setup_2^26.key, 8GB files)

These keys, also referred to as Common Reference Strings (CRS), are essential for KZG commitments and were a crucial
part of our old proving system.

With the introduction of the new prover, CRS is only utilized in the final step, specifically during the snark_wrapper
phase. However, since the computational requirements in this stage are significantly reduced compared to the past, we
can rely on a smaller CRS file, namely the setup_2^24.key.

## Advanced

### What's inside the key

#### Setup key

Setup keys house the [ProverSetupData object][prover_setup_data], which in turn contains the full Merkle tree. This is
part of the reason why setup keys can be quite large in size.

To put it in simpler terms, if we consider the circuits as a massive collection of linear equations, the setup key
essentially contains all the parameters for these equations. Every detail that defines and regulates these equations is
stored within the setup key, making it a comprehensive and crucial component in the proving process.

#### Verification key

Verification keys are stored in a more accessible format, as JSON files, making it relatively easy to explore their
contents.

Inside, you’ll find numerous configuration fields related to the circuit. These include the size of the circuit, the
number of columns, the locations of constants, where to insert the public input, and the size of the public input, among
other details.

Additionally, at the end of the file, there’s a Merkle tree hash. In our case, there are actually 16 hashes because our
proving system utilizes a 'Cap' Merkle tree. Imagine a Merkle tree with 16 roots instead of just one; this design
ensures that each Merkle path is slightly shorter, improving efficiency.

#### Verification key hash

As previously stated, the verification key hash is derived from hash function applied to the data contained in the
verification key. You can view the exact process of how the keccak hash is computed in the
[Verifier.sol][verifier_computation] file.

For SNARK circuits (like snark_wrapper), we use keccak as hash function. For START based circuits, we use more circuit
friendly hash function (currently Poseidon2).

[basic_circuit_list]:
  https://github.com/matter-labs/era-zkevm_test_harness/blob/3cd647aa57fc2e1180bab53f7a3b61ec47502a46/circuit_definitions/src/circuit_definitions/base_layer/mod.rs#L77
[recursive_circuit_list]:
  https://github.com/matter-labs/era-zkevm_test_harness/blob/3cd647aa57fc2e1180bab53f7a3b61ec47502a46/circuit_definitions/src/circuit_definitions/recursion_layer/mod.rs#L29
[verification_key_list]:
  https://github.com/matter-labs/zksync-era/tree/boojum-integration/prover/vk_setup_data_generator_server_fri/data
[env_variables_for_hash]:
  https://github.com/matter-labs/zksync-era/blob/boojum-integration/etc/env/base/contracts.toml#L44
[prover_setup_data]:
  https://github.com/matter-labs/zksync-era/blob/d2ca29bf20b4ec2d9ec9e327b4ba6b281d9793de/prover/vk_setup_data_generator_server_fri/src/lib.rs#L61
[verifier_computation]:
  https://github.com/matter-labs/era-contracts/blob/dev/ethereum/contracts/zksync/Verifier.sol#L268

# Overview

The purpose of this document is to explain our new proof system from an engineering standpoint. We will examine the code
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

Internaly the `Variable` object is `pub struct Variable(pub(crate) u64);` - so it is an index to the position within the
constraint system object.

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

Implements the `Selectable` trait - that allows this struct to participage in operations like conditionally select (so
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
    // is simply the amount of opcodes that we put wihtin 1 circuit.
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
used_bytecodes: std::collections::HashMap<U256, Vec<[u8; 32]>>, // auxilary information to avoid passing a full set of all used codes
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
