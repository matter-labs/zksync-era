# In memory node, with fork support

This crate provides an in-memory node that supports forking the state from other networks.

The goal of this crate is to offer a fast solution for integration testing, bootloader and system contract testing, and
prototyping.

Please note that this crate is still in the alpha stage, and not all functionality is fully supported. For final
testing, it is highly recommended to use the 'local-node' or a testnet.

Current limitations:

- No communication between Layer 1 and Layer 2 (the local node operates only on Layer 2).
- Many APIs are not yet implemented, but the basic set of APIs is supported.
- No support for accessing historical data, such as the storage state at a specific block.
- Only one transaction is allowed per Layer 1 batch.
- Fixed values are returned for zk Gas estimation.

Current features:

- Can fork the state of the mainnet, testnet, or a custom network at any given height.
- Uses local bootloader and system contracts, making it suitable for testing new changes.
- When running in non-fork mode, it operates deterministically (only one transaction per block, etc.), which simplifies
  testing.
- Starts up quickly and comes pre-configured with a few 'rich' accounts.

## How to

To start a node:

```shell
cargo run --release -p zksync_test_node run
```

This will run a node (with an empty state) and make it available on port 8011

To fork mainnet:

```shell
cargo run --release -p zksync_test_node fork mainnet
```

This will run the node, forked at current head of mainnet

You can also specify the custom http endpoint and custom forking height:

```shell
cargo run --release -p zksync_test_node fork --fork-at 7000000 http://172.17.0.3:3060
```

## Forking network & sending calls

You can use your favorite development tool (or tools like `curl`) or zksync-foundry:

Check testnet LINK balance

```shell
$ cargo run --release -p zksync_test_node fork testnet

$ zkcast call 0x40609141Db628BeEE3BfAB8034Fc2D8278D0Cc78 "name()(string)" --rpc-url http://localhost:8011

> ChainLink Token (goerli)


$ $ zkcast call 0x40609141Db628BeEE3BfAB8034Fc2D8278D0Cc78 "balanceOf(address)(uint256)"  0x40609141Db628BeEE3BfAB8034Fc2D8278D0Cc78  --rpc-url http://localhost:8011
> 28762283719732275444443116625665
```

Or Mainnet USDT:

```shell
cargo run -p zksync_test_node fork mainnet

zkcast call 0x493257fD37EDB34451f62EDf8D2a0C418852bA4C "name()(string)" --rpc-url http://localhost:8011

> Tether USD
```

And you can also build & deploy your own contracts:

```shell
fzkforge zkc src/Greeter.sol:Greeter --constructor-args "ZkSync and Foundry" --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --rpc-url http://localhost:8011 --chain 270

```

## Testing bootloader & system contracts

In-memory node is taking the currently compiled bootloader & system contracts - therefore easily allowing to test
changes (and together with fork, allows to see the effects of the changes on the already deployed contracts).

You can see the bootloader logs, by setting the proper log level. In the example below, we recompile the bootloader, and
run it with mainnet fork.

```shell

cd etc/system-contracts
yarn preprocess && yarn hardhat run ./scripts/compile-yul.ts
cd -
RUST_LOG=vm=trace cargo run -p zksync_test_node fork --fork-at 70000000 testnet
```

## Replaying other network transaction locally

Imagine, that you have a testnet transaction, that you'd like to replay locally (for example to see more debug
information).

```shell
cargo run --release -p zksync_test_node replay_tx testnet 0x7f039bcbb1490b855be37e74cf2400503ad57f51c84856362f99b0cbf1ef478a
```

### How does it work

It utilizes an in-memory database to store the state information and employs simplified hashmaps to track blocks and
transactions.

In fork mode, it attempts to retrieve missing storage data from a remote source when it's not available locally.
