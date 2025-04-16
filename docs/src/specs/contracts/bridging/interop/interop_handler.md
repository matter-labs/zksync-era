# InteropHandler

The interop handler processes interop bundles. Each interop bundle has an execution address, if specified only this address can execute the bundle. Otherwise any address can execute the bundle. When executing the bundle, for each call in the bundle is executed. There are two options to execute calls, [implicit](https://github.com/ethereum/L2-interop/pull/26/files) vs explicit. See these below. 

## Account Aliasing / Standard Interop Account deployment

The interop handler can execute calls either via explicit calls using [ERC-7786](https://github.com/ethereum/ERCs/pull/673/files) or via [implicit](https://github.com/ethereum/L2-interop/pull/26/files) calls using Standard Interop Account deployment. The benefit of implicit calls is that the called contract does not have to support interop. To make the implicit call secure, the call is executed by an intermediate InteropAccount, which can only be triggered by interop transactions. 

## Open questions:

### Cancellations/ reexecutions:

- L2->L1 transactions allow arbitrary reexecutions, and we don't support call bundling here. If a withdrawal fails, it means the contracts need to be fixed.
- L1->L2 transactions might fail. So on L1 we support claimFailedDeposits.
- L2->L2 transactions can be arbitrarily reexecuted. But here bundles are supported. It means one call might succeed, while another always fails. A bad call can ruin the whole bundle. There are two solutions here:
    - support cancellations of whole bundle. Then we need to implement claimFailedDeposits.
    - support unbundling of calls in a bundle, to execute only some of the calls.

### Exact deployment method of the Standard Interop Account

It has to be:

- evm compatible
- eraVM compatible
- generate the same address on all chains.

The simplest solution right now is to enable evm-emulator on all chains and deploy an evm contract. This has the same address on all chains.