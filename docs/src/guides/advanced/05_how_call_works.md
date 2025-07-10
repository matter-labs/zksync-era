# Life of a 'call'

This article will show you how the `call` method works in our backend. The `call` method is a 'read-only' operation,
which means it doesn't change anything on the blockchain. This will give you a chance to understand the system,
including the bootloader and VM.

For this example, let's assume that the contract is already deployed, and we will use the `call` method to interact with
it.

Since the 'call' method is only for reading data, all the calculations will happen in the `api_server`.

### Calling the 'call' method

If you need to make calls quickly, you can use the 'cast' binary from the
[Foundry ZKsync](https://foundry-book.zksync.io/getting-started/installation) suite:

```shell=
cast call 0x23DF7589897C2C9cBa1C3282be2ee6a938138f10 "myfunction()()" --rpc-url http://localhost:3050
```

The address of your contract is represented by 0x23D...

Alternatively, you can make an RPC call directly, but this can be complicated as you will have to create the correct
payload, which includes computing the ABI for the method, among other things.

An example of an RPC call would be:

```shell=
curl --location 'http://localhost:3050' \
--header 'Content-Type: application/json' \
--data '{
    "jsonrpc": "2.0",
    "id": 2,
    "method": "eth_call",
    "params": [
        {
            "from": "0x0000000000000000000000000000000000000000",
            "data": "0x0dfe1681",
            "to": "0x2292539b1232A0022d1Fc86587600d86e26396D2"
        }

    ]
}'
```

As you can see, using the RPC call directly is much more complex. That's why I recommend using the 'cast' tool instead.

### What's happening in the server

Under the hood, the 'cast' tool calls the `eth_call` RPC method, which is part of the official Ethereum API set. You can
find the definition of these methods in the [namespaces/eth.rs][namespaces_rpc_api] file in our code.

Afterward, it goes to the implementation, which is also in the [namespaces/eth.rs][namespaces_rpc_impl] file but in a
different parent directory.

The server then executes the function in a VM sandbox. Since this is a `call` function, the VM only runs this function
before shutting down. This is handled by the `execute_tx_eth_call` method, which fetches metadata like block number and
timestamp from the database, and the `execute_tx_in_sandbox` method, which takes care of the execution itself. Both of
these functions are in the [api_server/execution_sandbox.rs][execution_sandbox] file.

Finally, the transaction is pushed into bootloader memory, and the VM executes it until it finishes.

### VM

Before we look at the bootloader, let's briefly examine the VM itself.

The zkEVM is a state machine with a heap, stack, 16 registers, and state. It executes zkEVM assembly, which has many
opcodes similar to EVM, but operates on registers rather than a stack. We have two implementations of the VM: one is in
'pure rust' without circuits (in the zk_evm repository), and the other has circuits (in the sync_vm repository). In this
example, the api server uses the 'zk_evm' implementation without circuits.

Most of the code that the server uses to interact with the VM is in
[core/lib/multivm/src/versions/vm_latest/implementation/execution.rs][vm_code].

In this line, we're calling self.state.cycle(), which executes a single VM instruction. You can see that we do a lot of
things around this, such as executing multiple tracers after each instruction. This allows us to debug and provide
additional feedback about the state of the VM.

### Bootloader & transaction execution

The Bootloader is a large 'quasi' system contract, written in Yul and located in
[system_contracts/bootloader/bootloader.yul][bootloader_code] .

It's a 'quasi' contract because it isn't actually deployed under any address. Instead, it's loaded directly into the VM
by the binary in the constructor [init_vm_inner][init_vm_inner].

So why do we still need a bootloader if we have the call data, contract binary, and VM? There are two main reasons:

- It allows us to 'glue' transactions together into one large transaction, making proofs a lot cheaper.
- It allows us to handle some system logic (checking gas, managing some L1-L2 data, etc.) in a provable way. From the
  circuit/proving perspective, this behaves like contract code.
- You'll notice that the way we run the bootloader in the VM is by first 'kicking it off' and cycling step-by-step until
  it's ready to accept the first transaction. Then we 'inject' the transaction by putting it in the right place in VM
  memory and start iterating the VM again. The bootloader sees the new transaction and simply executes its opcodes.

This allows us to 'insert' transactions one by one and easily revert the VM state if something goes wrong. Otherwise,
we'd have to start with a fresh VM and re-run all the transactions again.

### Final steps

Since our request was just a 'call', after running the VM to the end, we can collect the result and return it to the
caller. Since this isn't a real transaction, we don't have to do any proofs, witnesses, or publishing to L1.

## Summary

In this article, we covered the 'life of a call' from the RPC to the inner workings of the system, and finally to the
'out-of-circuit' VM with the bootloader.

[namespaces_rpc_api]: https://github.com/matter-labs/zksync-era/blob/edd48fc37bdd58f9f9d85e27d684c01ef2cac8ae/core/bin/zksync_core/src/api_server/web3/backend_jsonrpc/namespaces/eth.rs "namespaces RPC api"
[namespaces_rpc_impl]: https://github.com/matter-labs/zksync-era/blob/main/core/node/api_server/src/web3/namespaces/eth.rs "namespaces RPC implementation"
[execution_sandbox]: https://github.com/matter-labs/zksync-era/blob/main/core/node/api_server/src/execution_sandbox/execute.rs "execution sandbox"
[vm_code]: https://github.com/matter-labs/zksync-era/blob/ccd13ce88ff52c3135d794c6f92bec3b16f2210f/core/lib/multivm/src/versions/vm_latest/implementation/execution.rs#L108 "vm code"
[bootloader_code]: https://github.com/matter-labs/era-system-contracts/blob/93a375ef6ccfe0181a248cb712c88a1babe1f119/bootloader/bootloader.yul "bootloader code"
[init_vm_inner]: https://github.com/matter-labs/zksync-era/blob/main/core/lib/multivm/src/versions/vm_m6/vm_with_bootloader.rs#L330 "vm constructor"
