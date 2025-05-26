# Consensus Registry

As part of the decentralization effort we plan to introduce two new roles into the system:

- Validators, which are nodes that are meant to receive L2 blocks from the sequencer, execute them locally and broadcast their signature over the block if it’s valid. If the sequencer receives enough of these signatures, the L2 block is considered finalized. Nodes that are following the chain or syncing will only accept blocks that are finalized.
- Attesters, which basically do the same thing as validators but for L1 batches instead of L2 blocks. Just like with L2 blocks, if a L1 batch is accompanied by enough attester signatures then it’s considered finalized. How these signatures are used is different from validators' signatures though. These signatures are meant to be submitted to L1 together with the L1 batch when it’s committed. And the L1 contracts are meant to only accept L1 batches that come with enough signatures from the correct attesters. But that functionality is not implemented yet.

The `ConsensusRegistry` contract implements a small part of that entire flow. In order to verify the L2 block and L1 batch signatures we need to know the public keys of the validators and attesters that signed them. And we also want that set of validators and attesters to be dynamic. The `ConsensusRegistry` contract is going to store and manage the current set of validators and attesters and expose methods to add, remove and modify validators/attesters.

## Users

There are basically three types of users that will call this contract:

- The contract owner. This is generally meant to be some multisig or governance contract. In this case, it will initially be Matter Labs multisig and later it will be changed to be ZKsync’s governance. It can call any method in the contract and basically can modify the validator and attester sets at will. There are methods that are exclusive to it though. Namely add nodes, remove nodes, change validator/attester weights (the relative voting power of each validator/attester) and commit validator/attester committees (this creates a snapshot of the current nodes and updates the committees).
- The node owners. The entities that will run the validators and attesters. They change over time as nodes get added/removed. They can only activate/deactivate their nodes (deactivated nodes do not get selected to be part of committees) and change their validator/attester public keys.
- The sequencer plus anyone running an external node. They need to verify L1 batch and L2 block signatures so they need to get the attester and validator committees for each batch. There are getter methods for this.

## Future integration

Currently the `ConsensusRegistry` contract is not directly connected to the protocol. The plan is to read the validator committee from the consensus registry contract on each new batch. And, with upcoming protocol upgrades, start verifying the validator signatures onchain in each submitted batch.
