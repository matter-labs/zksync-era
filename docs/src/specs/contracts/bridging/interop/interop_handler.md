# InteropHandler

The interop handler processes interop bundles. Each interop bundle has an execution address, if specified only this address can execute the bundle. Otherwise any address can execute the bundle. When executing the bundle, for each call in the bundle is executed. There are two options to execute calls, [implicit](https://github.com/ethereum/L2-interop/pull/26/files) vs explicit. See these below. 

## Account Aliasing / Standard Interop Account / Shadow Account deployment

ERC 7786 defines a standard interface for contracts to use interop using a standard interface. 
However, not all contracts will implement the ERC7786 interface. For these contracts implicit [implicit](https://github.com/ethereum/L2-interop/pull/26/files) need to be used (i.e. the passed raw calldata needs to be invoked on the receiver contract). To make the implicit call secure, the call is executed by the intermediate ShadowAccount, which can only be triggered by interop transactions. 
The interop handler can execute calls either via explicit calls using [ERC-7786](https://github.com/ethereum/ERCs/pull/673/files) or via implicit calls using Shadow Account deployment. 

Imagine a user wants to do a cross chain swap. The swap contract will not implement 7786. So a ShadowAccount is deployed for the user, allowing the user to hold funds securely on the destination chain. This ShadowAccount can also interact with the swap contract. The flow would be: 
1. User bridges funds to the shadow account address using the bridge.
2. the user swaps using the shadow account.
3. the user bridges the funds back from their shadow account.

The shadow account is deployed when the user first uses it, i.e in step 2. 

## Open questions:

### Cancellations/ reexecutions:

- L2->L1 transactions allow arbitrary reexecutions, and we don't support call bundling here. If a withdrawal fails, it means the contracts need to be fixed.
- L1->L2 transactions might fail. So on L1 we support claimFailedDeposits.
- L2->L2 transactions can be arbitrarily reexecuted. But here bundles are supported. It means one call might succeed, while another always fails. A bad call can ruin the whole bundle. There are two solutions here:
    - support cancellations of whole bundle. Then we need to implement claimFailedDeposits.
    - support unbundling of calls in a bundle, to execute only some of the calls. This will be the most likely solution.

### Exact deployment method of the Shadow Account

It has to be:

- evm compatible
- eraVM compatible
- generate the same address on all chains.

The simplest solution right now is to enable evm-emulator on all chains and deploy an evm contract. This has the same address on all chains.