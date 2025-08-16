# Handling L1→L2 operations

The transactions on ZKsync can be initiated not only on L2, but also on L1. There are two types of transactions that can
be initiated on L1:

- Priority operations. These are the kind of operations that any user can create.
- Upgrade transactions. These can be created only during upgrades.

### Prerequisites

Please read [articles](../../../zkevm/overview.md)
on the general system contracts / bootloader structure as well as the pubdata structure with Boojum system to understand
[the difference](../../data_availability/pubdata.md)
between system and user logs.

## Priority operations

### Initiation

A new priority operation can be appended by calling the `requestL2TransactionDirect` or `requestL2TransactionTwoBridges` methods on `BridgeHub` smart contract. `BridgeHub` will ensure that the base token is deposited via `L1AssetRouter` and send transaction request to the specified state transition contract (selected by the chainID). State transition contract will perform several checks for the transaction, making sure that it is processable and provides enough fee to compensate the operator for this transaction. Then, this transaction will be [appended](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/state-transition/chain-deps/facets/Mailbox.sol#569) to the priority tree (and optionally to the legacy priority queue).

> In the previous system, priority operations were structured in a queue. However, now they will be stored in an incremental merkle tree. The motivation for the tree structure can be read [here](../priority-queue.md).

The difference between `requestL2TransactionDirect` and `requestL2TransactionTwoBridges` is that the `msg.sender` on the L2 Transaction is the second bridge in the `requestL2TransactionTwoBridges` case, while it is the `msg.sender` of the `requestL2TransactionDirect` in the first case. For more details read the [bridgehub documentation](../../../interop/interop_center/overview.md)

The struct called in the `bridgehubRequestL2Transaction` method is the following: 
```solidity
struct BridgehubL2TransactionRequest {
    address sender;
    address contractL2;
    uint256 mintValue;
    uint256 l2Value;
    bytes l2Calldata;
    uint256 l2GasLimit;
    uint256 l2GasPerPubdataByteLimit;
    bytes[] factoryDeps;
    address refundRecipient;
}
```
- `sender` is the address of the user that initiated the transaction. Will be used as msg.sender for the L2 transaction.
- `contractL2` is the address of the contract on L2 to call.
- `mintValue` is the amount of base token that should be minted on L2 as the result of this transaction. This includes msg.value + value used for gas payment.
- `l2Value` is the msg.value of the L2 transaction.
- `l2Calldata` is the calldata for the L2 transaction.
- `l2GasLimit` is the limit of the L2 gas for the L2 transaction
- `l2GasPerPubdataByteLimit` is the price for a single pubdata byte in L2 gas.
- `factoryDeps` is the array of L2 bytecodes that the tx depends on.
- `refundRecipient` is the recipient of the refund for the transaction on L2. If the transaction fails, then
this address will receive the `l2Value`.

### Bootloader

Whenever an operator sees a priority operation, it can include the transaction into the batch. While for normal L2
transaction the account abstraction protocol will ensure that the `msg.sender` has indeed agreed to start a transaction
out of this name, for L1→L2 transactions there is no signature verification. In order to verify that the operator
includes only transactions that were indeed requested on L1, the bootloader
[maintains](https://github.com/matter-labs/era-contracts/tree/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L970)
two variables:

- `numberOfPriorityTransactions` (maintained at `PRIORITY_TXS_L1_DATA_BEGIN_BYTE` of bootloader memory)
- `priorityOperationsRollingHash` (maintained at `PRIORITY_TXS_L1_DATA_BEGIN_BYTE + 32` of the bootloader memory)

Whenever a priority transaction is processed, the `numberOfPriorityTransactions` gets incremented by 1, while
`priorityOperationsRollingHash` is assigned to `keccak256(priorityOperationsRollingHash, processedPriorityOpHash)`,
where `processedPriorityOpHash` is the hash of the priority operations that has been just processed.

Also, for each priority transaction, we
[emit](https://github.com/matter-labs/era-contracts/tree/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L966)
a user L2→L1 log with its hash and result, which basically means that it will get Merklized and users will be able to
prove on L1 that a certain priority transaction has succeeded or failed (which can be helpful to reclaim your funds from
bridges if the L2 part of the deposit has failed).

Then, at the end of the batch, we
[submit](https://github.com/matter-labs/era-contracts/tree/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/system-contracts/bootloader/bootloader.yul#L3819)
2 L2→L1 log system log with these values.

### Batch commit

During batch commit, the contract will remember those values, but not validate them in any way.

### Batch execution

During batch execution, we will check that the `priorityOperationsRollingHash` rolling hash provided before was correct. There are two ways to do it:

- [Legacy one that uses priority queue](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/state-transition/chain-deps/facets/Executor.sol#L397). We will pop `numberOfPriorityTransactions` from the top of priority queue and verify that the hashes match.
- [The new one that uses priority tree](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/state-transition/chain-deps/facets/Executor.sol#L397). The operator would have to provide the hashes of these priority operations in an array, as well as proof that this entire segment belongs to the merkle tree. After it is verified that this array of leaves is correct, it will be checked whether the rolling hash of those is equal to the `priorityOperationsRollingHash`.

## Upgrade transactions

### Initiation

Upgrade transactions can only be created during a system upgrade. It is done if the `DiamondProxy` delegatecalls to the implementation that manually puts this transaction into the storage of the DiamondProxy, this could happen on calling `upgradeChainFromVersion` function in `Admin.sol` on the State Transition contract. 
Note, that since it happens during the upgrade, there is no “real” checks on the structure of this transaction. We do have [some validation](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/upgrades/BaseZkSyncUpgrade.sol#L175),
but it is purely on the side of the implementation which the `DiamondProxy` delegatecalls to and so may be lifted if they implementation is changed.

The hash of the currently required upgrade transaction is
under `l2SystemContractsUpgradeTxHash` [variable](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/state-transition/chain-deps/ZKChainStorage.sol#L127).

We will also track the batch where the upgrade has been committed in the `l2SystemContractsUpgradeBatchNumber`
[variable](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/state-transition/chain-deps/ZKChainStorage.sol#L130).

We can not support multiple upgrades in parallel, i.e. the next upgrade should start only after the previous one has
been complete.

### Bootloader

The upgrade transactions are processed just like with priority transactions, with only the following differences:

- We can have only one upgrade transaction per batch & this transaction must be the first transaction in the batch.
- The system contracts upgrade transaction is not appended to `priorityOperationsRollingHash` and doesn't increment
  `numberOfPriorityTransactions`. Instead, its hash is calculated via a system L2→L1 log _before_ it gets executed.
  Note, that it is an important property. More on it [below](#security-considerations).

### Commit

After an upgrade has been initiated, it will be required that the next commit batches operation already contains the
system upgrade transaction. It is
[checked](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/state-transition/chain-deps/facets/Executor.sol#L157)
by verifying the corresponding L2→L1 log.

We also remember that the upgrade transaction has been processed in this batch (by amending the
`l2SystemContractsUpgradeBatchNumber` variable).

### Revert

In a very rare event when the team needs to revert the batch with the upgrade on ZKsync, the
`l2SystemContractsUpgradeBatchNumber` is
[reset](https://github.com/matter-labs/era-contracts/blob/1fb28d2e3bbc3cade42f256c77f5454482bd281e/l1-contracts/contracts/state-transition/chain-deps/facets/Executor.sol#L412).

Note, however, that we do not “remember” that certain batches had a version before the upgrade, i.e. if the reverted
batches will have to be re-executed, the upgrade transaction must still be present there, even if some of the deleted
batches were committed before the upgrade and thus didn’t contain the transaction.

### Execute

Once batch with the upgrade transaction has been executed, we
[delete](https://github.com/matter-labs/era-contracts/blob/b43cf6b3b069c85aec3cd61d33dd3ae2c462c896/l1-contracts/contracts/state-transition/chain-deps/facets/Executor.sol#L304)
them from storage for efficiency to signify that the upgrade has been fully processed and that a new upgrade can be
initiated.

## Security considerations

Since the operator can put any data into the bootloader memory and for L1→L2 transactions the bootloader has to blindly
trust it and rely on L1 contracts to validate it, it may be a very powerful tool for a malicious operator. Note, that
while the governance mechanism is generally trusted, we try to limit our trust for the operator as much as possible,
since in the future anyone would be able to become an operator.

Some time ago, we _used to_ have a system where the upgrades could be done via L1→L2 transactions, i.e. the implementation of the `DiamondProxy` upgrade would [include](https://github.com/matter-labs/era-contracts/blob/f06a58360a2b8e7129f64413998767ac169d1efd/ethereum/contracts/zksync/upgrade-initializers/DIamondUpgradeInit2.sol#L27) a priority transaction (with `from` equal to for instance `FORCE_DEPLOYER`) with all the upgrade params.

In the Boojum though having such logic would be dangerous and would allow for the following attack:

- Let’s say that we have at least 1 priority operations in the priority queue. This can be any operation, initiated by
  anyone.
- The operator puts a malicious priority operation with an upgrade into the bootloader memory. This operation was never
  included in the priority operations queue / and it is not an upgrade transaction. However, as already mentioned above
  the bootloader has no idea what priority / upgrade transactions are correct and so this transaction will be processed.

The most important caveat of this malicious upgrade is that it may change implementation of the `Keccak256` precompile
to return any values that the operator needs.

- When the`priorityOperationsRollingHash` will be updated, instead of the “correct” rolling hash of the priority
  transactions, the one which would appear with the correct topmost priority operation is returned. The operator can’t
  amend the behaviour of `numberOfPriorityTransactions`, but it won’t help much, since the
  the`priorityOperationsRollingHash` will match on L1 on the execution step.

That’s why the concept of the upgrade transaction is needed: this is the only transaction that can initiate transactions
out of the kernel space and thus change bytecodes of system contracts. That’s why it must be the first one and that’s
why
[emit](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L587)
its hash via a system L2→L1 log before actually processing it.

### Why it doesn’t break on the previous version of the system

This section is not required for Boojum understanding but for those willing to analyze the production system that is
deployed at the time of this writing.

Note that the hash of the transaction is calculated before the transaction is executed:
[https://github.com/matter-labs/era-system-contracts/blob/3e954a629ad8e01616174bde2218241b360fda0a/bootloader/bootloader.yul#L1055](https://github.com/matter-labs/era-system-contracts/blob/3e954a629ad8e01616174bde2218241b360fda0a/bootloader/bootloader.yul#L1055)

And then we publish its hash on L1 via a _system_ L2→L1 log:
[https://github.com/matter-labs/era-system-contracts/blob/3e954a629ad8e01616174bde2218241b360fda0a/bootloader/bootloader.yul#L1133](https://github.com/matter-labs/era-system-contracts/blob/3e954a629ad8e01616174bde2218241b360fda0a/bootloader/bootloader.yul#L1133)

In the new upgrade system, the `priorityOperationsRollingHash` is calculated on L2 and so if something in the middle
changes the implementation of `Keccak256`, it may lead to the full `priorityOperationsRollingHash` be maliciously
crafted. In the pre-Boojum system, we publish all the hashes of the priority transactions via system L2→L1 and then the
rolling hash is calculated on L1. This means that if at least one of the hash is incorrect, then the entire rolling hash
will not match also.
