# Precommit based interop

[back to readme](../../README.md)

Types of interop and security assumptions.

1. Proof based
2. Commit based
3. Pre-commit based interop. 

## Proof based interop

Slow (proof time ~10+ mins, Secure)

- Batch is sealed, posted to GW
- Proof is posted on GW, batch is fully finalized cannot be reverted.
- We get GW’s global messageRoot from GW.

## Commit/Batch based interop (Tee)

Relatively fast (~1 min, secure), relatively secure.

- Batch has to be sealed. (**Due to no precompiles** this will might also take ~1 min as we need to have large batches to keep expenses low). Even with smaller batches, it will be slower than pre-commit based.
- TEE can be run for extra security. (+ ~1min).
- Batch is committed to GW ( with TEE). Tx data is also committed, so we can regenerate the batch. Alternatively, EN-s are run for chain.
- We get the chains MessageRoot

## Pre-commit/Miniblock based interop

Very fast.

- Batches are not sealed, but are builtx in parallel. MessageRoot is updated mid batch after each block. After the batch is sealed the imported MessageRoot has to be “extended”.
- This means interop can be very fast (~5s). 
- For security EN’s will need to be run. Ok, for a small number of chains.
- For many chains, we can use proof -based or commit based.
    - One directional interop, e.g from Era central hub.
- Note: similar setup is needed for Shared Sequencing.

# Implications on settlement process

- Proof based: Global message root is updated, we can import a single root for all chains. When committing the batch the single root can be exported and checked against the MessageRoot contract.
- commit based: Individual roots are imported from different dependent chains.  We need to export all of them when committing the batch and save them. When executing the batch, we need to check the dependencies have been already executed.
- precommit based: Similar to commit based, with one additional step. When executing the batch the exported MessageRoot might not be the final MessageRoot that sending chain settled. An additional merkle proof will have to be provided here for each dependency.

## Implication on MessageRoot linking for precommit based interop.

For the current l2ToL1Logs root hash we currently we have both a rolling hash during the batch, and at the end we construct a Merkle-tree. 

- This is not efficient.
    
    We would either have to calculate the MT before the batch is sealed, or import all the elements of the rolling hash. Instead we should replace this construction with Dyn Inc merkle tree.
    

Solution: replace current L2ToL1 messages rolling hash + merkle tree with a single Dyn Inc MT, and on the destination chain, import all leaves. In the future we can imort only nodes. 

Using Dyn Inc MT also simplifies the L1Messenger contract. 
    

# Security considerations for Precommit based interop

Without TEEs for miniblocks vs with difference:

1. Without
    - Chains need to run ENs for each other
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
