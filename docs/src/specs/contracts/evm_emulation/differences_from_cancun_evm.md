# General differences from Ethereum

This feature allows EVM emulation on top of EraVM. This mode is not fully equivalent to native EVM and is limited by the EraVM design. This page describes the known differences between emulation and EVM (Cancun).

## Gas behavior differences

- EVM emulation is executed on top of EraVM and all transactions start in EraVM environment. So gas and gaslimit values signed in the transaction correspond to the native **EraVM** gas, not **EVM** gas. For that reason presigned/keyless transactions created for Ethereum may be not compatible with ZK Chains. For detailed info: [Gas emulation](./evm_gas_emulation.md).
- Our “Intrinsic gas costs” are different from EVM.
- We do not implement the gas refunds logic from **EVM**. Because users pay for EraVM gas and EVM gas is virtual, we use EraVM gas refunds logic instead of EVM one. It does not affect contracts behavior (refunds happen in the end of transaction).
- We do not charge EVM gas for tx calldata.
- Access lists are not supported (EIP-2930).

## Limitations

- `DELEGATECALL` between EVM and native EraVM contracts will be reverted.
- Calls to empty addresses in kernel space (address < 2^16) will fail.
- `GASLIMIT` opcode returns the same fixed constant as EraVM and should not be used.

Unsupported opcodes:

- `CALLCODE`
- `SELFDESTRUCT`
- `BLOBHASH`
- `BLOBBASEFEE`

## Precompiles

EVM emulation supports the same precompiles that are supported by EraVM.

## Technical differences

_These changes are unlikely to have an impact on the developer experience._

Differences:

- `JUMPDEST` analysis is simplified. It is not checked that `JUMPDEST` is not a part of `PUSH` instruction.
- No force of call stack depth limit. It is implicitly implemented by 63/64 gas rule.
- Account storage is not destroyed during contract deployment.
- If the deployer's nonce is overflowed during contract deployment, all passed gas will be consumed. EVM refunds all passed gas to the caller frame.
- Nonces are limited by size of `u128`, not `u64`
- During creation of EVM contract by EOA or EraVM contract, emulator does not charge additional `2` gas for every 32-byte chunk of `initcode` as specified in [EIP-3860](https://eips.ethereum.org/EIPS/eip-3860) (since perform `JUMPDEST` analysis is simplified). This cost **is** charged if contract is created by another EVM contract (to keep gas equivalence).
- Code deposit cost is charged from constructor frame, not caller frame. It will be changed in the future.
- Only those accounts that are accessed from an EVM environment become warm (including origin, sender, coinbase, precompiles). Anything that happens outside the EVM does not affect the warm/cold status of the accounts for EVM.
