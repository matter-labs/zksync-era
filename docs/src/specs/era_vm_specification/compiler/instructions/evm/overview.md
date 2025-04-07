# Native EVM Instructions

Such instructions are grouped into the following categories according to
[the original reference](https://www.evm.codes/):

- [Arithmetic](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/arithmetic.md)
- [Logical](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/logical.md)
- [Bitwise](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/bitwise.md)
- [SHA3](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/sha3.md)
- [Environment](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/environment.md)
- [Block](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/block.md)
- [Stack](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/stack.md)
- [Memory](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/memory.md)
- [Logging](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/logging.md)
- [Call](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/call.md)
- [Create](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/create.md)
- [Return](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evm/return.md)

### EraVM Assembly

Assembly emitted for LLVM standard library functions depends on available optimizations which differ between versions.
If there is no assembly example under an instruction, compile a reproducing contract with the latest version of
`zksolc`.
