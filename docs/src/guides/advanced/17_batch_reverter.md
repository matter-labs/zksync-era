# L1 Batch reversion

### Motivation

In extremely rare circumstances due to an operator mistake or a bug zksync-era you may revert unexecuted L1 batches. This
could be useful if an unprovable batch would be committed or a commit transaction failed due to mismatch between server
and blockchain state.

### Overview

The purpose of block reverter is to revert all stateful components state and uncommit the L1 batch/-es.

Block reverter performs following operations:

- `rollback-db` command:
  - revert state keeper cache
  - revert vm runners’ caches
  - revert all the trees
  - revert Postgres state
- `send-eth-transaction` command: revert L1 commit
- `clear-failed-transactions` command: removing failed L1 transaction from DB

**Note: this doesn’t cover reverting the prover subsystem.**

### Procedure

- Verify that `stuck_tx_timeout` is high enough that the relevant transactions are not deleted
- Stop `state_keeper`, `tree`, all vm runners (`protective-reads-writer`, `bwip`, `vm-playground` ) and snapshot creator
- We’ll have to connect to the **machines** that have RocksDB local state ( `tree` and `state_keeper`). But their
  components cannot function. One way to achieve this is to run respective containers with `-components=http_api` (
  `-components=` won’t work - we have to set something) instead of the normal components (e.g. `state_keeper`)
- connect to the server/container that is running the tree
- Run `print-suggested-values` pay attention to log message - committed ≠ verified ≠ executed\*\*

```bash
root@server-0:/# RUST_LOG=info ./usr/bin/block_reverter print-suggested-values \
--operator-address CHANGE_ME \
--genesis-path=/config/genesis/genesis.yaml \
--wallets-path=/config/wallets/wallets.yaml \
--config-path=/config/general/general.yaml \
--contracts-config-path=/config/contracts/contracts.yaml \
--secrets-path=/config/server/secrets.yaml

{"timestamp":"2024-04-15T17:42:40.913018741Z","level":"INFO","fields":{"message":"Last L1 batch numbers on contract: committed 517339, verified 516945, executed 516945"},"target":"zksync_core::block_reverter","filename":"core/lib/zksync_core/src/block_reverter/mod.rs","line_number":446}
Suggested values for rollback: SuggestedRollbackValues {
 last_executed_l1_batch_number: L1BatchNumber(
  516945,
 ),
 nonce: 575111,
 priority_fee: 1000000000,
}
```

- Rollback blocks on contract: run `send-eth-transaction` (Only if revert on L1 is needed). Note: here and later
  `--l1-batch-number` is the number of latest batch **remaining** after revert.

```bash
root@server-0:/# RUST_LOG=info ETH_SENDER_SENDER_OPERATOR_PRIVATE_KEY=CHANGE_ME ./usr/bin/block_reverter send-eth-transaction \
--genesis-path=/config/genesis/genesis.yaml \
--wallets-path=/config/wallets/wallets.yaml \
--config-path=/config/general/general.yaml \
--contracts-config-path=/config/contracts/contracts.yaml \
--secrets-path=/config/server-v2-secrets/server-v2-secrets.yaml \
--l1-batch-number CHANGE_ME_LAST_TO_KEEP \
--nonce CHANGE_ME
Mar 27 11:50:36.347  INFO zksync_eth_client::ethereum_gateway: Operator address: 0x01239bb69d0c5f795de5c86fd00d25f9cd3b9deb
waiting for L1 transaction confirmation...
revert transaction has completed
```

- Run `print-suggested-values` again to make sure the contract has updated values: **pay attention to log** - now it
  should say that `committed` equals `verified` that equals `executed`:

```
{"timestamp":"2024-04-15T18:38:36.015134635Z","level":"INFO","fields":{"message":"Last L1 batch numbers on contract: committed 516945, verified 516945, executed 516945 516945"},"target":"zksync_core::block_reverter","filename":"core/lib/zksync_core/src/block_reverter/mod.rs","line_number":446}
```

- Rollback tree: run `rollback-db` with `--rollback-tree` flag

