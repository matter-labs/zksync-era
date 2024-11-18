# zkEVM internals

## zkEVM clarifier

The ZKsync zkEVM plays a fundamentally different role in the zkStack than the EVM does in Ethereum. The EVM is used to
execute code in Ethereum's state transition function. This STF needs a client to implement and run it. Ethereum has a
multi-client philosophy, there are multiple clients, and they are written in Go, Rust, and other traditional programming
languages, all running and verifying the same STF.

We have a different set of requirements, we need to produce a proof that some client executed the STF correctly. The
first consequence is that the client needs to be hard-coded, we cannot have the same multi-client philosophy. This
client is the zkEVM, it can run the STF efficiently, including execution of smart contracts similarly to the EVM. The
zkEVM was also designed to be proven efficiently.

For efficiency reasons it the zkEVM is similar to the EVM. This makes executing smart programs inside of it easy. It
also has special features that are not in the EVM but are needed for the rollup's STF, storage, gas metering,
precompiles and other things. Some of these features are implemented as system contracts while others are built into the
VM. System Contracts are contracts with special permissions, deployed at predefined addresses. Finally, we have the
bootloader, which is also a contract, although it is not deployed at any address. This is the STF that is ultimately
executed by the zkEVM, and executes the transaction against the state.

<!-- kl to do *Add different abstraction levels diagram here:*-->

Full specification of the zkEVM is beyond the scope of this document. However, this section will give you most of the
details needed for understanding the L2 system smart contracts & basic differences between EVM and zkEVM. Note also that
usually understanding the EVM is needed for efficient smart contract development. Understanding the zkEVM goes beyond
this, it is needed for developing the rollup itself.

## Registers and memory management

On EVM, during transaction execution, the following memory areas are available:

- `memory` itself.
- `calldata` the immutable slice of parent memory.
- `returndata` the immutable slice returned by the latest call to another contract.
- `stack` where the local variables are stored.

