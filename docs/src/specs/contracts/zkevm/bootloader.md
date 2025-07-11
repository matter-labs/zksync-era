# Bootloader

On standard Ethereum clients, the workflow for executing blocks is the following:

1. Pick a transaction, validate the transactions & charge the fee, execute it
2. Gather the state changes (if the transaction has not reverted), apply them to the state.
3. Go back to step (1) if the block gas limit has not been yet exceeded.

However, having such flow on ZKsync (i.e. processing transaction one-by-one) would be too inefficient, since we have to
run the entire proving workflow for each individual transaction. That’s why we need the _bootloader_: instead of running
N transactions separately, we run the entire batch (set of blocks, more can be found
[here](./batches_and_blocks_on_zksync.md))
as a single program that accepts the array of transactions as well as some other batch metadata and processes them
inside a single big “transaction”. The easiest way to think about bootloader is to think in terms of EntryPoint from
EIP4337: it also accepts the array of transactions and facilitates the Account Abstraction protocol.

The hash of the code of the bootloader is stored on L1 and can only be changed as a part of a system upgrade. Note, that
unlike system contracts, the bootloader’s code is not stored anywhere on L2. That’s why we may sometimes refer to the
bootloader’s address as formal. It only exists for the sake of providing some value to `this` / `msg.sender`/etc. When
someone calls the bootloader address (e.g. to pay fees) the EmptyContract’s code is actually invoked.

Bootloader is the program that accepts an array of transactions and executes the entire ZKsync batch. This section will
expand on its invariants and methods.

## Playground bootloader vs proved bootloader

For convenience, we use the same implementation of the bootloader both in the mainnet batches and for emulating ethCalls
or other testing activities. _Only_ _proved_ bootloader is ever used for batch-building and thus this document only
describes it.

## Start of the batch

It is enforced by the ZKPs, that the state of the bootloader is equivalent to the state of a contract transaction with
empty calldata. The only difference is that it starts with all the possible memory pre-allocated (to avoid costs for
memory expansion).

For additional efficiency (and our convenience), the bootloader receives its parameters inside its memory. This is the
only point of non-determinism: the bootloader _starts with its memory pre-filled with any data the operator wants_.
That’s why it is responsible for validating the correctness of it and it should never rely on the initial contents of
the memory to be correct & valid.

For instance, for each transaction, we check that it is
[properly ABI-encoded](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L3058)
and that the transactions
[go exactly one after another](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L3736).
We also ensure that transactions do not exceed the limits of the memory space allowed for transactions.

## Transaction types & their validation

