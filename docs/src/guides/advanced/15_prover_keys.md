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

In our new proof system, there's also a final steps known as the compressor and **snark wrapper**, representing an
additional type of circuit.

It's essential to note that each circuit type requires its unique set of keys.

Also, the base circuits, leaves, node and scheduler are STARK based with FRI commitments, while the snark wrapper is
SNARK based with KZG commitment. This results in slightly different contents of the keys, but their role stays the same.
More info about commitment schemes can be found [here](https://en.wikipedia.org/wiki/Commitment_scheme).

## Keys

### Setup keys (big, >700MB each)

The following [link](https://github.com/matter-labs/zksync-era/blob/main/prover/setup-data-gpu-keys.json) provides the
GCS buckets containing the latest setup keys.

The primary key for a given circuit is called `setup key`. These keys can be substantial in size - approximately 700MB
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

[basic_circuit_list]: https://github.com/matter-labs/era-zkevm_test_harness/blob/3cd647aa57fc2e1180bab53f7a3b61ec47502a46/circuit_definitions/src/circuit_definitions/base_layer/mod.rs#L77
[recursive_circuit_list]: https://github.com/matter-labs/era-zkevm_test_harness/blob/3cd647aa57fc2e1180bab53f7a3b61ec47502a46/circuit_definitions/src/circuit_definitions/recursion_layer/mod.rs#L29
[verification_key_list]: https://github.com/matter-labs/zksync-era/tree/main/prover/data/keys
[env_variables_for_hash]: https://github.com/matter-labs/zksync-era/blob/6d18061df4a18803d3c6377305ef711ce60317e1/etc/env/base/contracts.toml#L61
[prover_setup_data]: https://github.com/matter-labs/zksync-era/blob/d2ca29bf20b4ec2d9ec9e327b4ba6b281d9793de/prover/vk_setup_data_generator_server_fri/src/lib.rs#L61
[verifier_computation]: https://github.com/matter-labs/era-contracts/blob/d85a73a1eeb5557343b7b44c6543aaf391d8b984/l1-contracts/contracts/zksync/Verifier.sol#L267
