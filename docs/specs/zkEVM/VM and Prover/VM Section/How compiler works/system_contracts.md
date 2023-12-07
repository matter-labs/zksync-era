# System Contracts

Many EVM instructions require special handling by the [System Contracts](https://era.zksync.io/docs/reference/architecture/system-contracts.html). Among them are:
`ORIGIN`, `CALLVALUE`, `BALANCE`, `CREATE`, `SHA3`, and others. To see the full detailed list of instructions requiring special handling, see [the EVM instructions reference](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm).

There are several types of System Contracts from the perspective of how they are handled by the zkSync Era compilers:

1. [Environmental data storage](#environmental-data-storage).
2. [KECCAK256 hash function](#keccak256-hash-function).
3. [Contract deployer](#contract-deployer).
4. [Ether value simulator](#ether-value-simulator).
5. [Simulator of immutables](#simulator-of-immutables).
6. [Event handler](#event-handler).



### Environmental Data Storage

Such storage contracts are accessed with static calls in order to retrieve values for the block, transaction, and other environmental entities:
`CHAINID`, `DIFFICULTY`, `BLOCKHASH`, etc.

One good example of such contract is [SystemContext](https://github.com/matter-labs/era-system-contracts/blob/main/contracts/SystemContext.sol) that provides the majority of the environmental data.

Since EVM is not using external calls for these instructions, we must use [the auxiliary heap](#auxiliary-heap) for their calldata.

Steps to handle such instructions:

1. Store the calldata for the System Contract call on the auxiliary heap.
2. Call the System Contract with a static call.
3. Check the return status code of the call.
4. [Revert or throw](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/exception_handling.md) if the status code is zero.
5. Read the ABI data and extract the result. All such System Contracts return a single 256-bit value.
6. Return the value as the result of the original instruction.

For reference, see [the LLVM IR codegen source code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/runtime/system_request.rs).



### KECCAK256 Hash Function

Handling of this function is similar to [Environmental Data Storage](#environmental-data-storage) with one difference:

Since EVM also uses heap to store the calldata for `KECCAK256`, the required memory chunk is allocated by the IR generator, and zkSync Era compiler does not need to use [the auxiliary heap](#auxiliary-heap).

For reference, see [the LLVM IR codegen source code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/runtime/keccak256.rs).



### Contract Deployer

See [handling CREATE](https://era.zksync.io/docs/reference/architecture/differences-with-ethereum.html#create-create2) and [dependency code substitution instructions](https://era.zksync.io/docs/reference/architecture/differences-with-ethereum.html#datasize-dataoffset-datacopy) on zkSync Era documentation.

For reference, see LLVM IR codegen for [the deployer call](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/context/function/runtime/deployer_call.rs) and [CREATE-related instructions](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/create.rs).



### Ether Value Simulator

EraVM does not support passing Ether natively, so this is handled by a special System Contract called [MsgValueSimulator](https://github.com/matter-labs/era-system-contracts/blob/main/contracts/MsgValueSimulator.sol).

An external call is redirected through the simulator if the following conditions are met:

1. The [call](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/call.md) has the Ether value parameter.
2. The Ether value is non-zero.

The call to the simulator requires extra data passed via ABI using registers:

1. Ether value.
2. The address of the contract to call.
3. The system call bit, which is only set if a call to the [ContractDeployer](#contract-deployer) is being redirected, that is `CREATE` or `CREATE2` is called with non-zero Ether.

For reference, see [the LLVM IR codegen source code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/call.rs#L530).



### Simulator of Immutables

See [handling immutables](https://era.zksync.io/docs/reference/architecture/differences-with-ethereum.html#setimmutable-loadimmutable) on zkSync Era documentation.

For reference, see LLVM IR codegen for [instructions for immutables](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/immutable.rs) and [RETURN from the deploy code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/return.rs#L28).



### Event Handler

Event payloads are sent to a special System Contract called [EventWriter](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/EventWriter.yul). Like on EVM, the payload consists of topics and data:

1. The topics with a length-prefix are passed via ABI using registers.
2. The data is passed via the default heap, like on EVM.

For reference, see [the LLVM IR codegen source code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/event.rs).



# Auxiliary Heap

Both [zksolc](https://era.zksync.io/docs/tools/compiler-toolchain/solidity.html) and [zkvyper](https://era.zksync.io/docs/tools/compiler-toolchain/vyper.html) compilers for EraVM operate
on [the IR level](https://era.zksync.io/docs/tools/compiler-toolchain/overview.html#ir-compilers), so they cannot control the heap memory allocator which remains a responsibility of
[the high-level source code compilers](https://era.zksync.io/docs/tools/compiler-toolchain/overview.html#high-level-source-code-compilers) emitting the IRs.

However, the are several cases where EraVM needs to allocate memory on the heap and EVM does not. The auxiliary heap is used for these cases:

1. [Returning immutables](https://era.zksync.io/docs/reference/architecture/differences-with-ethereum.html#setimmutable-loadimmutable) from the constructor.
2. Allocating calldata and return data for calling the [System Contracts](https://era.zksync.io/docs/reference/architecture/system-contracts.html).