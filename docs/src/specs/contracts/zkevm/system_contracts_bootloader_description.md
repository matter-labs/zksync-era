# System contracts/bootloader description (VM v1.5.0)

## Bootloader

On standard Ethereum clients, the workflow for executing blocks is the following:

1. Pick a transaction, validate the transactions & charge the fee, execute it
2. Gather the state changes (if the transaction has not reverted), apply them to the state.
3. Go back to step (1) if the block gas limit has not been yet exceeded.

However, having such flow on ZKsync (i.e. processing transaction one-by-one) would be too inefficient, since we have to run the entire proving workflow for each individual transaction. That’s what we need the _bootloader_ for: instead of running N transactions separately, we run the entire batch (set of blocks, more can be found [here](./batches_and_blocks_on_zksync.md)) as a single program that accepts the array of transactions as well as some other batch metadata and processes them inside a single big “transaction”. The easiest way to think about bootloader is to think in terms of EntryPoint from EIP4337: it also accepts the array of transactions and facilitates the Account Abstraction protocol.

The hash of the code of the bootloader is stored on L1 and can only be changed as a part of a system upgrade. Note, that unlike system contracts, the bootloader’s code is not stored anywhere on L2. That’s why we may sometimes refer to the bootloader’s address as formal. It only exists for the sake of providing some value to `this` / `msg.sender`/etc. When someone calls the bootloader address (e.g. to pay fees) the EmptyContract’s code is actually invoked.

## System contracts

While most of the primitive EVM opcodes can be supported out of the box (i.e. zero-value calls, addition/multiplication/memory/storage management, etc), some of the opcodes are not supported by the VM by default and they are implemented via “system contracts” — these contracts are located in a special _kernel space,_ i.e. in the address space in range `[0..2^16-1]`, and they have some special privileges, which users’ contracts don’t have. These contracts are pre-deployed at the genesis and updating their code can be done only via system upgrade, managed from L1.

The use of each system contract will be explained down below.

### Pre-deployed contracts

Some of the contracts need to be predeployed at the genesis, but they do not need the kernel space rights. To give them minimal permissiones, we predeploy them at consecutive addresses that start right at the `2^16`. These will be described in the following sections.

## zkEVM internals

Full specification of the zkEVM is beyond the scope of this document. However, this section will give you most of the details needed for understanding the L2 system smart contracts & basic differences between EVM and zkEVM.

### Registers and memory management

On EVM, during transaction execution, the following memory areas are available:

- `memory` itself.
- `calldata` the immutable slice of parent memory.
- `returndata` the immutable slice returned by the latest call to another contract.
- `stack` where the local variables are stored.

