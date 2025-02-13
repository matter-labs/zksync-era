# [POP](https://www.evm.codes/#50?fork=shanghai)

In Yul, only used to mark unused values, and is not translated to LLVM IR.

```solidity
pop(staticcall(gas(), address(), 0, 64, 0, 32))
```

For EVMLA, see
[EVM Legacy Assembly Translator](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/evmla_translator.md).

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/evmla/assembly/instruction/stack.rs#L108).

## [JUMPDEST](https://www.evm.codes/#5b?fork=shanghai)

Is not available in Yul.

Ignored in EVMLA. See
[EVM Legacy Assembly Translator](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/evmla_translator.md)
for more information.

## [PUSH](https://www.evm.codes/#5f?fork=shanghai) - [PUSH32](https://www.evm.codes/#7f?fork=shanghai)

Is not available in Yul.

For EVMLA, see
[EVM Legacy Assembly Translator](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/evmla_translator.md).

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/evmla/assembly/instruction/stack.rs#L10).

## [DUP1](https://www.evm.codes/#80?fork=shanghai) - [DUP16](https://www.evm.codes/#8f?fork=shanghai)

Is not available in Yul.

For EVMLA, see
[EVM Legacy Assembly Translator](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/evmla_translator.md).

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/evmla/assembly/instruction/stack.rs#L48).

## [SWAP1](https://www.evm.codes/#90?fork=shanghai) - [SWAP16](https://www.evm.codes/#9f?fork=shanghai)

Is not available in Yul.

For EVMLA, see
[EVM Legacy Assembly Translator](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/evmla_translator.md).

[The LLVM IR generator code](https://github.com/matter-labs/era-compiler-solidity/blob/main/src/evmla/assembly/instruction/stack.rs#L74).
