# Life of transaction

In this article, we will explore the lifecycle of a transaction, which is an operation that is stored permanently in the
blockchain and results in a change of its overall state.

To better understand the content discussed here, it is recommended that you first read the
[life of a call](./05_how_call_works.md).

## L1 vs L2 transactions

There are two main methods through which transactions can enter the system. The most common approach involves making a
call to the RPC (Remote Procedure Call), where you send what is known as an [`L2Tx`][l2_tx] transaction.

The second method involves interacting with Ethereum directly by sending a 'wrapped' transaction to our Ethereum
contract. These transactions are referred to as [`L1Tx`][l1_tx] or Priority transactions, and the process of sending
transactions in this manner is called the 'priority queue'.

### Transaction types

We provide support for five different types of transactions.

Here's a simplified table of the transaction types:

| Type id | Transaction type                                           | Features                                                                                           | Use cases                                                                             | % of transactions (mainnet/testnet) |
| ------- | ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- | ----------------------------------- |
| 0x0     | 'Legacy'                                                   | Only includes `gas price`                                                                          | These are traditional Ethereum transactions.                                          | 60% / 82%                           |
| 0x1     | EIP-2930                                                   | Contains a list of storage keys/addresses the transaction will access                              | At present, this type of transaction is not enabled.                                  |
| 0x2     | EIP-1559                                                   | Includes `max_priority_fee_per_gas`, `max_gas_price`                                               | These are Ethereum transactions that provide more control over the gas fee.           | 35% / 12%                           |
| 0x71    | EIP-712 (specific to ZKsync)                               | Similar to EIP-1559, but also adds `max_gas_per_pubdata`, custom signatures, and Paymaster support | This is used by those who are using ZKsync specific Software Development Kits (SDKs). | 1% / 2%                             |
| 0xFF    | L1 transactions also known as priority transactions `L1Tx` | Originating from L1, these have more custom fields like 'refund' addresses etc                     | Mainly used to transfer funds/data between L1 & L2 layer.                             | 4% / 3%                             |

Here's the code that does the parsing: [TransactionRequest::from_bytes][transaction_request_from_bytes]

## Transactions lifecycle

### Priority queue (L1 Tx only)

L1 transactions are first 'packaged' and then sent to our Ethereum contract. After this, the L1 contract records this
transaction in L1 logs. [The `eth_watcher` component][eth_watcher] constantly monitors these logs and then adds them to
the database (mempool).

### RPC & validation (L2 Tx only)

Transactions are received via the `eth_sendRawTransaction` method. These are then parsed and validated using the
[`submit_tx`][submit_tx] method on the API server.

The validations ensure that the correct amount of gas has been assigned by the user and that the user's account has
sufficient gas, among other things.

As part of this validation, we also perform a `validation_check` to ensure that if account abstraction / paymaster is
used, they are prepared to cover the fees. Additionally, we perform a 'dry_run' of the transaction for a better
developer experience, providing almost immediate feedback if the transaction fails.

Please note, that transaction can still fail in the later phases, even if it succeeded in the API, as it is going to be
executed in the context of a different block.

Once validated, the transaction is added to the mempool for later execution. Currently, the mempool is stored in the
`transactions` table in postgres (see the `insert_transaction_l2()` method).

### Batch executor & State keeper

The State Keeper's job is to take transactions from the mempool and place them into an L1 batch. This is done using the
[`process_l1_batch()`][process_l1_batch] method.

This method takes the next transaction from the mempool (which could be either an L1Tx or L2Tx - but L1Tx are always
given the priority and they are taken first), executes it, and checks if the L1 batch is ready to be sealed (for more
details on when we finalize L1 batches, see the 'Blocks & Batches' article).

Once the batch is sealed, it's ready to be sent for proof generation and have this proof committed into L1. More details
on this will be covered in a separate article.

The transaction can have three different results in state keeper:

- Success
- Failure (but still included in the block, and gas was charged)
- Rejection - when it fails validation, and cannot be included in the block. This last case should (in theory) never
  happen - as we cannot charge the fee in such scenario, and it opens the possibility for the DDoS attack.

[transaction_request_from_bytes]:
  https://github.com/matter-labs/zksync-era/blob/main/core/lib/types/src/transaction_request.rs#L196
  'transaction request from bytes'
[eth_watcher]: https://github.com/matter-labs/zksync-era/blob/main/core/node/eth_watch 'Ethereum watcher component'
[l1_tx]: https://github.com/matter-labs/zksync-era/blob/main/core/lib/types/src/l1/mod.rs#L183 'l1 tx'
[l2_tx]: https://github.com/matter-labs/zksync-era/blob/main/core/lib/types/src/l2/mod.rs#L140 'l2 tx'
[submit_tx]:
  https://github.com/matter-labs/zksync-era/blob/main/core/lib/zksync_core/src/api_server/tx_sender/mod.rs#L288
  'submit tx'
[process_l1_batch]:
  https://github.com/matter-labs/zksync-era/blob/main/core/lib/zksync_core/src/state_keeper/keeper.rs#L257
  'process l1 batch'
