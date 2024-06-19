# Calls

All EVM call instructions are handled similarly.

The call type is encoded on the assembly level, so we will describe the common handling workflow, mentioning
distinctions if there are any.

For more information, see the
[ZKsync Era documentation](https://docs.zksync.io/build/developer-reference/ethereum-differences/evm-instructions#call-staticcall-delegatecall).

## [CALL](https://www.evm.codes/#f1?fork=shanghai)

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/call.rs#L530)
is common for Yul and EVMLA representations.

The code checks if the call is non-static and the Ether value is non-zero. If so, the call is redirected to the
[MsgValueSimulator](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/system_contracts.md#ether-value-simulator).

## [DELEGATECALL](https://www.evm.codes/#f4?fork=shanghai)

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/call.rs#L530)
is common for Yul and EVMLA representations.

## [STATICCALL](https://www.evm.codes/#fa?fork=shanghai)

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-llvm-context/blob/main/src/eravm/evm/call.rs#L530)
is common for Yul and EVMLA representations.
