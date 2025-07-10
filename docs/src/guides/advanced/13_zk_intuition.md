# Intuition guide to ZK in zkEVM

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
    CodeDecommitmentsDeduplicatorInstanceWitness<E>,
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

The job of generating witnesses, which we discussed earlier, is handled by the witness generator. Initially, this was
located in a module zksync core witness. However, for the new proof system, the team began to shift this function to a
new location called [separate witness binary].

Inside this new location, after the necessary data is fetched from storage, the witness generator calls another piece of
code from [zkevm_test_harness witness] named `run_with_fixed_params`. This code is responsible for creating the witnesses
themselves (which can get really HUGE).

## Generating the Proof

Once we have the witness data lined up, it’s time to crunch the numbers and create the proofs.

The main goal of this step is to take an operation (for example, a calculation called `ecrecover`) and break it down
into smaller pieces. Then, we represent this information as a special mathematical expression called a polynomial.

To construct these polynomials, we use something called a `ConstraintSystem`. The specific type that we use is called
zkSNARK, and our custom version of it is named bellman. You can find our code for this in the [bellman repo]. Additionally,
we have an optimized version that's designed to run faster on certain types of hardware (using CUDA technology), which you
can find in the [bellman cuda repo].

An [example ecrecover circuit] might give you a clearer picture of what this looks like in practice.

The proof itself is generated by evaluating this polynomial expression at many different points. Because this involves
heavy calculations, we use GPUs to speed things up.

### Where is the Code

The main code that utilizes the GPUs to create proofs is located in a repository named [heavy_ops_service repo]. This code
combines elements from the [bellman cuda repo] that we mentioned earlier, along with a huge amount of data generated by the
witness, to produce the final proofs.

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

[witness_example]: https://github.com/matter-labs/era-zkevm_test_harness/tree/main/src/witness/individual_circuits/decommit_code.rs#L24
[verifier]: https://github.com/matter-labs/era-contracts/blob/main/l1-contracts/contracts/zksync/Verifier.sol
[bellman repo]: https://github.com/matter-labs/bellman
[bellman cuda repo]: https://github.com/matter-labs/era-bellman-cuda
[example ecrecover circuit]: https://github.com/matter-labs/era-sync_vm/blob/v1.3.2/src/glue/ecrecover_circuit/mod.rs#L157
[separate witness binary]: https://github.com/matter-labs/zksync-era/blob/main/prover/crates/bin/witness_generator/src/main.rs
[zkevm_test_harness witness]: https://github.com/matter-labs/era-zkevm_test_harness/blob/fb47657ae3b6ff6e4bb5199964d3d37212978200/src/external_calls.rs#L579
[heavy_ops_service repo]: https://github.com/matter-labs/era-heavy-ops-service
[franklin repo]: https://github.com/matter-labs/franklin-crypto
