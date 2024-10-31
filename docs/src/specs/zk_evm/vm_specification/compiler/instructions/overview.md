# Instructions

In this specification, instructions are grouped by their relevance to the EVM instruction set:

- [Native EVM instructions](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/extensions/overview.md).
- [Yul auxiliary instructions](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/yul.md).
- [EVM legacy assembly auxiliary instructions](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/evmla.md).
- [ZKsync Era extensions](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/extensions/overview.md).

Most of the EVM native instructions are represented in both Yul and EVM legacy assembly IRs. If they are not, it is
stated explicitly in the description of each instruction.

## Addressing modes

EraVM is a register-based virtual machine with different addressing modes.  
It overrides all stack mechanics described in [the original EVM opcodes documentation](https://www.evm.codes/) including
errors they produce on EVM.

## Solidity Intermediate Representations (IRs)

Every instruction is translated via two IRs available in the Solidity compiler unless stated otherwise:

1. Yul
2. EVM legacy assembly

## Yul Extensions

At the moment there is no way of adding ZKsync-specific instructions to Yul as long as we use the official Solidity
compiler, which would produce an error on an unknown instruction.

There are two ways of supporting such instructions: one for Solidity and one for Yul.

### The Solidity Mode

In Solidity we have introduced **call simulations**. They are not actual calls, as they are substituted by our Yul
translator with the needed instruction, depending on the constant address. This way the Solidity compiler is not
optimizing them out and is not emitting compilation errors.

To see the list of available instructions, visit this page:

[ZKsync Era Extension Simulation (call)](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/extensions/call.md)

### The Yul Mode

The non-call ZKsync-specific instructions are only available in the Yul mode of **zksolc**.  
To have better compatibility, they are implemented as `verbatim` instructions with some predefined keys.

To see the list of available instructions, visit this page:

[ZKsync Era Extension Simulation (verbatim)](https://github.com/code-423n4/2023-10-zksync/blob/main/docs/VM%20Section/How%20compiler%20works/instructions/extensions/verbatim.md)
