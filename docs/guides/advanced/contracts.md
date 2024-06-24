# ZKsync contracts

Now that we know how to bridge tokens back and forth, let's talk about running things on ZKsync.

We have a bunch of great tutorials (like this one <https://docs.zksync.io/build/tooling/hardhat/getting-started>) that
you can follow to get the exact code & command line calls to create the contracts - so in this article, let's focus on
how things differ between ZKsync and Ethereum.

**Note** Before reading this article, I'd recommend doing the hardhat tutorial above.

## Ethereum flow

In case of Ethereum, you start by writing a contract code in solidity, then you compile it with `solc`, and you get the
EVM bytecode, deployment bytecode (which is a function that should return the bytecode itself) and ABI (interface).

Afterwards, you send the deployment bytecode to the 0x000 address on Ethereum, which does some magic (executes the
deployment bytecode, that should contain the constructor etc) and puts the contract under the address that is generated
based on your account id and a nonce.

From this moment on, you can send the transactions to this new address (and most of the tools would ask you to provide
the ABI, so that they can set the proper function arguments).

All the bytecode will be run on the EVM (Ethereum Virtual Machine) - that has a stack, access to memory and storage, and
a bunch of opcodes.

## ZKsync flow

The main part (and the main cost) of the ZKsync is the proving system. In order to make proof as fast as possible, we're
running a little bit different virtual machine (zkEVM) - that has a slightly different set of opcodes, and also contains
a bunch of registers. More details on this will be written in the future articles.

Having a different VM means that we must have a separate compiler [zk-solc](https://github.com/matter-labs/zksolc-bin) -
as the bytecode that is produced by this compiler has to use the zkEVM specific opcodes.

While having a separate compiler introduces a bunch of challenges (for example, we need a custom
[hardhat plugins](https://github.com/matter-labs/hardhat-zksync) ), it brings a bunch of benefits too: for example it
allows us to move some of the VM logic (like new contract deployment) into System contracts - which allows faster &
cheaper modifications and increased flexibility.

### ZKsync system contracts

Small note on system contracts: as mentioned above, we moved some of the VM logic into system contracts, which allows us
to keep VM simpler (and with this - keep the proving system simpler).

You can see the full list (and codes) of the system contracts here:
<https://github.com/matter-labs/era-system-contracts>.

While some of them are not really visible to the contract developer (like the fact that we're running a special
`Bootleader` to package a bunch of transactions together - more info in a future article) - some others are very
visible - like our `ContractDeployer`

### ContractDeployer

Deploying a new contract differs on Ethereum and ZKsync.

While on Ethereum - you send the transaction to 0x00 address - on ZKsync you have to call the special `ContractDeployer`
system contract.

If you look on your hardhat example, you'll notice that your `deploy.ts` is actually using a `Deployer` class from the
`hardhat-zksync-deploy` plugin.

Which inside uses the ZKsync's web3.js, that calls the contract deployer
[here](https://github.com/zksync-sdk/zksync2-js/blob/b1d11aa016d93ebba240cdeceb40e675fb948133/src/contract.ts#L76)

```typescript
override getDeployTransaction(..) {
    ...
    txRequest.to = CONTRACT_DEPLOYER_ADDRESS;
    ...
}
```

Also `ContractDeployer` adding a special prefix for all the new contract addresses. This means that contract addresses
WILL be different on `ZKsync` and Ethereum (and also leaves us the possibility of adding Ethereum addresses in the
future if needed).

You can look for `CREATE2_PREFIX` and `CREATE_PREFIX` in the code.

### Gas costs

Another part, where ZKsync differs from Ethereum is gas cost. The best example for this are storage slots.

If you have two transactions that are updating the same storage slot - and they are in the same 'batch' - only the first
one would be charged (as when we write the final storage to ethereum, we just write the final diff of what slots have
changed - so updating the same slot multiple times doesn't increase the amount of data that we have to write to L1).

### Account abstraction and some method calls

As `ZKsync` has a built-in Account Abstraction (more on this in a separate article) - you shouldn't depend on some of
the solidity functions (like `ecrecover` - that checks the keys, or `tx.origin`) - in all the cases, the compiler will
try to warn you.

## Summary

In this article, we looked at how contract development & deployment differs on Ethereum and ZKsync (looking at
differences in VMs, compilers and system contracts).
