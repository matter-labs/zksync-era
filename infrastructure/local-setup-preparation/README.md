# Scripts for local setup preparation

This project contains scripts that should be executed when preparing the ZKsync local setup used by outside developers,
e.g. deposit ETH to some of the test accounts.

With the server running (`zk server`), execute the following from `$ZKSYNC_HOME` to fund the L2 wallets

```
zk f bash -c 'cd infrastructure/local-setup-preparation ; yarn start'
```