While the main transaction format is the internal `Transaction`
[format](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/contracts/libraries/TransactionHelper.sol#L25),
it is a struct that is used to represent various kinds of transactions types. It contains a lot of `reserved` fields
that could be used depending in the future types of transactions without need for AA to change the interfaces of their
contracts.

The exact type of the transaction is marked by the `txType` field of the transaction type. There are 6 types currently
supported:

- `txType`: 0. It means that this transaction is of legacy transaction type. The following restrictions are enforced:
- `maxFeePerErgs=getMaxPriorityFeePerErg` since it is pre-EIP1559 tx type.
- `reserved1..reserved4` as well as `paymaster` are 0. `paymasterInput` is zero.
- Note, that unlike type 1 and type 2 transactions, `reserved0` field can be set to a non-zero value, denoting that this
  legacy transaction is EIP-155-compatible and its RLP encoding (as well as signature) should contain the `chainId` of
  the system.
- `txType`: 1. It means that the transaction is of type 1, i.e. transactions access list. ZKsync does not support access
  lists in any way, so no benefits of fulfilling this list will be provided. The access list is assumed to be empty. The
  same restrictions as for type 0 are enforced, but also `reserved0` must be 0.
- `txType`: 2. It is EIP1559 transactions. The same restrictions as for type 1 apply, but now `maxFeePerErgs` may not be
  equal to `getMaxPriorityFeePerErg`.
- `txType`: 113. It is ZKsync transaction type. This transaction type is intended for AA support. The only restriction
  that applies to this transaction type: fields `reserved0..reserved4` must be equal to 0.
- `txType`: 254. It is a transaction type that is used for upgrading the L2 system. This is the only type of transaction
  is allowed to start a transaction out of the name of the contracts in kernel space.
- `txType`: 255. It is a transaction that comes from L1. There are almost no restrictions explicitly imposed upon this
  type of transaction, since the bootloader at the end of its execution sends the rolling hash of the executed priority
  transactions. The L1 contract ensures that the hash did indeed match the
  [hashes of the priority transactions on L1](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/contracts/ethereum/contracts/zksync/facets/Executor.sol#L282).

You can also read more on L1->L2 transactions and upgrade transactions
[here](../settlement_contracts/priority_queue/l1_l2_communication/l1_to_l2.md).

However, as already stated, the bootloader’s memory is not deterministic and the operator is free to put anything it
wants there. For all of the transaction types above the restrictions are imposed in the following
([method](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L2828)),
which is called before starting processing the transaction.

## Structure of the bootloader’s memory

The bootloader expects the following structure of the memory (here by word we denote 32-bytes, the same machine word as
on EVM):

### **Batch information**

The first 8 words are reserved for the batch information provided by the operator.

- `0` word — the address of the operator (the beneficiary of the transactions).
- `1` word — the hash of the previous batch. Its validation will be explained later on.
- `2` word — the timestamp of the current batch. Its validation will be explained later on.
- `3` word — the number of the new batch.
- `4` word — the L1 gas price provided by the operator.
- `5` word — the “fair” price for L2 gas, i.e. the price below which the `baseFee` of the batch should not fall. For
  now, it is provided by the operator, but it in the future it may become hardcoded.
- `6` word — the base fee for the batch that is expected by the operator. While the base fee is deterministic, it is
  still provided to the bootloader just to make sure that the data that the operator has coincides with the data
  provided by the bootloader.
- `7` word — reserved word. Unused on proved batch.

The batch information slots
[are used at the beginning of the batch](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L3629).
Once read, these slots can be used for temporary data.

### **Temporary data for debug & transaction processing purposes**

- `[8..39]` – reserved slots for debugging purposes
- `[40..72]` – slots for holding the paymaster context data for the current transaction. The role of the paymaster
  context is similar to the [EIP4337](https://eips.ethereum.org/EIPS/eip-4337)’s one. You can read more about it in the
  account abstraction documentation.
- `[73..74]` – slots for signed and explorer transaction hash of the currently processed L2 transaction.
- `[75..110]` – 36 slots for the calldata for the KnownCodesContract call.
- `[111..1134]` – 1024 slots for the refunds for the transactions.
- `[1135..2158]` – 1024 slots for the overhead for batch for the transactions. This overhead is suggested by the
  operator, i.e. the bootloader will still double-check that the operator does not overcharge the user.
- `[2159..3182]` – slots for the “trusted” gas limits by the operator. The user’s transaction will have at its disposal
  `min(MAX_TX_GAS(), trustedGasLimit)`, where `MAX_TX_GAS` is a constant guaranteed by the system. Currently, it is
  equal to 80 million gas. In the future, this feature will be removed.
- `[3183..7282]` – slots for storing L2 block info for each transaction. You can read more on the difference L2 blocks
  and batches
  [here](./batches_and_blocks_on_zksync.md).
- `[7283..40050]` – slots used for compressed bytecodes each in the following format:
  - 32 bytecode hash
  - 32 zeroes (but then it will be modified by the bootloader to contain 28 zeroes and then the 4-byte selector of the
    `publishCompressedBytecode` function of the `BytecodeCompressor`)
  - The calldata to the bytecode compressor (without the selector).
- `[40051..40052]` – slots where the hash and the number of current priority ops is stored. More on it in the priority
  operations
  [section](../settlement_contracts/priority_queue/README.md).

### L1Messenger Pubdata

- `[40053..248052]` – slots where the final batch pubdata is supplied to be verified by the L1Messenger. More on how the
  L1Messenger system contracts handles the pubdata can be read
  [here](../settlement_contracts/data_availability/pubdata.md).

But briefly, this space is used for the calldata to the L1Messenger’s `publishPubdataAndClearState` function, which
accepts the list of the user L2→L1 logs, published L2→L1 messages as well as bytecodes. It also takes the list of full
state diff entries, which describe how each storage slot has changed as well as compressed state diffs. This method will
then check the correctness of the provided data and publish the hash of the correct pubdata to L1.

Note, that while the realistic number of pubdata that can be published in a batch is 120kb, the size of the calldata to
L1Messenger may be a lot larger due to the fact that this method also accepts the original uncompressed state diff
entries. These will not be published to L1, but will be used to verify the correctness of the compression. The
worst-case number of bytes that may be needed for this scratch space is if all the pubdata consists of repeated writes
(i.e. we’ll need only 4 bytes to include key) that turn into 0 (i.e. they’ll need only 1 byte to describe it). However,
each of these writes in the uncompressed form will be represented as 272 byte state diff entry and so we get the number
of diffs is `120k / 5 = 24k`. This means that they will have accommodate `24k * 272 = 6528000` bytes of calldata for the
uncompressed state diffs. Adding 120k on top leaves us with roughly `6650000` bytes needed for calldata. `207813` slots
are needed to accommodate this amount of data. We round up to `208000` slots to give space for constant-size factors for
ABI-encoding, like offsets, lengths, etc.

In theory though much more calldata could be used (if for instance 1 byte is used for enum index). It is the
responsibility of the operator to ensure that it can form the correct calldata for the L1Messenger.

### **Transaction’s meta descriptions**

- `[248053..250100]` words — 2048 slots for 1024 transaction’s meta descriptions (their structure is explained below).

For internal reasons related to possible future integrations of zero-knowledge proofs about some of the contents of the
bootloader’s memory, the array of the transactions is not passed as the ABI-encoding of the array of transactions, but:

- We have a constant maximum number of transactions. At the time of this writing, this number is 1024.
- Then, we have 1024 transaction descriptions, each ABI encoded as the following struct:

```solidity
struct BootloaderTxDescription {
  // The offset by which the ABI-encoded transaction's data is stored
  uint256 txDataOffset;
  // Auxiliary data on the transaction's execution. In our internal versions
  // of the bootloader it may have some special meaning, but for the
  // bootloader used on the mainnet it has only one meaning: whether to execute
  // the transaction. If 0, no more transactions should be executed. If 1, then
  // we should execute this transaction and possibly try to execute the next one.
  uint256 txExecutionMeta;
}

```

### **Reserved slots for the calldata for the paymaster’s postOp operation**

- `[252149..252188]` words — 40 slots which could be used for encoding the calls for postOp methods of the paymaster.

To avoid additional copying of transactions for calls for the account abstraction, we reserve some of the slots which
could be then used to form the calldata for the `postOp` call for the account abstraction without having to copy the
entire transaction’s data.

### **The actual transaction’s descriptions**

- `[252189..523261]`

Starting from the 487312 word, the actual descriptions of the transactions start. (The struct can be found by this
[link](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/contracts/libraries/TransactionHelper.sol#L25)).
The bootloader enforces that:

- They are correctly ABI encoded representations of the struct above.
- They are located without any gaps in memory (the first transaction starts at word 653 and each transaction goes right
  after the next one).
- The contents of the currently processed transaction (and the ones that will be processed later on are untouched).
  Note, that we do allow overriding data from the already processed transactions as it helps to preserve efficiency by
  not having to copy the contents of the `Transaction` each time we need to encode a call to the account.

### **VM hook pointers**

- `[523261..523263]`

These are memory slots that are used purely for debugging purposes (when the VM writes to these slots, the server side
can catch these calls and give important insight information for debugging issues).

### **Result ptr pointer**

- \[523264..524287\]

These are memory slots that are used to track the success status of a transaction. If the transaction with number `i`
succeeded, the slot `2^19 - 1024 + i` will be marked as 1 and 0 otherwise.

## General flow of the bootloader’s execution

1. At the start of the batch it
   [reads the initial batch information](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L3629)
   and
   [sends the information](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L3674)
   about the current batch to the SystemContext system contract.
2. It goes through each of
   [transaction’s descriptions](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L3715)
   and checks whether the `execute` field is set. If not, it ends processing of the transactions and ends execution of
   the batch. If the execute field is non-zero, the transaction will be executed and it goes to step 3.
3. Based on the transaction’s type it decides whether the transaction is an L1 or L2 transaction and processes them
   accordingly. More on the processing of the L1 transactions can be read [here](#l1-l2-transactions). More on L2
   transactions can be read [here](#l2-transactions).

## L2 transactions

On ZKsync, every address is a contract. Users can start transactions from their EOA accounts, because every address that
does not have any contract deployed on it implicitly contains the code defined in the
[DefaultAccount.sol](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/DefaultAccount.sol)
file. Whenever anyone calls a contract that is not in kernel space (i.e. the address is ≥ 2^16) and does not have any
contract code deployed on it, the code for `DefaultAccount` will be used as the contract’s code.

Note, that if you call an account that is in kernel space and does not have any code deployed there, right now, the
transaction will revert.

We process the L2 transactions according to our account abstraction protocol:
[https://v2-docs.zksync.io/dev/tutorials/custom-aa-tutorial.html#prerequisite](https://v2-docs.zksync.io/dev/tutorials/custom-aa-tutorial.html#prerequisite).

1. We
   [deduct](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L1073)
   the transaction’s upfront payment for the overhead for the block’s processing. You can read more on how that works in
   the fee model
   [description](./zksync_fee_model.md).
2. Then we calculate the gasPrice for these transactions according to the EIP1559 rules.
3. We
   [conduct the validation step](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L1180)
   of the AA protocol:

- We calculate the hash of the transaction.
- If enough gas has been provided, we near_call the validation function in the bootloader. It sets the tx.origin to the
  address of the bootloader, sets the ergsPrice. It also marks the factory dependencies provided by the transaction as
  marked and then invokes the validation method of the account and verifies the returned magic.
- Calls the accounts and, if needed, the paymaster to receive the payment for the transaction. Note, that accounts may
  not use `block.baseFee` context variable, so they have no way to know what exact sum to pay. That’s why the accounts
  typically firstly send `tx.maxFeePerErg * tx.ergsLimit` and the bootloader
  [refunds](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L730)
  for any excess funds sent.

1. [We perform the execution of the transaction](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L1234).
   Note, that if the sender is an EOA, tx.origin is set equal to the `from` the value of the transaction. During the
   execution of the transaction, the publishing of the compressed bytecodes happens: for each factory dependency if it
   has not been published yet and its hash is currently pointed to in the compressed bytecodes area of the bootloader, a
   call to the bytecode compressor is done. Also, at the end the call to the KnownCodeStorage is done to ensure all the
   bytecodes have indeed been published.
2. We
   [refund](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L1401)
   the user for any excess funds he spent on the transaction:

- Firstly, the postTransaction operation is called to the paymaster.
- The bootloader asks the operator to provide a refund. During the first VM run without proofs the provide directly
  inserts the refunds in the memory of the bootloader. During the run for the proved batches, the operator already knows
  what which values have to be inserted there. You can read more about it in the
  [documentation](./zksync_fee_model.md)
  of the fee model.
- The bootloader refunds the user.

1. We notify the operator about the
   [refund](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L1112)
   that was granted to the user. It will be used for the correct displaying of gasUsed for the transaction in explorer.

## L1->L2 transactions

L1->L2 transactions are transactions that were initiated on L1. We assume that `from` has already authorized the L1→L2
transactions. It also has its L1 pubdata price as well as ergsPrice set on L1.

Most of the steps from the execution of L2 transactions are omitted and we set `tx.origin` to the `from`, and
`ergsPrice` to the one provided by transaction. After that, we use
[mimicCall](../../../guides/advanced/12_alternative_vm_intro.md#zkevm-specific-opcodes)
to provide the operation itself from the name of the sender account.

Note, that for L1→L2 transactions, `reserved0` field denotes the amount of ETH that should be minted on L2 as a result
of this transaction. `reserved1` is the refund receiver address, i.e. the address that would receive the refund for the
transaction as well as the msg.value if the transaction fails.

There are two kinds of L1->L2 transactions:

- Priority operations, initiated by users (they have type `255`).
- Upgrade transactions, that can be initiated during system upgrade (they have type `254`).

You can read more about differences between those in the corresponding
[document](../settlement_contracts/priority_queue/l1_l2_communication/l1_to_l2.md).

## End of the batch

At the end of the batch we set `tx.origin` and `tx.gasprice` context variables to zero to save L1 gas on calldata and
send the entire bootloader balance to the operator, effectively sending fees to him.

Also, we
[set](https://github.com/code-423n4/2023-10-zksync/blob/ef99273a8fdb19f5912ca38ba46d6bd02071363d/code/system-contracts/bootloader/bootloader.yul#L3812)
the fictive L2 block’s data. Then, we call the system context to ensure that it publishes the timestamp of the L2 block
as well as L1 batch. We also reset the `txNumberInBlock` counter to avoid its state diffs from being published on L1.
You can read more about block processing on ZKsync
[here](./batches_and_blocks_on_zksync.md).

After that, we publish the hash as well as the number of priority operations in this batch. More on it
[here](../settlement_contracts/priority_queue/l1_l2_communication/l1_to_l2.md).

Then, we call the L1Messenger system contract for it to compose the pubdata to be published on L1. You can read more
about the pubdata processing
[here](../settlement_contracts/data_availability/pubdata.md).
