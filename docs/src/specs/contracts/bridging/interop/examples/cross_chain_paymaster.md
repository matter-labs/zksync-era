# Cross Chain Paymaster

We want to allow the automatic execution of cross-chain txs. I.e. the user should not have to interact with the destination chain. We use triggers for this. The cross chain txs can be triggered on the origin chain, if this is the case then there is a feePaymentBundle and an execution bundle this is a general solution, that allows multiple options for paying for gas.

- Easiest. Pay a required baseTokenAmount on the sender chain (in the baseToken on the destination chain). The same amount will be minted on the destination chain. e.g. [here](https://github.com/matter-labs/zksync-era/blob/f4ebeecfebd9d01a31a8b036c3f1ff63e5252341/core/tests/ts-integration/tests/interop.test.ts#L526)
- Cross chain paymaster:
    - bridge some other token, e.g. USDC.
    - provide paymaster params in Gasfields ( missing right now). The users contract can take the minted USDC on the destination chain, and swap it for the baseToken using the Paymaster.