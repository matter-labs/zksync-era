# zkEVM

The zkEVM is used to execute transactions. It is similar in construction to the EVM, so it executes transactions
similarly, but it plays a fundamentally different role in the zkStack than the EVM does in Ethereum. The EVM is used to
execute smart contracts in Ethereum's state transition function. This STF needs a client to implement and run it.

Rollups have a different set of requirements, they need to produce a proof that some client executed the STF correctly.
This client is the zkEVM, it is made to run the STF efficiently. The STF is the [Bootloader](./bootloader.md).

The smart contracts are native zkEVM bytecode, zkEVM can execute these easily. In the future the ZK Stack will also
support EVM bytecode by running an efficient interpreter inside the zkEVM.

The zkEVM has a lot of special features compared to the EVM that are needed for the rollup's STF, storage, gas metering,
precompiles etc. These functions are either built into the zkEVM, or there are special
[system contracts](./system_contracts.md) for them. The system contracts are deployed at predefined addresses, they are
called by the Bootloader, and they have special permissions compared to normal user contracts. These are not to be
confused with the precompiles, which are also pre-deployed contracts with special support from the
zkEVM, but these contract do not have special permissions and are called by the users and not the Bootloader.

The zkEVM also has user-facing features. For the best possible UX the ZK Stack supports native
[account abstraction](./account_abstraction.md). This means users can fully customize how they pay the fees needed to
execute their transactions.

All transactions need to pay fees. The requirements to run a rollup are different than the ones needed to run Ethereum,
so the ZK Stack has a different [fee model](./zksync_fee_model.md). The fee model is designed to consider all the components
that are needed to run the rollup: data and proof execution costs on L1, sequencer costs, and prover costs.