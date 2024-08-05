# Loadnext: loadtest for ZKsync

Loadnext is a utility for random stress-testing the ZKsync server. It is capable of simulating the behavior of many
independent users of ZKsync network, who are sending quasi-random requests to the server.

The general flow is as follows:

- The master account performs an initial deposit to L2
- Paymaster on L2 is funded if necessary
- The L2 master account distributes funds to the participating accounts (`accounts_amount` configuration option)
- Each account continuously sends L2 transactions as configured in `contract_execution_params` configuration option. At
  any given time there are no more than `max_inflight_txs` transactions in flight for each account.
- Once each account is done with the initial deposit, the test is run for `duration_sec` seconds.
- After the test is finished, the master account withdraws all the remaining funds from L2.
- The average TPS is reported.

## Features

It:

- doesn't care whether the server is alive or not. In the worst-case scenario, it will simply mark the test as failed.
- does a unique set of operations for each participating account.
- sends transactions and priority operations.
- sends incorrect transactions as well as correct ones and compares the outcome to the expected one.
- has an easy-to-extend command system that allows adding new types of actions to the flow.
- has an easy-to-extend report analysis system.

## Transactions Parameters

The smart contract that is used for every l2 transaction can be found here:
`etc/contracts-test-data/contracts/loadnext/loadnext_contract.sol`.

The `execute` function of the contract has the following parameters:

```
    function execute(uint reads, uint writes, uint hashes, uint events, uint max_recursion, uint deploys) external returns(uint) {
```

which correspond to the following configuration options:

```
pub struct LoadnextContractExecutionParams {
    pub reads: usize,
    pub writes: usize,
    pub events: usize,
    pub hashes: usize,
    pub recursive_calls: usize,
    pub deploys: usize,
}
```

For example, to simulate an average transaction on mainnet, one could do:

```
CONTRACT_EXECUTION_PARAMS_WRITES=2
CONTRACT_EXECUTION_PARAMS_READS=6
CONTRACT_EXECUTION_PARAMS_EVENTS=2
CONTRACT_EXECUTION_PARAMS_HASHES=10
CONTRACT_EXECUTION_PARAMS_RECURSIVE_CALLS=0
CONTRACT_EXECUTION_PARAMS_DEPLOYS=0
```

Similarly, to simulate a lightweight transaction:

```
CONTRACT_EXECUTION_PARAMS_WRITES=0
CONTRACT_EXECUTION_PARAMS_READS=0
CONTRACT_EXECUTION_PARAMS_EVENTS=0
CONTRACT_EXECUTION_PARAMS_HASHES=0
CONTRACT_EXECUTION_PARAMS_RECURSIVE_CALLS=0
CONTRACT_EXECUTION_PARAMS_DEPLOYS=0
```

## Configuration

For the full list of configuration options, see `loadnext/src/config.rs`.

Example invocation:

- transactions similar to mainnet
- 300 accounts - should be enough to put full load to the sequencer
- 20 transactions in flight - corresponds to the current limits on the mainnet and testnet
- 20 minutes of testing - should be enough to properly estimate the TPS
- As `L2_RPC_ADDRESS`, `L2_WS_RPC_ADDRESS`, `L1_RPC_ADDRESS` and `L1_RPC_ADDRESS` is not set, the test will run against
  the local environment.
- `MASTER_WALLET_PK` needs to be set to the private key of the master account.
- `MAIN_TOKEN` needs to be set to the address of the token to be used for the loadtest.

```
cargo build

CONTRACT_EXECUTION_PARAMS_WRITES=2 \
CONTRACT_EXECUTION_PARAMS_READS=6 \
CONTRACT_EXECUTION_PARAMS_EVENTS=2 \
CONTRACT_EXECUTION_PARAMS_HASHES=10 \
CONTRACT_EXECUTION_PARAMS_RECURSIVE_CALLS=0 \
CONTRACT_EXECUTION_PARAMS_DEPLOYS=0 \
ACCOUNTS_AMOUNT=300 \
ACCOUNTS_GROUP_SIZE=300 \
MAX_INFLIGHT_TXS=20 \
RUST_LOG="info,loadnext=debug" \
SYNC_API_REQUESTS_LIMIT=0 \
SYNC_PUBSUB_SUBSCRIPTIONS_LIMIT=0 \
TRANSACTION_WEIGHTS_DEPOSIT=0 \
TRANSACTION_WEIGHTS_WITHDRAWAL=0 \
TRANSACTION_WEIGHTS_L1_TRANSACTIONS=0 \
TRANSACTION_WEIGHTS_L2_TRANSACTIONS=1 \
DURATION_SEC=1200 \
MASTER_WALLET_PK="..." \
MAIN_TOKEN="..." \
cargo run --bin loadnext
```