Unlike EVM, which is stack machine, zkEVM has 16 registers. Instead of receiving input from `calldata`, zkEVM starts by
receiving a _pointer_ in its first register *(*basically a packed struct with 4 elements: the memory page id, start and
length of the slice to which it points to*)* to the calldata page of the parent. Similarly, a transaction can receive
some other additional data within its registers at the start of the program: whether the transaction should invoke the
constructor
[more about deployments here](https://github.com/matter-labs/zksync-era/blob/main/docs/specs/zk_evm/system_contracts.md#contractdeployer--immutablesimulator),
whether the transaction has `isSystem` flag, etc. The meaning of each of these flags will be expanded further in this
section.

_Pointers_ are separate type in the VM. It is only possible to:

- Read some value within a pointer.
- Shrink the pointer by reducing the slice to which pointer points to.
- Receive the pointer to the `returndata` as a calldata.
- Pointers can be stored only on stack/registers to make sure that the other contracts can not read `memory/returndata`
  of contracts they are not supposed to.
- A pointer can be converted to the u256 integer representing it, but an integer can not be converted to a pointer to
  prevent unallowed memory access.
- It is not possible to return a pointer that points to a memory page with id smaller than the one for the current page.
  What this means is that it is only possible to `return` only pointer to the memory of the current frame or one of the
  pointers returned by the subcalls of the current frame.

### Memory areas in zkEVM

For each frame, the following memory areas are allocated:

- _Heap_ (plays the same role as `memory` on Ethereum).
- _AuxHeap_ (auxiliary heap). It has the same properties as Heap, but it is used for the compiler to encode
  calldata/copy the `returndata` from the calls to system contracts to not interfere with the standard Solidity memory
  alignment.
- _Stack_. Unlike Ethereum, stack is not the primary place to get arguments for opcodes. The biggest difference between
  stack on zkEVM and EVM is that on ZKsync stack can be accessed at any location (just like memory). While users do not
  pay for the growth of stack, the stack can be fully cleared at the end of the frame, so the overhead is minimal.
- _Code_. The memory area from which the VM executes the code of the contract. The contract itself can not read the code
  page, it is only done implicitly by the VM.

Also, as mentioned in the previous section, the contract receives the pointer to the calldata.

### Managing returndata & calldata

Whenever a contract finishes its execution, the parent’s frame receives a _pointer_ as `returndata`. This pointer may
point to the child frame’s Heap/AuxHeap or it can even be the same `returndata` pointer that the child frame received
from some of its child frames.

The same goes with the `calldata`. Whenever a contract starts its execution, it receives the pointer to the calldata.
The parent frame can provide any valid pointer as the calldata, which means it can either be a pointer to the slice of
parent’s frame memory (heap or auxHeap) or it can be some valid pointer that the parent frame has received before as
calldata/returndata.

Contracts simply remember the calldata pointer at the start of the execution frame (it is by design of the compiler) and
remembers the latest received returndata pointer.

Some important implications of this is that it is now possible to do the following calls without any memory copying:

A → B → C

where C receives a slice of the calldata received by B.

The same goes for returning data:

A ← B ← C

There is no need to copy returned data if the B returns a slice of the returndata returned by C.

Note, that you can _not_ use the pointer that you received via calldata as returndata (i.e. return it at the end of the
execution frame). Otherwise, it would be possible that returndata points to the memory slice of the active frame and
allow editing the `returndata`. It means that in the examples above, C could not return a slice of its calldata without
memory copying.

Some of these memory optimizations can be seen utilized in the
[EfficientCall](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/libraries/EfficientCall.sol)
library that allows to perform a call while reusing the slice of calldata that the frame already has, without memory
copying.

### Returndata & precompiles

Some of the operations which are opcodes on Ethereum, have become calls to some of the system contracts. The most
notable examples are `Keccak256`, `SystemContext`, etc. Note, that, if done naively, the following lines of code would
work differently on ZKsync and Ethereum:

```solidity
pop(call(...))
keccak(...)
returndatacopy(...)
```

Since the call to keccak precompile would modify the `returndata`. To avoid this, our compiler does not override the
latest `returndata` pointer after calls to such opcode-like precompiles.

## zkEVM specific opcodes

While some Ethereum opcodes are not supported out of the box, some of the new opcodes were added to facilitate the
development of the system contracts.

Note, that this lists does not aim to be specific about the internals, but rather explain methods in the
[SystemContractHelper.sol](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/libraries/SystemContractHelper.sol)

### **Only for kernel space**

These opcodes are allowed only for contracts in kernel space (i.e. system contracts). If executed in other places they
result in `revert(0,0)`.

- `mimic_call`. The same as a normal `call`, but it can alter the `msg.sender` field of the transaction.
- `to_l1`. Sends a system L2→L1 log to Ethereum. The structure of this log can be seen
  [here](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/contracts/ethereum/contracts/zksync/Storage.sol#L47).
- `event`. Emits an L2 log to ZKsync. Note, that L2 logs are not equivalent to Ethereum events. Each L2 log can emit 64
  bytes of data (the actual size is 88 bytes, because it includes the emitter address, etc). A single Ethereum event is
  represented with multiple `event` logs constitute. This opcode is only used by `EventWriter` system contract.
- `precompile_call`. This is an opcode that accepts two parameters: the uint256 representing the packed parameters for
  it as well as the ergs to burn. Besides the price for the precompile call itself, it burns the provided ergs and
  executes the precompile. The action that it does depend on `this` during execution:
  - If it is the address of the `ecrecover` system contract, it performs the ecrecover operation
  - If it is the address of the `sha256`/`keccak256` system contracts, it performs the corresponding hashing operation.
  - It does nothing (i.e. just burns ergs) otherwise. It can be used to burn ergs needed for L2→L1 communication or
    publication of bytecodes onchain.
- `setValueForNextFarCall` sets `msg.value` for the next `call`/`mimic_call`. Note, that it does not mean that the value
  will be really transferred. It just sets the corresponding `msg.value` context variable. The transferring of ETH
  should be done via other means by the system contract that uses this parameter. Note, that this method has no effect
  on `delegatecall` , since `delegatecall` inherits the `msg.value` of the previous frame.
- `increment_tx_counter` increments the counter of the transactions within the VM. The transaction counter used mostly
  for the VM’s internal tracking of events. Used only in bootloader after the end of each transaction.

Note, that currently we do not have access to the `tx_counter` within VM (i.e. for now it is possible to increment it
and it will be automatically used for logs such as `event`s as well as system logs produced by `to_l1`, but we can not
read it). We need to read it to publish the _user_ L2→L1 logs, so `increment_tx_counter` is always accompanied by the
corresponding call to the
[SystemContext](https://github.com/matter-labs/zksync-era/blob/main/docs/specs/zk_evm/system_contracts.md#systemcontext)
contract.

More on the difference between system and user logs can be read
[here](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/Smart%20contract%20Section/Handling%20pubdata%20in%20Boojum.md). -
`set_pubdata_price` sets the price (in gas) for publishing a single byte of pubdata.

### **Generally accessible**

Here are opcodes that can be generally accessed by any contract. Note that while the VM allows to access these methods,
it does not mean that this is easy: the compiler might not have convenient support for some use-cases yet.

- `near_call`. It is basically a “framed” jump to some location of the code of your contract. The difference between the
  `near_call` and ordinary jump are:
  1. It is possible to provide an ergsLimit for it. Note, that unlike “`far_call`”s (i.e. calls between contracts) the
     63/64 rule does not apply to them.
  2. If the near call frame panics, all state changes made by it are reversed. Please note, that the memory changes will
     **not** be reverted.
- `getMeta`. Returns an u256 packed value of
  [ZkSyncMeta](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/contracts/libraries/SystemContractHelper.sol#L42)
  struct. Note that this is not tight packing. The struct is formed by the
  [following rust code](https://github.com/matter-labs/era-zkevm_opcode_defs/blob/c7ab62f4c60b27dfc690c3ab3efb5fff1ded1a25/src/definitions/abi/meta.rs#L4).
- `getCodeAddress` — receives the address of the executed code. This is different from `this` , since in case of
  delegatecalls `this` is preserved, but `codeAddress` is not.

### Flags for calls

Besides the calldata, it is also possible to provide additional information to the callee when doing `call` ,
`mimic_call`, `delegate_call`. The called contract will receive the following information in its first 12 registers at
the start of execution:

- _r1_ — the pointer to the calldata.
- _r2_ — the pointer with flags of the call. This is a mask, where each bit is set only if certain flags have been set
  to the call. Currently, two flags are supported: 0-th bit: `isConstructor` flag. This flag can only be set by system
  contracts and denotes whether the account should execute its constructor logic. Note, unlike Ethereum, there is no
  separation on constructor & deployment bytecode. More on that can be read
  [here](https://github.com/matter-labs/zksync-era/blob/main/docs/specs/zk_evm/system_contracts.md#contractdeployer--immutablesimulator).
  1-st bit: `isSystem` flag. Whether the call intends a system contracts’ function. While most of the system contracts’
  functions are relatively harmless, accessing some with calldata only may break the invariants of Ethereum, e.g. if the
  system contract uses `mimic_call`: no one expects that by calling a contract some operations may be done out of the
  name of the caller. This flag can be only set if the callee is in kernel space.
- The rest r3..r12 registers are non-empty only if the `isSystem` flag is set. There may be arbitrary values passed,
  which we call `extraAbiParams`.

The compiler implementation is that these flags are remembered by the contract and can be accessed later during
execution via special
[simulations](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/extensions/overview.md).

If the caller provides inappropriate flags (i.e. tries to set `isSystem` flag when callee is not in the kernel space),
the flags are ignored.

### `onlySystemCall` modifier

Some of the system contracts can act on behalf of the user or have a very important impact on the behavior of the
account. That’s why we wanted to make it clear that users can not invoke potentially dangerous operations by doing a
simple EVM-like `call`. Whenever a user wants to invoke some of the operations which we considered dangerous, they must
provide “`isSystem`” flag with them.

The `onlySystemCall` flag checks that the call was either done with the “isSystemCall” flag provided or the call is done
by another system contract (since Matter Labs is fully aware of system contracts).

### Simulations via our compiler

In the future, we plan to introduce our “extended” version of Solidity with more supported opcodes than the original
one. However, right now it was beyond the capacity of the team to do, so in order to represent accessing ZKsync-specific
opcodes, we use `call` opcode with certain constant parameters that will be automatically replaced by the compiler with
zkEVM native opcode.

Example:

```solidity
function getCodeAddress() internal view returns (address addr) {
  address callAddr = CODE_ADDRESS_CALL_ADDRESS;
  assembly {
    addr := staticcall(0, callAddr, 0, 0xFFFF, 0, 0)
  }
}

```

In the example above, the compiler will detect that the static call is done to the constant `CODE_ADDRESS_CALL_ADDRESS`
and so it will replace it with the opcode for getting the code address of the current execution.

Full list of opcode simulations can be found
[here](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/extensions/call.md).

We also use
[verbatim-like](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/extensions/verbatim.md)
statements to access ZKsync-specific opcodes in the bootloader.

All the usages of the simulations in our Solidity code are implemented in the
[SystemContractHelper](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/libraries/SystemContractHelper.sol)
library and the
[SystemContractsCaller](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/libraries/SystemContractsCaller.sol)
library.

**Simulating** `near_call` **(in Yul only)**

In order to use `near_call` i.e. to call a local function, while providing a limit of ergs (gas) that this function can
use, the following syntax is used:

The function should contain `ZKSYNC_NEAR_CALL` string in its name and accept at least 1 input parameter. The first input
parameter is the packed ABI of the `near_call`. Currently, it is equal to the number of ergs to be passed with the
`near_call`.

Whenever a `near_call` panics, the `ZKSYNC_CATCH_NEAR_CALL` function is called.

_Important note:_ the compiler behaves in a way that if there is a `revert` in the bootloader, the
`ZKSYNC_CATCH_NEAR_CALL` is not called and the parent frame is reverted as well. The only way to revert only the
`near_call` frame is to trigger VM’s _panic_ (it can be triggered with either invalid opcode or out of gas error).

_Important note 2:_ The 63/64 rule does not apply to `near_call`. Also, if 0 gas is provided to the near call, then
actually all of the available gas will go to it.

### Notes on security

To prevent unintended substitution, the compiler requires `--system-mode` flag to be passed during compilation for the
above substitutions to work.

## Bytecode hashes

On ZKsync the bytecode hashes are stored in the following format:

- The 0th byte denotes the version of the format. Currently the only version that is used is “1”.
- The 1st byte is `0` for deployed contracts’ code and `1` for the contract code
  [that is being constructed](https://github.com/matter-labs/zksync-era/blob/main/docs/specs/zk_evm/system_contracts.md#constructing-vs-non-constructing-code-hash).
- The 2nd and 3rd bytes denote the length of the contract in 32-byte words as big-endian 2-byte number.
- The next 28 bytes are the last 28 bytes of the sha256 hash of the contract’s bytecode.

The bytes are ordered in little-endian order (i.e. the same way as for `bytes32` ).

### Bytecode validity

A bytecode is valid if it:

- Has its length in bytes divisible by 32 (i.e. consists of an integer number of 32-byte words).
- Has a length of less than 2^16 words (i.e. its length in words fits into 2 bytes).
- Has an odd length in words (i.e. the 3rd byte is an odd number).

Note, that it does not have to consist of only correct opcodes. In case the VM encounters an invalid opcode, it will
simply revert (similar to how EVM would treat them).

A call to a contract with invalid bytecode can not be proven. That is why it is **essential** that no contract with
invalid bytecode is ever deployed on ZKsync. It is the job of the
[KnownCodesStorage](https://github.com/matter-labs/zksync-era/blob/main/docs/specs/zk_evm/system_contracts.md#knowncodestorage)
to ensure that all allowed bytecodes in the system are valid.
