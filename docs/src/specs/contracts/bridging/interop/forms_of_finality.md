# MessageRoot importing and forms of finality

[back to readme](../../README.md)

## Introduction

The message root is the contract on L1 that collects messages from different chains and aggregates them into a single merkle tree. This makes interop more efficient, since instead of having to import each individual message, chains can import the MessageRoot, which is an aggregate of messages in a single batch, then across batches of a single chain, and then across chains. 

The MessageRoot contract is deployed both on L1 and ZK chains, but on ZK chains it is only used on GW. On GW it is used to aggregate messages for chains that are settling on GW, in the same way that it is done on L1. Read about it [here](../gateway/nested_l3_l1_messaging.md).

![MessageRoot](../img/message_root.png)

Interop requires the importing of a MessageRoot which contains the interop tx. This can be done in different ways, depending on the security trust between the chains.

1. Proof based interop
2. Commit based interop
3. Pre-commit based interop. 

## Proof based interop

The batch where the interop tx is emitted is sealed and committed to the chain's settlement layer (Gateway or L1). Proof is posted on the SL, and the batch is fully finalized and cannot be reverted. When this happens the SL's `MessageRoot` is updated. The receiving chain imports this `MessageRoot`. When the receiving chain settles, it the imported `MessageRoot` is checked against the MessageRoot.sol contract.

This solution is the most trustless, but it is the slowest, since proofs have to be generated.

## Commit/Batch based interop

The batch has to be sealed, and the Batch is committed to SL. We do not wait for the `MessageRoot` to be updated. We get the `ChainBatchRoot` of the source chain from the DiamondProxy of the chain itself. When the receiving chain commits the batch, the imported MessageRoot is stored as a dependency. When the batch is executed, it is checked that all the dependencies have been executed.

This is faster than proof based interop, but not as fast as pre-commit based interop. It is also the middle ground in security.

## Pre-commit/Miniblock based interop

The batch is not sealed, but are built in parallel. `L2ToL1LogsRoot` is updated mid batch after each block. The receiving chain can read the current `L2ToL1LogsRoot` from the L1Messenger contract of the source chain. This can be two way, i.e. both chains can read from each other. It can happen multiple times as well. When settling, the imported `L2ToL1LogsRoot` might not be the final one that is settled by the source chain. An additional merkle proof will have to complete the root. This final root can be checked against the pending root of the other chain. If the roots match, then the batches of the chains have to be executed in parallel.

- Note: this solution can also be used for Shared Sequencing.
- Note: the L2ToL1 messages are aggregated in a Dynamic Incramental Merkle tree, see [here](../../../../../../contracts/l1-contracts/contracts/common/libraries/DynamicIncrementalMerkle.sol). 

This solution is the fastest, and also the least secure. 

# Implications on settlement process

- Proof based: SL's `MessageRoot` is updated, we can import a single root for all chains. When committing the batch the single root can be exported and checked against the `MessageRoot` contract.
- Commit based: Individual `ChainBatchRoot`s  are imported from different dependent chains.  We need to export all of them when committing the batch and save them. When executing the batch, we need to check the dependencies have been already executed.
- precommit based: Similar to commit based, with one additional step. When executing the batch the exported `LocalRoot` might not be the final `ChainBatchRoot` that sending chain settled. An additional merkle proof will have to be provided here for each dependency.

# Security considerations for Precommit based interop

Without TEEs for miniblocks vs with difference:

1. Without
    - There is no validity checking of blocks.
        - So we need to run ENs for all chains as well. Chains can query us for those MessageRoots.
        - We can create an onchain lock with miniblock states.
            - Some actor need to be able to revert miniblocks for all chains, in case of wrong blocks/ loss of data.
        - We need EN to main node swap. If the chain stalls, we need to be able to finish the committed blocks.  

2. With
    
    Calldata + other data + output is posted to GW. 
    
    - 2.1. Either the proofs is verified offchain  ( it can be posted to the chain)
        - If verification is too expensive. i.e. for L1 chains.
        - Chains can commit different batches later.
        - We need to be able to revert those batches, and commit the correct ones.
    - 2.2. Or onchain in a contract,
        
        In this case we should store the the committed TEE results, to avoid duplication. Reverting is only possible with 
        
         If onchain, the contract is either.  
        
        - 2.2.1. enforced TEE and inclusion
            - we only need to get the security council involved for bugs in TEEs.
            - we only need to worry about missing zk proofs, and timeouts.
        - 2.2.3 Enforced inclusion, not enforced TEE proof
            - if TEE proof is expensive this is useful. But I think it is cheap, simple signature.
            - But if the chains commits incorrect TEE proof, it will be frozen ( it does not effect interop though)
            

corner cases:

- TEE committed, settlement not finished.
    - We need to be able to use the provided data to:
        - seal the batch, commit it, generate a proof, and execute it. Are we set up for this? Is the platform work enough?
    - if the timeout is large - then this can ‘impact’ the finality of the whole network.
        
        we have to live with this, as long as we can be sure there will be settlement.  
        
        - Some chains take days to close batches
        - What if we cannot generate a proof? normally we could revert the batch. Here we willl need the help of the security council.
    - Forced settlement
        - timeout, after which the batch can be closed externally.
            - Multi level timeout. Initially only the operator, then the security council, then zkchain operators, than anybody can commit batches.
        - We need to make sure to show the batch boundry in the TEE miniblocks.
        - The chain might have some underlying uncommitted blocks that we inside the batch ( but left out). We need to change the code to do the internal re-org. This can hopefully come later. In the short term, we can run ENs for the important chains, and commit all batches. The timeout also helps (the operator has time to commit).
- if there is a bug in TEE - then transition is stuck
    - Unlikely, security council can intervene.
- Upgrades
    - During upgrades we could disable pre-commit based interop, if there is an interface difference.
