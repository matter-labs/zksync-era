# VM Crate

This crate contains code that interacts with the VM (Virtual Machine). The VM itself is in a separate repository
[era-zk_evm][zk_evm_repo_ext].

## VM Dependencies

The VM relies on several subcomponents or traits, such as Memory and Storage. These traits are defined in the `zk_evm`
repository, while their implementations can be found in this crate, such as the storage implementation in
`oracles/storage.rs` and the Memory implementation in `memory.rs`.

Many of these implementations also support easy rollbacks and history, which is useful when creating a block with
multiple transactions and needing to return the VM to a previous state if a transaction doesn't fit.

## Running the VM

To interact with the VM, you must initialize it with `L1BatchEnv`, which represents the initial parameters of the batch,
`SystemEnv`, that represents the system parameters, and a reference to the Storage. To execute a transaction, you have
to push the transaction into the bootloader memory and call the `execute_next_transaction` method.

### Tracers

The VM implementation allows for the addition of `Tracers`, which are activated before and after each instruction. This
provides a more in-depth look into the VM, collecting detailed debugging information and logs. More details can be found
in the `tracer/` directory.

This VM also supports custom tracers. You can call the `inspect_next_transaction` method with a custom tracer and
receive the result of the execution.

### Bootloader

In the context of zkEVM, we usually think about transactions. However, from the VM's perspective, it runs a single
program called the bootloader, which internally processes multiple transactions.

### Rollbacks

The `VMInstance` in `vm.rs` allows for easy rollbacks. You can save the current state at any moment by calling
`make_snapshot()` and return to that state using `rollback_to_the_latest_snapshot()`.

This rollback affects all subcomponents, such as memory, storage, and events, and is mainly used if a transaction
doesn't fit in a block.

[zk_evm_repo]: https://github.com/matter-labs/zk_evm "internal zk EVM repo"
[zk_evm_repo_ext]: https://github.com/matter-labs/era-zk_evm "external zk EVM repo"
