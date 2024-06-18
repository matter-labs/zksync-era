# Eth-sender `zksync_eth_sender`

Eth-sender is the component that executes scheduled transactions on Ethereum. As of 06.2024 it is only used by core to
execute Commit, Prove and Execute L1 transactions.

It was designed with fire-and-forget ideology in mind, as soon as you send transaction to it to be executed, it will
make sure it is mined, handling stuff as bumping gas prices and resending transaction if needed.

## API

eth-sender API exposes only two methods:

```
send_tx(idempotency_key, raw_tx, tx_type, contract_address, blob_sidecar) -> None
```

```
get_tx(idempotency_key) -> Optional<(tx_hash, status)>
```

where `idempotency_key` can be just any String and status be one of `Pending`, `Confirmed`, `Failed`

Attempting to send tx for the same `idempotency_key`, but with different parameters returns in an error.

Nonces are determined via order of calling send_tx.

## Integration with server-v2

Whenever we want to send an L1 transaction, we create an entry in `l1_transactions` table with status `Created`. We have
eth_sender_sync that periodically picks up all entries from l1_transactions. It finds all that have status `Created` and
sends them using `send_tx`. It also tries to fetch status of transactions with status `Pending` using `get_tx`.

There isn't a way to 'cancel' transactions already sent to eth-sender. If there is a need to remove some transactions,
we remove them only from `l1_transactions` table and create new 'replacement' ones using different idempotency_key.
