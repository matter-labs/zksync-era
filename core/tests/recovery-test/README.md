# Recovery Integration Tests

These integration tests verify that a full node can initialize from an application snapshot or from genesis and then
sync with the main node.

## Running locally

The tests require that the main node is running; you can start it with a command like

```shell
zk server &>server.log &
```

- [**Snapshot recovery test**](tests/snapshot-recovery.test.ts) can be run using
  `yarn recovery-test snapshot-recovery-test`. It outputs logs to the files in the test directory:
  `snapshot-creator.log` (snapshot creator logs) and `snapshot-recovery.log` (full node logs).
- [**Genesis recovery test**](tests/genesis-recovery.test.ts) can be run using
  `yarn recovery-test genesis-recovery-test`. It outputs full node logs to `genesis-recovery.log` in the test directory.
