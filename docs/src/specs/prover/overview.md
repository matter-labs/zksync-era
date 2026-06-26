# Intro to ZKsync’s ZK

This page is specific to our cryptography. For a general introduction, please read:
[https://docs.zksync.io/zksync-protocol/rollup](https://docs.zksync.io/zksync-protocol/rollup)

As a ZK rollup, we want everything to be verified by cryptography and secured by Ethereum. The power of ZK allows for
transaction compression, reducing fees for users while inheriting the same security.

ZK Proofs allow a verifier to easily check whether a prover has done a computation correctly. For ZKsync, the prover
will prove the correct execution of ZKsync’s EVM, and a smart contract on Ethereum will verify the proof is correct.

In more detail, there are several steps.

- Witness generation: witness generation can be perceived as part of the process where the user (prover) generates proof
  of transaction validity. For instance, when a user initiates a transaction, a corresponding witness is generated,
  which serves as proof that the transaction is valid and adheres to the network's consensus rules. The zero-knowledge
  aspect ensures that the witness reveals no information about the transaction's specifics, maintaining user privacy and
  data security. New transactions are proved in batches. These batches will be processed and sent to the circuits.
- Circuits: Our virtual machine needs to prove that the execution was completed correctly to generate proofs correctly.
  This is accomplished using circuits. In order for proofs to work, normal code logic must be transformed into a format
  readable by the proof system. The virtual machine reads the code that will be executed and sorts the parts into
  various circuits. These circuits then break down the parts of code, which can then be sent to the proof system.
- Proof system: We need a proof system to process the ZK circuit. Our proving system is called Boojum.

Here are the different repositories we use:

- **Boojum**: Think of this as the toolbox. It holds essential tools and parts like the prover (which helps confirm the
  circuit's functionality), verifier (which double-checks everything), and various other backend components. These are
  the technical bits and pieces, like defining Booleans, Nums, and Variables that will be used in the circuits.
- **zkevm_circuits**: This is where we build and store the actual circuits. The circuits are built from Boojum and
  designed to replicate the behavior of the EVM.
- **zkevm_test_harness**: It's like our testing ground. Here, we have different tests to ensure our circuits work
  correctly. Additionally, it has the necessary code that helps kickstart and run these circuits smoothly.

### What is a circuit

ZK circuits get their name from Arithmetic Circuits, which look like this (see picture). You can read the circuit by
starting at the bottom with the inputs, and following the arrows, computing each operation as you go.

![Untitled](./img/intro_to_zkSync’s_ZK/circuit.png)

The prover will prove that the circuit is “satisfied” by the inputs, meaning every step is computed correctly, leading
to a correct output.

It is very important that every step is actually “constrained”. The prover must be forced to compute the correct values.
If the circuit is missing a constraint, then a malicious prover can create proofs that will pass verification but not be
valid. The ZK terminology for this is that an underconstrained circuit could lead to a soundness error.

### What do ZKsync’s circuits prove

The main goal of our circuits is to prove correct execution of our VM. This includes proving each opcode run within the
VM, as well as other components such as precompiles, storage, and circuits that connect everything else together. This
is described in more detail in
[Circuits](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/Circuits%20Section/Circuits.md)

### More details

The process of turning code into constraints is called arithmetization. Our arithmetization is based on a variation of
“Plonk”. The details are abstracted away from the circuits, but if you’d like to learn more, read about Plonk in
[Vitalik’s blog](https://vitalik.eth.limo/general/2019/09/22/plonk.html) or the
[Plonky2 paper](https://github.com/0xPolygonZero/plonky2/blob/5d9da5a65bbcba2c66eb29c035090eb2e9ccb05f/plonky2/plonky2.pdf).

More details of our proving system can be found in the [Redshift Paper](https://eprint.iacr.org/2019/1400.pdf).
