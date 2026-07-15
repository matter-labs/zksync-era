# EVM gas emulation overview

The gas model can be one of the most tricky topics regarding EVM emulation since there are two interacting executions are supported at the same time. This document is intended to explain in more detail how the emulation of the EVM gas works.

## EVM emulation execution flow

From the point of view of EraVM, the EVM emulator is (almost) a regular EraVM contract. This contract is predefined and can be changed only during protocol upgrades. When an EVM contract is called, EraVM invokes the emulator code. The emulator loads the corresponding EVM bytecode and interprets it.

Thus, the emulation is executed on top of EraVM, uses EraVM opcodes and, most importantly, **pays for EraVM operations in native gas (ergs)**.

⚠️ To avoid confusion, native EraVM gas will be referred to as "ergs" further in the text.

This means that **ergs** are used to pay for all operations on EraVM (and therefore on ZK Chains). Gas and gaslimit values signed in user's transactions are specified in **ergs**. All refunds are also made by the virtual machine in ergs.

## EVM gas model emulation

Full EVM gas equivalence is necessary to provide an EVM-compatible environment and meet the assumptions made in contracts. However, the cost of EraVM operations differs, and EVM emulation requires multiple EraVM operations for each EVM operation. As a result, we emulate the EVM gas model.

As mentioned in the previous section, the EVM environment is virtual: it operates on top of EraVM and incurs costs for execution in EraVM ergs. The EVM emulator, however, has its own internal gas accounting system that corresponds to the EVM gas model. Each EVM opcode consumes a predefined amount of gas, according to the EVM specification. And if there is insufficient gas for an operation, the frame will be reverted.

⚠️ EVM gas is used only for compatibility in the EVM emulation mode. Underlying virtual machine is not aware about EVM gas and uses native ergs.

## Ergs to gas conversion rate for gas limit

EVM gas units are not equivalent to EraVM ergs. And both the EVM and EraVM do not provide mechanisms for an EOA or contract to specify how to convert gas from one unit to another. For practical use in the emulator, a predefined constant is used to convert gas limit from one unit to another.

⚠️ Current EraVM -> EVM gas limit conversion ratio is 5:1.

This means that if a user made a call to an EVM contract with 100,000 ergs, the EVM emulator will start execution with 20,000 EVM gas. At the same time, underlying EraVM will have all 100,000 ergs available for use and will refund any unused ergs at the end of transaction.

In the other direction, when calling an EraVM contract from EVM context, if 20,000 gas is provided, the emulator will <ins>try</ins> to pass 100,000 ergs (or less, if amount of ergs left is not enough).

## Out-of-ergs situation

Because EVM gas is virtual and it is not equivalent to EraVM ergs, situations are possible in which emulator has enough EVM gas to continue execution, but encounters `out-of-ergs` panic from EraVM. In this case we simply propagate special internal kind of panic and revert the **whole** EVM frames chain.

⚠️ EVM emulation can only be executed completely, up to returning from the EVM context (incl. EVM reverts), or completely rolled back.

This fact should be taken into account by smart contract developers:

❗**It is <ins>highly</ins> discouraged to use try-catch patterns with unknown contracts in EVM environment**❗

Technically, this problem is similar to classic gas-griefing issues in EVM contracts, if not handled appropriately.