Unlike EVM, which is stack machine, zkEVM has 16 registers. Instead of receiving input from `calldata`, zkEVM starts by receiving a _pointer_ in its first register (_basically a packed struct with 4 elements: the memory page id, start and length of the slice to which it points to_) to the calldata page of the parent. Similarly, a transaction can receive some other additional data within its registers at the start of the program: whether the transaction should invoke the constructor ([more about deployments here](#contractdeployer--immutablesimulator)), whether the transaction has `isSystem` flag, etc. The meaning of each of these flags will be expanded further in this section.

_Pointers_ are separate type in the VM. It is only possible to:

- Read some value within a pointer.
- Shrink the pointer by reducing the slice to which pointer points to.
- Receive the pointer to the returndata/as a calldata.
- Pointers can be stored only on stack/registers to make sure that the other contracts can not read memory/returndata of contracts they are not supposed to.
- A pointer can be converted to the u256 integer representing it, but an integer can not be converted to a pointer to prevent unallowed memory access.
- It is not possible to return a pointer that points to a memory page with id smaller than the one for the current page. What this means is that it is only possible to `return` only pointer to the memory of the current frame or one of the pointers returned by the subcalls of the current frame.

#### Memory areas in zkEVM

For each frame, the following memory areas are allocated:

- _Heap_ (plays the same role as `memory` on Ethereum).
- _AuxHeap_ (auxiliary heap). It has the same properties as Heap, but it is used for the compiler to encode calldata/copy the returndata from the calls to system contracts to not interfere with the standard Solidity memory alignment.
- _Stack_. Unlike Ethereum, stack is not the primary place to get arguments for opcodes. The biggest difference between stack on zkEVM and EVM is that on ZKsync stack can be accessed at any location (just like memory). While users do not pay for the growth of stack, the stack can be fully cleared at the end of the frame, so the overhead is minimal.
- _Code_. The memory area from which the VM executes the code of the contract. The contract itself can not read the code page, it is only done implicitly by the VM.

Also, as mentioned in the previous section, the contract receives the pointer to the calldata.

#### Managing returndata & calldata

Whenever a contract finishes its execution, the parent’s frame receives a _pointer_ as `returndata`. This pointer may point to the child frame’s Heap/AuxHeap or it can even be the same `returndata` pointer that the child frame received from some of its child frames.

The same goes with the `calldata`. Whenever a contract starts its execution, it receives the pointer to the calldata. The parent frame can provide any valid pointer as the calldata, which means it can either be a pointer to the slice of parent’s frame memory (heap or auxHeap) or it can be some valid pointer that the parent frame has received before as calldata/returndata.

Contracts simply remember the calldata pointer at the start of the execution frame (it is by design of the compiler) and remembers the latest received returndata pointer.

Some important implications of this is that it is now possible to do the following calls without any memory copying:

A → B → C

where C receives a slice of the calldata received by B.

The same goes for returning data:

A ← B ← C

There is no need to copy returned data if the B returns a slice of the returndata returned by C.

Note, that you can _not_ use the pointer that you received via calldata as returndata (i.e. return it at the end of the execution frame). Otherwise, it would be possible that returndata points to the memory slice of the active frame and allow editing the `returndata`. It means that in the examples above, C could not return a slice of its calldata without memory copying.

Note, that the rule above is implemented by the principle "it is not possible to return a slice of data with memory page id lower than the memory page id of the current heap", since a memory page with smaller id could only be created before the call. That's why a user contract can usually safely return a slice of previously returned returndata (since it is guaranteed to have a higher memory page id). However, system contracts have an exemption from the rule above. It is needed in particular to the correct functionality of the `CodeOracle` system contract. You can read more about it [here](#codeoracle). So the rule of thumb is that returndata from `CodeOracle` should never be passed along.

Some of these memory optimizations can be seen utilized in the [EfficientCall](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/libraries/EfficientCall.sol#L34) library that allows to perform a call while reusing the slice of calldata that the frame already has, without memory copying.

#### Returndata & precompiles

Some of the operations which are opcodes on Ethereum, have become calls to some of the system contracts. The most notable examples are `Keccak256`, `SystemContext`, etc. Note, that, if done naively, the following lines of code would work differently on ZKsync and Ethereum:

```solidity
pop(call(...))
keccak(...)
returndatacopy(...)
```

Since the call to keccak precompile would modify the `returndata`. To avoid this, our compiler does not override the latest `returndata` pointer after calls to such opcode-like precompiles.

### ZKsync specific opcodes

While some Ethereum opcodes are not supported out of the box, some of the new opcodes were added to facilitate the development of the system contracts.

Note, that this lists does not aim to be specific about the internals, but rather explain methods in the [SystemContractHelper.sol](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/libraries/SystemContractHelper.sol#L44)

#### **Only for kernel space**

These opcodes are allowed only for contracts in kernel space (i.e. system contracts). If executed in other places they result in `revert(0,0)`.

- `mimic_call`. The same as a normal `call`, but it can alter the `msg.sender` field of the transaction.
- `to_l1`. Sends a system L2→L1 log to Ethereum. The structure of this log can be seen [here](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/common/Messaging.sol#L23).
- `event`. Emits an L2 log to ZKsync. Note, that L2 logs are not equivalent to Ethereum events. Each L2 log can emit 64 bytes of data (the actual size is 88 bytes, because it includes the emitter address, etc). A single Ethereum event is represented with multiple `event` logs constitute. This opcode is only used by `EventWriter` system contract.
- `precompile_call`. This is an opcode that accepts two parameters: the uint256 representing the packed parameters for it as well as the ergs to burn. Besides the price for the precompile call itself, it burns the provided ergs and executes the precompile. The action that it does depend on `this` during execution:
  - If it is the address of the `ecrecover` system contract, it performs the ecrecover operation
  - If it is the address of the `sha256`/`keccak256` system contracts, it performs the corresponding hashing operation.
  - It does nothing (i.e. just burns ergs) otherwise. It can be used to burn ergs needed for L2→L1 communication or publication of bytecodes onchain.
- `setValueForNextFarCall` sets `msg.value` for the next `call`/`mimic_call`. Note, that it does not mean that the value will be really transferred. It just sets the corresponding `msg.value` context variable. The transferring of ETH should be done via other means by the system contract that uses this parameter. Note, that this method has no effect on `delegatecall` , since `delegatecall` inherits the `msg.value` of the previous frame.
- `increment_tx_counter` increments the counter of the transactions within the VM. The transaction counter used mostly for the VM’s internal tracking of events. Used only in bootloader after the end of each transaction.
- `decommit` will return a pointer to a slice with the corresponding bytecode hash preimage. If this bytecode has been unpacked before, the memory page where it was unpacked will be reused. If it has never been unpacked before, it will be unpacked into the current heap.

Note, that currently we do not have access to the `tx_counter` within VM (i.e. for now it is possible to increment it and it will be automatically used for logs such as `event`s as well as system logs produced by `to_l1`, but we can not read it). We need to read it to publish the _user_ L2→L1 logs, so `increment_tx_counter` is always accompanied by the corresponding call to the [SystemContext](#systemcontext) contract.

More on the difference between system and user logs can be read [here](../settlement_contracts/data_availability/standard_pubdata_format.md).

#### **Generally accessible**

Here are opcodes that can be generally accessed by any contract. Note that while the VM allows to access these methods, it does not mean that this is easy: the compiler might not have convenient support for some use-cases yet.

- `near_call`. It is basically a “framed” jump to some location of the code of your contract. The difference between the `near_call` and ordinary jump are:
  1. It is possible to provide an ergsLimit for it. Note, that unlike “`far_call`”s (i.e. calls between contracts) the 63/64 rule does not apply to them.
  2. If the near call frame panics, all state changes made by it are reversed. Please note, that the memory changes will **not** be reverted.
- `getMeta`. Returns an u256 packed value of [ZkSyncMeta](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/libraries/SystemContractHelper.sol#L18) struct. Note that this is not tight packing. The struct is formed by the [following rust code](https://github.com/matter-labs/zksync-protocol/blob/main/crates/zkevm_opcode_defs/src/definitions/abi/meta.rs#L4).
- `getCodeAddress` — receives the address of the executed code. This is different from `this` , since in case of delegatecalls `this` is preserved, but `codeAddress` is not.

#### Flags for calls

Besides the calldata, it is also possible to provide additional information to the callee when doing `call` , `mimic_call`, `delegate_call`. The called contract will receive the following information in its first 12 registers at the start of execution:

- _r1_ — the pointer to the calldata.
- _r2_ — the pointer with flags of the call. This is a mask, where each bit is set only if certain flags have been set to the call. Currently, two flags are supported: 0-th bit: `isConstructor` flag. This flag can only be set by system contracts and denotes whether the account should execute its constructor logic. Note, unlike Ethereum, there is no separation on constructor & deployment bytecode. More on that can be read [here](#contractdeployer--immutablesimulator). 1-st bit: `isSystem` flag. Whether the call intends a system contracts’ function. While most of the system contracts’ functions are relatively harmless, accessing some with calldata only may break the invariants of Ethereum, e.g. if the system contract uses `mimic_call`: no one expects that by calling a contract some operations may be done out of the name of the caller. This flag can be only set if the callee is in kernel space.
- The rest r3..r12 registers are non-empty only if the `isSystem` flag is set. There may be arbitrary values passed, which we call `extraAbiParams`.

The compiler implementation is that these flags are remembered by the contract and can be accessed later during execution via special [simulations](https://github.com/code-423n4/2024-03-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/extensions/overview.md).

If the caller provides inappropriate flags (i.e. tries to set `isSystem` flag when callee is not in the kernel space), the flags are ignored.

#### `onlySystemCall` modifier

Some of the system contracts can act on behalf of the user or have a very important impact on the behavior of the account. That’s why we wanted to make it clear that users can not invoke potentially dangerous operations by doing a simple EVM-like `call`. Whenever a user wants to invoke some of the operations which we considered dangerous, they must provide “`isSystem`” flag with them.

The `onlySystemCall` flag checks that the call was either done with the “isSystemCall” flag provided or the call is done by another system contract (since Matter Labs is fully aware of system contracts).

#### Simulations via our compiler

In the future, we plan to introduce our “extended” version of Solidity with more supported opcodes than the original one. However, right now it was beyond the capacity of the team to do, so in order to represent accessing ZKsync-specific opcodes, we use `call` opcode with certain constant parameters that will be automatically replaced by the compiler with zkEVM native opcode.

Example:

```solidity
function getCodeAddress() internal view returns (address addr) {
  address callAddr = CODE_ADDRESS_CALL_ADDRESS;
  assembly {
    addr := staticcall(0, callAddr, 0, 0xFFFF, 0, 0)
  }
}
```

In the example above, the compiler will detect that the static call is done to the constant `CODE_ADDRESS_CALL_ADDRESS` and so it will replace it with the opcode for getting the code address of the current execution.

Full list of opcode simulations can be found [here](https://github.com/code-423n4/2024-03-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/extensions/call.md).

We also use [verbatim-like](https://github.com/code-423n4/2024-03-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/extensions/verbatim.md) statements to access ZKsync-specific opcodes in the bootloader.

All the usages of the simulations in our Solidity code are implemented in the [SystemContractHelper](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/libraries/SystemContractHelper.sol) library and the [SystemContractsCaller](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts//libraries/SystemContractsCaller.sol) library.

#### Simulating `near_call` (in Yul only)

In order to use `near_call` i.e. to call a local function, while providing a limit of ergs (gas) that this function can use, the following syntax is used:

The function should contain `ZKSYNC_NEAR_CALL` string in its name and accept at least 1 input parameter. The first input parameter is the packed ABI of the `near_call`. Currently, it is equal to the number of ergs to be passed with the `near_call`.

Whenever a `near_call` panics, the `ZKSYNC_CATCH_NEAR_CALL` function is called.

_Important note:_ the compiler behaves in a way that if there is a `revert` in the bootloader, the `ZKSYNC_CATCH_NEAR_CALL` is not called and the parent frame is reverted as well. The only way to revert only the `near_call` frame is to trigger VM’s _panic_ (it can be triggered with either invalid opcode or out of gas error).

_Important note 2:_ The 63/64 rule does not apply to `near_call`. Also, if 0 gas is provided to the near call, then actually all of the available gas will go to it.

#### Notes on security

To prevent unintended substitution, the compiler requires `--system-mode` flag to be passed during compilation for the above substitutions to work.

> Note, that in the more recent compiler versions this the `--system-mode` has been renamed to `enable_eravm_extensions` (this can be seen in e.g. our [foundry.toml](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/foundry.toml))

### Bytecode hashes

On ZKsync the bytecode hashes are stored in the following format:

- The 0th byte denotes the version of the format. Currently the only version that is used is “1”.
- The 1st byte is `0` for deployed contracts’ code and `1` for the contract code [that is being constructed](#constructing-vs-non-constructing-code-hash).
- The 2nd and 3rd bytes denote the length of the contract in 32-byte words as big-endian 2-byte number.
- The next 28 bytes are the last 28 bytes of the sha256 hash of the contract’s bytecode.

The bytes are ordered in little-endian order (i.e. the same way as for `bytes32` ).

#### Bytecode validity

A bytecode is valid if it:

- Has its length in bytes divisible by 32 (i.e. consists of an integer number of 32-byte words).
- Has a length of less than 2^16 words (i.e. its length in words fits into 2 bytes).
- Has an odd length in words (i.e. the 3rd byte is an odd number).

Note, that it does not have to consist of only correct opcodes. In case the VM encounters an invalid opcode, it will simply revert (similar to how EVM would treat them).

A call to a contract with invalid bytecode can not be proven. That is why it is **essential** that no contract with invalid bytecode is ever deployed on ZKsync. It is the job of the [KnownCodesStorage](#knowncodestorage) to ensure that all allowed bytecodes in the system are valid.

## Account abstraction

One of the other important features of ZKsync is the support of account abstraction. It is highly recommended to read the documentation on our AA protocol here: [https://docs.zksync.io/zk-stack/concepts/account-abstraction](https://docs.zksync.io/zk-stack/concepts/account-abstraction)

#### Account versioning

Each account can also specify which version of the account abstraction protocol do they support. This is needed to allow breaking changes of the protocol in the future.

Currently, two versions are supported: `None` (i.e. it is a simple contract and it should never be used as `from` field of a transaction), and `Version1`.

#### Nonce ordering

Accounts can also signal to the operator which nonce ordering it should expect from these accounts: `Sequential` or `Arbitrary`.

`Sequential` means that the nonces should be ordered in the same way as in EOAs. This means, that, for instance, the operator will always wait for a transaction with nonce `X` before processing a transaction with nonce `X+1`.

`Arbitrary` means that the nonces can be ordered in arbitrary order. It is supported by the server right now, i.e. if there is a contract with arbitrary nonce ordering, its transactions will likely either be rejected or get stuck in the mempool due to nonce mismatch.

Note, that this is not enforced by system contracts in any way. Some sanity checks may be present, but the accounts are allowed to do however they like. It is more of a suggestion to the operator on how to manage the mempool.

#### Returned magic value

Now, both accounts and paymasters are required to return a certain magic value upon validation. This magic value will be enforced to be correct on the mainnet, but will be ignored during fee estimation. Unlike Ethereum, the signature verification + fee charging/nonce increment are not included as part of the intrinsic costs of the transaction. These are paid as part of the execution and so they need to be estimated as part of the estimation for the transaction’s costs.

Generally, the accounts are recommended to perform as many operations as during normal validation, but only return the invalid magic in the end of the validation. This will allow to correctly (or at least as correctly as possible) estimate the price for the validation of the account.

## Bootloader

Bootloader is the program that accepts an array of transactions and executes the entire ZKsync batch. This section will expand on its invariants and methods.

### Playground bootloader vs proved bootloader

For convenience, we use the same implementation of the bootloader both in the mainnet batches and for emulating ethCalls or other testing activities. _Only_ _proved_ bootloader is ever used for batch-building and thus this document describes only it.

### Start of the batch

It is enforced by the ZKPs, that the state of the bootloader is equivalent to the state of a contract transaction with empty calldata. The only difference is that it starts with all the possible memory pre-allocated (to avoid costs for memory expansion).

For additional efficiency (and our convenience), the bootloader receives its parameters inside its memory. This is the only point of non-determinism: the bootloader _starts with its memory pre-filled with any data the operator wants_. That’s why it is responsible for validating the correctness of it and it should never rely on the initial contents of the memory to be correct & valid.

For instance, for each transaction, we check that it is [properly ABI-encoded](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L4044) and that the transactions [go exactly one after another](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L4037). We also ensure that transactions do not exceed the limits of the memory space allowed for transactions.

### Transaction types & their validation

While the main transaction format is the internal `Transaction` [format](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/libraries/TransactionHelper.sol#L25), it is a struct that is used to represent various kinds of transactions types. It contains a lot of `reserved` fields that could be used depending in the future types of transactions without need for AA to change the interfaces of their contracts.

The exact type of the transaction is marked by the `txType` field of the transaction type. There are 6 types currently supported:

- `txType`: 0. It means that this transaction is of legacy transaction type. The following restrictions are enforced:
- `maxFeePerErgs=getMaxPriorityFeePerErg` since it is pre-EIP1559 tx type.
- `reserved1..reserved4` as well as `paymaster` are 0. `paymasterInput` is zero.
- Note, that unlike type 1 and type 2 transactions, `reserved0` field can be set to a non-zero value, denoting that this legacy transaction is EIP-155-compatible and its RLP encoding (as well as signature) should contain the `chainId` of the system.
- `txType`: 1. It means that the transaction is of type 1, i.e. transactions access list. ZKsync does not support access lists in any way, so no benefits of fulfilling this list will be provided. The access list is assumed to be empty. The same restrictions as for type 0 are enforced, but also `reserved0` must be 0.
- `txType`: 2. It is EIP1559 transactions. The same restrictions as for type 1 apply, but now `maxFeePerErgs` may not be equal to `getMaxPriorityFeePerErg`.
- `txType`: 113. It is ZKsync transaction type. This transaction type is intended for AA support. The only restriction that applies to this transaction type: fields `reserved0..reserved4` must be equal to 0.
- `txType`: 254. It is a transaction type that is used for upgrading the L2 system. This is the only type of transaction is allowed to start a transaction out of the name of the contracts in kernel space.
- `txType`: 255. It is a transaction that comes from L1. There are almost no restrictions explicitly imposed upon this type of transaction, since the bootloader at the end of its execution sends the rolling hash of the executed priority transactions. The L1 contract ensures that the hash did indeed match the [hashes of the priority transactions on L1](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/state-transition/chain-deps/facets/Executor.sol#L376).

You can also read more on L1->L2 transactions and upgrade transactions [here](../settlement_contracts/priority_queue/l1_l2_communication/l1_to_l2.md).

However, as already stated, the bootloader’s memory is not deterministic and the operator is free to put anything it wants there. For all of the transaction types above the restrictions are imposed in the following ([method](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L3107)), which is called before starting processing the transaction.

### Structure of the bootloader’s memory

The bootloader expects the following structure of the memory (here by word we denote 32-bytes, the same machine word as on EVM):

#### **Batch information**

The first 8 words are reserved for the batch information provided by the operator.

- `0` word — the address of the operator (the beneficiary of the transactions).
- `1` word — the hash of the previous batch. Its validation will be explained later on.
- `2` word — the timestamp of the current batch. Its validation will be explained later on.
- `3` word — the number of the new batch.
- `4` word — the fair pubdata price. More on how our pubdata is calculated can be read [here](./zksync_fee_model.md).
- `5` word — the “fair” price for L2 gas, i.e. the price below which the `baseFee` of the batch should not fall. For now, it is provided by the operator, but it in the future it may become hardcoded.
- `6` word — the base fee for the batch that is expected by the operator. While the base fee is deterministic, it is still provided to the bootloader just to make sure that the data that the operator has coincides with the data provided by the bootloader.
- `7` word — reserved word. Unused on proved batch.

The batch information slots [are used at the beginning of the batch](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#3921). Once read, these slots can be used for temporary data.

#### **Temporary data for debug & transaction processing purposes**

- `[8..39]` – reserved slots for debugging purposes
- `[40..72]` – slots for holding the paymaster context data for the current transaction. The role of the paymaster context is similar to the [EIP4337](https://eips.ethereum.org/EIPS/eip-4337)’s one. You can read more about it in the account abstraction documentation.
- `[73..74]` – slots for signed and explorer transaction hash of the currently processed L2 transaction.
- `[75..142]` – 68 slots for the calldata for the KnownCodesContract call.
- `[143..10142]` – 10000 slots for the refunds for the transactions.
- `[10143..20142]` – 10000 slots for the overhead for batch for the transactions. This overhead is suggested by the operator, i.e. the bootloader will still double-check that the operator does not overcharge the user.
- `[20143..30142]` – slots for the “trusted” gas limits by the operator. The user’s transaction will have at its disposal `min(MAX_TX_GAS(), trustedGasLimit)`, where `MAX_TX_GAS` is a constant guaranteed by the system. Currently, it is equal to 80 million gas. In the future, this feature will be removed.
- `[30143..70146]` – slots for storing L2 block info for each transaction. You can read more on the difference L2 blocks and batches [here](./batches_and_blocks_on_zksync.md).
- `[70147..266754]` – slots used for compressed bytecodes each in the following format:
  - 32 bytecode hash
  - 32 zeroes (but then it will be modified by the bootloader to contain 28 zeroes and then the 4-byte selector of the `publishCompressedBytecode` function of the `BytecodeCompressor`)
  - The calldata to the bytecode compressor (without the selector).
- `[266755..266756]` – slots where the hash and the number of current priority ops is stored. More on it in the priority operations [section](../settlement_contracts/priority_queue/l1_l2_communication/l1_to_l2.md).

#### L1Messenger Pubdata

- `[266757..1626756]` – slots where the final batch pubdata is supplied to be verified by the [L2DAValidator](../settlement_contracts/data_availability/custom_da.md).

But briefly, this space is used for the calldata to the L1Messenger’s `publishPubdataAndClearState` function, which accepts the address of the L2DAValidator as well as the pubdata for it to check. The L2DAValidator is a contract that is responsible to ensure efficiency [when handling pubdata](../settlement_contracts/data_availability/custom_da.md). Typically, the calldata `L2DAValidator` would include uncompressed preimages for bytecodes, L2->L1 messages, L2->L1 logs, etc as their compressed counterparts. However, the exact implementation may vary across various ZK chains.

Note, that while the realistic number of pubdata that can be published in a batch is ~780kb, the size of the calldata to L1Messenger may be a lot larger due to the fact that this method also accepts the original uncompressed state diff entries. These will not be published to L1, but will be used to verify the correctness of the compression.

One of "worst case" scenarios for the number of state diffs in a batch is when 780kb of pubdata is spent on repeated writes, that are all zeroed out. In this case, the number of diffs is 780kb / 5 = 156k. This means that they will have accoomdate 42432000 bytes of calldata for the uncompressed state diffs. Adding 780kb on top leaves us with roughly 43212000 bytes needed for calldata. 1350375 slots are needed to accommodate this amount of data. We round up to 1360000 slots just in case.

In theory though much more calldata could be used (if for instance 1 byte is used for enum index). It is the responsibility of the
operator to ensure that it can form the correct calldata for the L1Messenger.

#### **Transaction’s meta descriptions**

- `[1626756..1646756]` words — 20000 slots for 10000 transaction’s meta descriptions (their structure is explained below).

For internal reasons related to possible future integrations of zero-knowledge proofs about some of the contents of the bootloader’s memory, the array of the transactions is not passed as the ABI-encoding of the array of transactions, but:

- We have a constant maximum number of transactions. At the time of this writing, this number is 10000.
- Then, we have 10000 transaction descriptions, each ABI encoded as the following struct:

```solidity
struct BootloaderTxDescription {
  // The offset by which the ABI-encoded transaction's data is stored
  uint256 txDataOffset;
  // Auxiliary data on the transaction's execution. In our internal versions
  // of the bootloader it may have some special meaning, but for the
  // bootloader used on the mainnet it has only one meaning: whether to execute
  // the transaction. If 0, no more transactions should be executed. If 1, then
  // we should execute this transaction and possibly try to execute the next one.
  uint256 txExecutionMeta;
}
```

#### **Reserved slots for the calldata for the paymaster’s postOp operation**

- `[1646756..1646795]` words — 40 slots which could be used for encoding the calls for postOp methods of the paymaster.

To avoid additional copying of transactions for calls for the account abstraction, we reserve some of the slots which could be then used to form the calldata for the `postOp` call for the account abstraction without having to copy the entire transaction’s data.

#### **The actual transaction’s descriptions**

- `[1646796..1967599]`

Starting from the 487312 word, the actual descriptions of the transactions start. (The struct can be found by this [link](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/libraries/TransactionHelper.sol#L25)). The bootloader enforces that:

- They are correctly ABI encoded representations of the struct above.
- They are located without any gaps in memory (the first transaction starts at word 653 and each transaction goes right after the next one).
- The contents of the currently processed transaction (and the ones that will be processed later on are untouched). Note, that we do allow overriding data from the already processed transactions as it helps to preserve efficiency by not having to copy the contents of the `Transaction` each time we need to encode a call to the account.

#### **VM hook pointers**

- `[1967600..1967602]`

These are memory slots that are used purely for debugging purposes (when the VM writes to these slots, the server side can catch these calls and give important insight information for debugging issues).

#### **Result ptr pointer**

- `[1967602..1977602]`

These are memory slots that are used to track the success status of a transaction. If the transaction with number `i` succeeded, the slot `937499 - 10000 + i` will be marked as 1 and 0 otherwise.

### General flow of the bootloader’s execution

1. At the start of the batch it [reads the initial batch information](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L3928) and [sends the information](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L2857) about the current batch to the SystemContext system contract.
2. It goes through each of [transaction’s descriptions](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L4016) and checks whether the `execute` field is set. If not, it ends processing of the transactions and ends execution of the batch. If the execute field is non-zero, the transaction will be executed and it goes to step 3.
3. Based on the transaction’s type it decides whether the transaction is an L1 or L2 transaction and processes them accordingly. More on the processing of the L1 transactions can be read [here](#l1-l2-transactions). More on L2 transactions can be read [here](#l2-transactions).

### L2 transactions

On ZKsync, every address is a contract. Users can start transactions from their EOA accounts, because every address that does not have any contract deployed on it implicitly contains the code defined in the [DefaultAccount.sol](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/DefaultAccount.sol) file. Whenever anyone calls a contract that is not in kernel space (i.e. the address is ≥ 2^16) and does not have any contract code deployed on it, the code for `DefaultAccount` will be used as the contract’s code.

Note, that if you call an account that is in kernel space and does not have any code deployed there, right now, the transaction will revert.

We process the L2 transactions according to our account abstraction protocol: [https://docs.zksync.io/build/developer-reference/account-abstraction](https://docs.zksync.io/build/developer-reference/account-abstraction).

1. We [deduct](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L1263) the transaction’s upfront payment for the overhead for the block’s processing. You can read more on how that works in the fee model [description](./zksync_fee_model.md).
2. Then we calculate the gasPrice for these transactions according to the EIP1559 rules.
3. We [conduct the validation step](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L1287) of the AA protocol:

   - We calculate the hash of the transaction.
   - If enough gas has been provided, we near_call the validation function in the bootloader. It sets the tx.origin to the address of the bootloader, sets the ergsPrice. It also marks the factory dependencies provided by the transaction as marked and then invokes the validation method of the account and verifies the returned magic.
   - Calls the accounts and, if needed, the paymaster to receive the payment for the transaction. Note, that accounts may not use `block.baseFee` context variable, so they have no way to know what exact sum to pay. That’s why the accounts typically firstly send `tx.maxFeePerErg * tx.ergsLimit` and the bootloader [refunds](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L792) for any excess funds sent.

4. [We perform the execution of the transaction](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L1352). Note, that if the sender is an EOA, tx.origin is set equal to the `from` the value of the transaction. During the execution of the transaction, the publishing of the compressed bytecodes happens: for each factory dependency if it has not been published yet and its hash is currently pointed to in the compressed bytecodes area of the bootloader, a call to the bytecode compressor is done. Also, at the end the call to the KnownCodeStorage is done to ensure all the bytecodes have indeed been published.
5. We [refund](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L1206) the user for any excess funds he spent on the transaction:

   - Firstly, the `postTransaction` operation is called to the paymaster.
   - The bootloader asks the operator to provide a refund. During the first VM run without proofs the provide directly inserts the refunds in the memory of the bootloader. During the run for the proved batches, the operator already knows what which values have to be inserted there. You can read more about it in the [documentation](./zksync_fee_model.md) of the fee model.
   - The bootloader refunds the user.

6. We notify the operator about the [refund](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L1217) that was granted to the user. It will be used for the correct displaying of gasUsed for the transaction in explorer.

### L1->L2 transactions

L1->L2 transactions are transactions that were initiated on L1. We assume that `from` has already authorized the L1→L2 transactions. It also has its L1 pubdata price as well as ergsPrice set on L1.

Most of the steps from the execution of L2 transactions are omitted and we set `tx.origin` to the `from`, and `ergsPrice` to the one provided by transaction. After that, we use [mimicCall](#zksync-specific-opcodes) to provide the operation itself from the name of the sender account.

Note, that for L1→L2 transactions, `reserved0` field denotes the amount of ETH that should be minted on L2 as a result of this transaction. `reserved1` is the refund receiver address, i.e. the address that would receive the refund for the transaction as well as the msg.value if the transaction fails.

There are two kinds of L1->L2 transactions:

- Priority operations, initiated by users (they have type `255`).
- Upgrade transactions, that can be initiated during system upgrade (they have type `254`).

You can read more about differences between those in the corresponding [document](../settlement_contracts/priority_queue/l1_l2_communication/l1_to_l2.md).

### End of the batch

At the end of the batch we set `tx.origin` and `tx.gasprice` context variables to zero to save L1 gas on calldata and send the entire bootloader balance to the operator, effectively sending fees to him.

Also, we [set](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L4110) the fictive L2 block’s data. Then, we call the system context to ensure that it publishes the timestamp of the L2 block as well as L1 batch. We also reset the `txNumberInBlock` counter to avoid its state diffs from being published on L1. You can read more about block processing on ZKsync [here](./batches_and_blocks_on_zksync.md).

After that, we publish the hash as well as the number of priority operations in this batch. More on it [here](../settlement_contracts/priority_queue/l1_l2_communication/l1_to_l2.md).

Then, we call the L1Messenger system contract for it to compose the pubdata to be published on L1. You can read more about the pubdata processing [here](../settlement_contracts/data_availability/standard_pubdata_format.md).

## System contracts

Most of the details on the implementation and the requirements for the execution of system contracts can be found in the doc-comments of their respective code bases. This chapter serves only as a high-level overview of such contracts.

All the codes of system contracts (including `DefaultAccount`s) are part of the protocol and can only be change via a system upgrade through L1.

### SystemContext

This contract is used to support various system parameters not included in the VM by default, i.e. `chainId`, `origin`, `ergsPrice`, `blockErgsLimit`, `coinbase`, `difficulty`, `baseFee`, `blockhash`, `block.number`, `block.timestamp.`

It is important to note that the constructor is **not** run for this contract upon genesis, i.e. the constant context values are set on genesis explicitly. Notably, if in the future we want to upgrade the contracts, we will do it via [ContractDeployer](#contractdeployer--immutablesimulator) and so the constructor will be run.

This contract is also responsible for ensuring validity and consistency of batches, L2 blocks. The implementation itself is rather straightforward, but to better understand this contract, please take a look at the [page](./batches_and_blocks_on_zksync.md) about the block processing on ZKsync.

### AccountCodeStorage

The code hashes of accounts are stored inside the storage of this contract. Whenever a VM calls a contract with address `address` it retrieves the value under storage slot `address` of this system contract, if this value is non-zero, it uses this as the code hash of the account.

Whenever a contract is called, the VM asks the operator to provide the preimage for the codehash of the account. That is why data availability of the code hashes is paramount.

#### Constructing vs Non-constructing code hash

In order to prevent contracts from being able to call a contract during its construction, we set the marker (i.e. second byte of the bytecode hash of the account) as `1`. This way, the VM will ensure that whenever a contract is called without the `isConstructor` flag, the bytecode of the default account (i.e. EOA) will be substituted instead of the original bytecode.

### BootloaderUtilities

This contract contains some of the methods which are needed purely for the bootloader functionality but were moved out from the bootloader itself for the convenience of not writing this logic in Yul.

### DefaultAccount

Whenever a contract that does **not** both:

- belong to kernel space
- have any code deployed on it (the value stored under the corresponding storage slot in `AccountCodeStorage` is zero)

The code of the default account is used. The main purpose of this contract is to provide EOA-like experience for both wallet users and contracts that call it, i.e. it should not be distinguishable (apart of spent gas) from EOA accounts on Ethereum.

### Ecrecover

The implementation of the ecrecover precompile. It is expected to be used frequently, so written in pure yul with a custom memory layout.

The contract accepts the calldata in the same format as EVM precompile, i.e. the first 32 bytes are the hash, the next 32 bytes are the v, the next 32 bytes are the r, and the last 32 bytes are the s.

It also validates the input by the same rules as the EVM precompile:

- The v should be either 27 or 28,
- The r and s should be less than the curve order.

After that, it makes a precompile call and returns empty bytes if the call failed, and the recovered address otherwise.

### Empty contracts

Some of the contracts are relied upon to have EOA-like behaviour, i.e. they can be always called and get the success value in return. An example of such address is 0 address. We also require the bootloader to be callable so that the users could transfer ETH to it.

For these contracts, we insert the `EmptyContract` code upon genesis. It is basically a noop code, which does nothing and returns `success=1`.

### SHA256 & Keccak256

Note that, unlike Ethereum, keccak256 is a precompile (_not an opcode_) on ZKsync.

These system contracts act as wrappers for their respective crypto precompile implementations. They are expected to be used frequently, especially keccak256, since Solidity computes storage slots for mapping and dynamic arrays with its help. That's why we wrote contracts on pure yul with optimizing the short input case. In the past both `sha256` and `keccak256` performed padding within the smart contracts, this is no longer true with `sha256` performing padding in the smart contracts and `keccak256` in the zk-circuits. Hashing is then completed for both within the zk-circuits.

It's important to note that the crypto part of the `sha256` precompile expects to work with padded data. This means that a bug in applying padding may lead to an unprovable transaction.

### EcAdd & EcMul

These precompiles simulate the behaviour of the EVM's EcAdd and EcMul precompiles and are fully implemented in Yul without circuit counterparts. You can read more about them [here](./elliptic_curve_precompiles.md).

### L2BaseToken & MsgValueSimulator

Unlike Ethereum, zkEVM does not have any notion of any special native token. That’s why we have to simulate operations with the native token (in which fees are charged) via two contracts: `L2BaseToken` & `MsgValueSimulator`.

`L2BaseToken` is a contract that holds the balances of native token for the users. This contract does NOT provide ERC20 interface. The only method for transferring native token is `transferFromTo`. It permits only some system contracts to transfer on behalf of users. This is needed to ensure that the interface is as close to Ethereum as possible, i.e. the only way to transfer native token is by doing a call to a contract with some `msg.value`. This is what `MsgValueSimulator` system contract is for.

Whenever anyone wants to do a non-zero value call, they need to call `MsgValueSimulator` with:

- The calldata for the call equal to the original one.
- Pass `value` and whether the call should be marked with `isSystem` in the first extra abi params.
- Pass the address of the callee in the second extraAbiParam.

More information on the extraAbiParams can be read [here](#flags-for-calls).

#### Support for `.send/.transfer`

On Ethereum, whenever a call with non-zero value is done, some additional gas is charged from the caller's frame and in return a `2300` gas stipend is given out to the callee frame. This stipend is usually enough to emit a small event, but it is enforced that it is not possible to change storage within these `2300` gas. This also means that in practice some users might opt to do `call` with 0 gas provided, relying on the `2300` stipend to be passed to the callee. This is the case for `.call/.transfer`.

While using `.send/.transfer` is generally not recommended, as a step towards better EVM compatibility, since vm1.5.0 a _partial_ support of these functions is present with ZKsync Era. It is the done via the following means:

- Whenever a call is done to the `MsgValueSimulator` system contract, `27000` gas is deducted from the caller's frame and it passed to the `MsgValueSimulator` on top of whatever gas the user has originally provided. The number was chosen to cover for the execution of the transferring of the balances as well as other constant size operations by the `MsgValueSimulator`. Note, that since it will be the frame of `MsgValueSimulator` that will actually call the callee, the constant must also include the cost for decommitting the code of the callee. Decoding bytecode of any size would be prohibitevely expensive and so we support only callees of size up to `100000` bytes.
- `MsgValueSimulator` ensures that no more than `2300` out of the stipend above gets to the callee, ensuring the reentrancy protection invariant for these functions holds.

Note, that unlike EVM any unused gas from such calls will be refunded.

The system preserves the following guarantees about `.send/.transfer`:

- No more than `2300` gas will be received by the callee. Note, [that a smaller, but a close amount](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/test-contracts/TransferTest.sol#L33) may be passed.
- It is not possible to do any storage changes within this stipend. This is enforced by having cold write cost more than `2300` gas. Also, cold write cost always has to be prepaid whenever executing storage writes. More on it can be read [here](../l2_system_contracts/zksync_fee_model.md#io-pricing).
- Any callee with bytecode size of up to `100000` will work.

The system does not guarantee the following:

- That callees with bytecode size larger than `100000` will work. Note, that a malicious operator can fail any call to a callee with large bytecode even if it has been decommitted before. More on it can be read [here](../l2_system_contracts/zksync_fee_model.md#io-pricing).

As a conclusion, using `.send/.transfer` should be generally avoided, but when avoiding is not possible it should be used with small callees, e.g. EOAs, which implement [DefaultAccount](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/DefaultAccount.sol).

### KnownCodeStorage

This contract is used to store whether a certain code hash is “known”, i.e. can be used to deploy contracts. On ZKsync, the L2 stores the contract’s code _hashes_ and not the codes themselves. Therefore, it must be part of the protocol to ensure that no contract with unknown bytecode (i.e. hash with an unknown preimage) is ever deployed.

The factory dependencies field provided by the user for each transaction contains the list of the contract’s bytecode hashes to be marked as known. We can not simply trust the operator to “know” these bytecodehashes as the operator might be malicious and hide the preimage. We ensure the availability of the bytecode in the following way:

- If the transaction comes from L1, i.e. all its factory dependencies have already been published on L1, we can simply mark these dependencies as “known”.
- If the transaction comes from L2, i.e. (the factory dependencies are yet to publish on L1), we make the user pays by burning ergs proportional to the bytecode’s length. After that, we send the L2→L1 log with the bytecode hash of the contract. It is the responsibility of the L1 contracts to verify that the corresponding bytecode hash has been published on L1.

It is the responsibility of the [ContractDeployer](#contractdeployer--immutablesimulator) system contract to deploy only those code hashes that are known.

The KnownCodesStorage contract is also responsible for ensuring that all the “known” bytecode hashes are also [valid](#bytecode-validity).

### ContractDeployer & ImmutableSimulator

`ContractDeployer` is a system contract responsible for deploying contracts on ZKsync. It is better to understand how it works in the context of how the contract deployment works on ZKsync. Unlike Ethereum, where `create`/`create2` are opcodes, on ZKsync these are implemented by the compiler via calls to the ContractDeployer system contract.

For additional security, we also distinguish the deployment of normal contracts and accounts. That’s why the main methods that will be used by the user are `create`, `create2`, `createAccount`, `create2Account`, which simulate the CREATE-like and CREATE2-like behavior for deploying normal and account contracts respectively.

#### **Address derivation**

Each rollup that supports L1→L2 communications needs to make sure that the addresses of contracts on L1 and L2 do not overlap during such communication (otherwise it would be possible that some evil proxy on L1 could mutate the state of the L2 contract). Generally, rollups solve this issue in two ways:

- XOR/ADD some kind of constant to addresses during L1→L2 communication. That’s how rollups closer to full EVM-equivalence solve it, since it allows them to maintain the same derivation rules on L1 at the expense of contract accounts on L1 having to redeploy on L2.
- Have different derivation rules from Ethereum. That is the path that ZKsync has chosen, mainly because since we have different bytecode than on EVM, CREATE2 address derivation would be different in practice anyway.

You can see the rules for our address derivation in `getNewAddressCreate2`/ `getNewAddressCreate` methods in the ContractDeployer.

Note, that we still add a certain constant to the addresses during L1→L2 communication in order to allow ourselves some way to support EVM bytecodes in the future.

#### **Deployment nonce**

On Ethereum, the same nonce is used for CREATE for accounts and EOA wallets. On ZKsync this is not the case, we use a separate nonce called “deploymentNonce” to track the nonces for accounts. This was done mostly for consistency with custom accounts and for having multicalls feature in the future.

#### **General process of deployment**

- After incrementing the deployment nonce, the contract deployer must ensure that the bytecode that is being deployed is available.
- After that, it puts the bytecode hash with a [special constructing marker](#constructing-vs-non-constructing-code-hash) as code for the address of the to-be-deployed contract.
- Then, if there is any value passed with the call, the contract deployer passes it to the deployed account and sets the `msg.value` for the next as equal to this value.
- Then, it uses `mimic_call` for calling the constructor of the contract out of the name of the account.
- It parses the array of immutables returned by the constructor (we’ll talk about immutables in more details later).
- Calls `ImmutableSimulator` to set the immutables that are to be used for the deployed contract.

Note how it is different from the EVM approach: on EVM when the contract is deployed, it executes the initCode and returns the deployedCode. On ZKsync, contracts only have the deployed code and can set immutables as storage variables returned by the constructor.

#### **Constructor**

On Ethereum, the constructor is only part of the initCode that gets executed during the deployment of the contract and returns the deployment code of the contract. On ZKsync, there is no separation between deployed code and constructor code. The constructor is always a part of the deployment code of the contract. In order to protect it from being called, the compiler-generated contracts invoke constructor only if the `isConstructor` flag provided (it is only available for the system contracts). You can read more about flags [here](#flags-for-calls).

After execution, the constructor must return an array of:

```solidity
struct ImmutableData {
  uint256 index;
  bytes32 value;
}
```

basically denoting an array of immutables passed to the contract.

#### **Immutables**

Immutables are stored in the `ImmutableSimulator` system contract. The way how `index` of each immutable is defined is part of the compiler specification. This contract treats it simply as mapping from index to value for each particular address.

Whenever a contract needs to access a value of some immutable, they call the `ImmutableSimulator.getImmutable(getCodeAddress(), index)`. Note that on ZKsync it is possible to get the current execution address (you can read more about `getCodeAddress()` [here](#zksync-specific-opcodes).

#### **Return value of the deployment methods**

If the call succeeded, the address of the deployed contract is returned. If the deploy fails, the error bubbles up.

### DefaultAccount

The implementation of the default account abstraction. This is the code that is used by default for all addresses that are not in kernel space and have no contract deployed on them. This address:

- Contains minimal implementation of our account abstraction protocol. Note that it supports the [built-in paymaster flows](https://docs.zksync.io/build/developer-reference/account-abstraction/paymasters).
- When anyone (except bootloader) calls it, it behaves in the same way as a call to an EOA, i.e. it always returns `success = 1, returndatasize = 0` for calls from anyone except for the bootloader.

### L1Messenger

A contract used for sending arbitrary length L2→L1 messages from ZKsync to L1. While ZKsync natively supports a rather limited number of L1→L2 logs, which can transfer only roughly 64 bytes of data a time, we allowed sending nearly-arbitrary length L2→L1 messages with the following trick:

The L1 messenger receives a message, hashes it and sends only its hash as well as the original sender via L2→L1 log. Then, it is the duty of the L1 smart contracts to make sure that the operator has provided full preimage of this hash in the commitment of the batch.

Note, that L1Messenger is calls the L2DAValidator and plays an important role in facilitating the [DA validation protocol](../settlement_contracts/data_availability/custom_da.md).

### NonceHolder

Serves as storage for nonces for our accounts. Besides making it easier for operator to order transactions (i.e. by reading the current nonces of account), it also serves a separate purpose: making sure that the pair (address, nonce) is always unique.

It provides a function `validateNonceUsage` which the bootloader uses to check whether the nonce has been used for a certain account or not. Bootloader enforces that the nonce is marked as non-used before validation step of the transaction and marked as used one afterwards. The contract ensures that once marked as used, the nonce can not be set back to the “unused” state.

Note that nonces do not necessarily have to be monotonic (this is needed to support more interesting applications of account abstractions, e.g. protocols that can start transactions on their own, tornado-cash like protocols, etc). That’s why there are two ways to set a certain nonce as “used”:

- By incrementing the `minNonce` for the account (thus making all nonces that are lower than `minNonce` as used).
- By setting some non-zero value under the nonce via `setValueUnderNonce`. This way, this key will be marked as used and will no longer be allowed to be used as nonce for accounts. This way it is also rather efficient, since these 32 bytes could be used to store some valuable information.

The accounts upon creation can also provide which type of nonce ordering do they want: Sequential (i.e. it should be expected that the nonces grow one by one, just like EOA) or Arbitrary, the nonces may have any values. This ordering is not enforced in any way by system contracts, but it is more of a suggestion to the operator on how it should order the transactions in the mempool.

### EventWriter

A system contract responsible for emitting events.

It accepts in its 0-th extra abi data param the number of topics. In the rest of the extraAbiParams he accepts topics for the event to emit. Note, that in reality the event the first topic of the event contains the address of the account. Generally, the users should not interact with this contract directly, but only through Solidity syntax of `emit`-ing new events.

### Compressor

One of the most expensive resource for a rollup is data availability, so in order to reduce costs for the users we compress the published pubdata in several ways:

- We compress published bytecodes.
- We compress state diffs.

The contract provides two methods:

- `publishCompressedBytecode` that verifies the correctness of the bytecode compression and publishes it in form of a message to the DA layer.
- `verifyCompressedStateDiffs` that can verify the correctness of our standard state diff compression. This method can be used by common L2DAValidators and it is for instance utilized by the [RollupL2DAValidator](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l2-contracts/contracts/data-availability/RollupL2DAValidator.sol).

You can read more about how custom DA is handled [here](../settlement_contracts/data_availability/custom_da.md).

### Pubdata Chunk Publisher

This contract is responsible for separating pubdata into chunks that each fit into a [4844 blob](../settlement_contracts/data_availability/rollup_da.md) and calculating the hash of the preimage of said blob. If a chunk's size is less than the total number of bytes for a blob, we pad it on the right with zeroes as the circuits will require that the chunk is of exact size.

This contract can be utilized by L2DAValidators, e.g. [RollupL2DAValidator](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l2-contracts/contracts/data-availability/RollupL2DAValidator.sol) uses it to compress the pubdata into blobs.

### CodeOracle

It is a contract that accepts the versioned hash of a bytecode and returns the preimage of it. It is similar to the `extcodecopy` functionality on Ethereum.

It works the following way:

1. It accepts a versioned hash and double checks that it is marked as “known”, i.e. the operator must know the preimage for such hash.
2. After that, it uses the `decommit` opcode, which accepts the versioned hash and the number of ergs to spent, which is proportional to the length of the preimage. If the preimage has been decommitted before, the requested cost will be refunded to the user.

   Note, that the decommitment process does not only happen using the `decommit` opcode, but during calls to contracts. Whenever a contract is called, its code is decommitted into a memory page dedicated to contract code. We never decommit the same preimage twice, regardless of whether it was decommitted via an explicit opcode or during a call to another contract, the previous unpacked bytecode memory page will be reused. When executing `decommit` inside the `CodeOracle` contract, the user will be firstly precharged with maximal possible price and then it will be refunded in case the bytecode has been decommitted before.

3. The `decommit` opcode returns to the slice of the decommitted bytecode. Note, that the returned pointer always has length of 2^21 bytes, regardless of the length of the actual bytecode. So it is the job of the `CodeOracle` system contract to shrink the length of the returned data.

### P256Verify

This contract exerts the same behavior as the P256Verify precompile from [RIP-7212](https://github.com/ethereum/RIPs/blob/master/RIPS/rip-7212.md). Note, that since Era has different gas schedule, we do not comply with the gas costs, but otherwise the interface is identical.

### GasBoundCaller

This is not a system contract, but it will be predeployed on a fixed user space address. This contract allows users to set an upper bound of how much pubdata can a subcall take, regardless of the gas per pubdata. More on how pubdata works on ZKsync can be read [here](./zksync_fee_model.md).

Note, that it is a deliberate decision not to deploy this contract in the kernel space, since it can relay calls to any contracts and so may break the assumption that all system contracts can be trusted.

### ComplexUpgrader

Usually an upgrade is performed by calling the `forceDeployOnAddresses` function of ContractDeployer out of the name of the `FORCE_DEPLOYER` constant address. However some upgrades may require more complex interactions, e.g. query something from a contract to determine which calls to make etc.

For cases like this `ComplexUpgrader` contract has been created. The assumption is that the implementation of the upgrade is predeployed and the `ComplexUpgrader` would delegatecall to it.

> Note, that while `ComplexUpgrader` existed even in the previous upgrade, it lacked `forceDeployAndUpgrade` function. This caused some serious limitations. More on how the gateway upgrade process will look like can be read [here](../../upgrade_history/gateway_upgrade/upgrade_process_no_gateway_chain.md).

### Predeployed contracts

There are some contracts need to predeployed, but having kernel space rights is not desirable for them. Such contracts are usuallypredeployed at sequential addresses starting from `2^16`.

### Create2Factory

Just a built-in Create2Factory. It allows to deterministically deploy contracts to the samme address on multiple chains.

### L2GenesisUpgrade

A contract that is responsible for facilitating initialization of a newly created chain. This is part of a [chain creation flow](../chain_management/chain_genesis.md).

### Bridging-related contracts

`L2Bridgehub`, `L2AssetRouter`, `L2NativeTokenVault`, as well as `L2MessageRoot`.

These contracts are used to facilitate cross-chain communication as well value bridging. You can read more about then in [the asset router spec](../bridging/asset_router_and_ntv/asset_router.md).

Note, that [L2AssetRouter](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/bridge/asset-router/L2AssetRouter.sol) and [L2NativeTokenVault](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/bridge/ntv/L2NativeTokenVault.sol) have unique code, the L2Bridgehub and L2MessageRoot share the same source code with their L1 precompiles, i.e. the L2Bridgehub has [this](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/bridgehub/Bridgehub.sol) code and L2MessageRoot has [this](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/bridgehub/MessageRoot.sol) code.

### SloadContract

During the L2GatewayUpgrade, the system contracts will need to read the storage of some other contracts, despite those lacking getters. The how it is implemented can be read in the `forcedSload` function of the [SystemContractHelper](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/contracts/libraries/SystemContractHelper.sol) contract.

While it is only used for the upgrade, it was decided to leave it as a predeployed contract for future use-cases as well.

### L2WrappedBaseTokenImplementation

While bridging wrapped base tokens (e.g. WETH) is not yet supported. The address of it is enshrined within the native token vault (both the L1 and L2 one). For consistency with other networks, our WETH token is deployed as a TransparentUpgradeableProxy. To have the deployment process easier, we predeploy the implementation.

## Known issues to be resolved

The protocol, while conceptually complete, contains some known issues which will be resolved in the short to middle term.

- Fee modeling is yet to be improved. More on it in the [document](./zksync_fee_model.md) on the fee model.
- We may add some kind of default implementation for the contracts in the kernel space (i.e. if called, they wouldn’t revert but behave like an EOA).
