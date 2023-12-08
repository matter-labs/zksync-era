# Intuition guide to ZK in zkEVM
[Back to ToC](../../../README.md)

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

## What It Means to Generate a Witness

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