```bash
root@server-0:/# RUST_LOG=info ./usr/bin/block_reverter rollback-db \
--genesis-path=/config/genesis/genesis.yaml \
--wallets-path=/config/wallets/wallets.yaml \
--config-path=/config/general/general.yaml \
--contracts-config-path=/config/contracts/contracts.yaml \
--secrets-path=/config/server/secrets.yaml \
--l1-batch-number CHANGE_ME_LAST_TO_KEEP \
--rollback-tree

{"timestamp":"2022-08-30T10:32:11.312280583Z","level":"INFO","fields":{"message":"Operator address: 0x85b7b2fcfd6d3e5c063e7a5063359f4e6b3fec29"},"target":"zksync_eth_client::clients::http_client"}
getting logs that should be applied to rollback state...
{"timestamp":"2022-08-30T10:32:11.554904678Z","level":"INFO","fields":{"message":"fetching keys that were changed after given block number"},"target":"zksync_dal::storage_logs_dedup_dal"}
{"timestamp":"2022-08-30T10:32:11.608430505Z","level":"INFO","fields":{"message":"loaded 2844 keys"},"target":"zksync_dal::storage_logs_dedup_dal"}
{"timestamp":"2022-08-30T10:32:12.933738644Z","level":"INFO","fields":{"message":"processed 1000 values"},"target":"zksync_dal::storage_logs_dedup_dal"}
{"timestamp":"2022-08-30T10:32:14.243366764Z","level":"INFO","fields":{"message":"processed 2000 values"},"target":"zksync_dal::storage_logs_dedup_dal"}
rolling back merkle tree...
checking match of the tree root hash and root hash from Postgres...
saving tree changes to disk...
```

- connect to the server/container that is running the state keeper
- Rollback state keeper cache: run `rollback-db` with `--rollback-sk-cache`

```bash
root@server-0:/# RUST_LOG=info ./usr/bin/block_reverter rollback-db \
--genesis-path=/config/genesis/genesis.yaml \
--wallets-path=/config/wallets/wallets.yaml \
--config-path=/config/general/general.yaml \
--contracts-config-path=/config/contracts/contracts.yaml \
--secrets-path=/config/server/secrets.yaml \
--l1-batch-number CHANGE_ME_LAST_TO_KEEP \
--rollback-sk-cache

{"timestamp":"2022-08-30T10:37:17.99102197Z","level":"INFO","fields":{"message":"Operator address: 0x85b7b2fcfd6d3e5c063e7a5063359f4e6b3fec29"},"target":"zksync_eth_client::clients::http_client"}
getting logs that should be applied to rollback state...
{"timestamp":"2022-08-30T10:37:18.364586826Z","level":"INFO","fields":{"message":"fetching keys that were changed after given block number"},"target":"zksync_dal::storage_logs_dedup_dal"}
{"timestamp":"2022-08-30T10:37:18.386431879Z","level":"INFO","fields":{"message":"loaded 2844 keys"},"target":"zksync_dal::storage_logs_dedup_dal"}
{"timestamp":"2022-08-30T10:37:19.107738185Z","level":"INFO","fields":{"message":"processed 1000 values"},"target":"zksync_dal::storage_logs_dedup_dal"}
{"timestamp":"2022-08-30T10:37:19.823963293Z","level":"INFO","fields":{"message":"processed 2000 values"},"target":"zksync_dal::storage_logs_dedup_dal"}
opening DB with state keeper cache...
getting contracts and factory deps that should be removed...
rolling back state keeper cache...
```

- Rollback vm runners’ caches: connect to each vm-runner instance and run command

```bash
./usr/bin/block_reverter rollback-db \
--genesis-path=/config/genesis/genesis.yaml \
--wallets-path=/config/wallets/wallets.yaml \
--config-path=/config/general/general.yaml \
--contracts-config-path=/config/contracts/contracts.yaml \
--secrets-path=/config/server/secrets.yaml \
--l1-batch-number CHANGE_ME_LAST_TO_KEEP \
--rollback-vm-runners-cache
```

- Rollback postgres:

```bash
root@server-0:/# ./r/bin/block_reverter rollback-db \
--genesis-path=/config/genesis/genesis.yaml \
--wallets-path=/config/wallets/wallets.yaml \
--config-path=/config/general/general.yaml \
--contracts-config-path=/config/contracts/contracts.yaml \
--secrets-path=/config/server/secrets.yaml \
--l1-batch-number CHANGE_ME_LAST_TO_KEEP \
--rollback-postgres
```

- Clear failed l1 txs

  ```bash
  root@server-0:/# ./usr/bin/block_reverter clear-failed-transactions \
  --genesis-path=/config/genesis/genesis.yaml \
  --wallets-path=/config/wallets/wallets.yaml \
  --config-path=/config/general/general.yaml \
  --contracts-config-path=/config/contracts/contracts.yaml \
  --secrets-path=/config/server/secrets.yaml \
  --l1-batch-number CHANGE_ME_LAST_TO_KEEP \
  ```

- Resume deployments for the relevant components
- Restart API nodes to clear cache
- ⚠️ Check external nodes to see whether they have performed blocks rollback. If it they did not - do it manually

## ISSUES

- If you faced issues with bootloader e.g:
  `The bootloader failed to set previous block hash. Reason: The provided block number is not correct`

You have to remove State Keeper cache. Run server-core in a safe regime. (see the first steps of this document). Login
to the server and remove state-keeper files

```bash
root@server-0:/# rm -rf /db/state_keeper/
```
