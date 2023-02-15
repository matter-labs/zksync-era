# Loadnext: the next generation loadtest for zkSync

Loadnext is an utility for random stress-testing the zkSync server. It is capable of simulating the behavior of many
independent users of zkSync network, who are sending quasi-random requests to the server.

It:

- doesn't care whether the server is alive or not. At worst, it will just consider the test failed. No panics, no
  mindless unwraps, yay.
- does a unique set of operations for each participating account.
- sends transactions and priority operations.
- sends incorrect transactions as well as correct ones and compares the outcome to the expected one.
- has an easy-to-extend command system that allows adding new types of actions to the flow.
- has an easy-to-extend report analysis system.

Flaws:

- It does not send API requests other than required to execute transactions.
- So far it has pretty primitive report system.

## Launch

In order to launch the test in the development scenario, you must first run server and prover (it is recommended to use
dummy prover), and then launch the test itself.

```sh
# First terminal
zk server
# Second terminal
RUST_BACKTRACE=1 RUST_LOG=info,jsonrpsee_ws_client=error cargo run --bin loadnext
```

Without any configuration supplied, the test will fallback to the dev defaults:

- Use one of the "rich" accounts in the private local Ethereum chain.
- Use a random ERC-20 token from `etc/tokens/localhost.json`.
- Connect to the localhost zkSync node and use localhost web3 API.

**Note:** when running the loadtest in the localhost scenario, you **must** adjust the supported block chunks sizes.
Edit the `etc/env/dev/chain.toml` and set `block_chunk_sizes` to `[10,32,72,156,322,654]` and `aggregated_proof_sizes`
to `[1,4,8,18]`. Do not forget to re-compile configs after that.

This is required because the loadtest relies on batches, which will not fit into smaller block sizes.

## Configuration

For cases when loadtest is launched outside of the localhost environment, configuration is provided via environment
variables.

The following variables are required:

```sh
# Address of the Ethereum web3 API.
L1_RPC_ADDRESS
# Ethereum private key of the wallet that has funds to perform a test (without `0x` prefix).
MASTER_WALLET_PK
# Amount of accounts to be used in test.
# This option configures the "width" of the test:
# how many concurrent operation flows will be executed.
ACCOUNTS_AMOUNT
# All of test accounts get split into groups that share the
# deployed contract address. This helps to emulate the behavior of
# sending `Execute` to the same contract and reading its events by
# single a group. This value should be less than or equal to `ACCOUNTS_AMOUNT`.
ACCOUNTS_GROUP_SIZE
# Amount of operations per account.
# This option configures the "length" of the test:
# how many individual operations each account of the test will execute.
OPERATIONS_PER_ACCOUNT
# Address of the ERC-20 token to be used in test.
#
# Token must satisfy two criteria:
# - Be supported by zkSync.
# - Have `mint` operation.
#
# Note that we use ERC-20 token since we can't easily mint a lot of ETH on
# Rinkeby or Ropsten without caring about collecting it back.
MAIN_TOKEN
# Path to test contracts bytecode and ABI required for sending
# deploy and execute L2 transactions. Each folder in the path is expected
# to have the following structure:
# .
# ├── bytecode
# └── abi.json
# Contract folder names names are not restricted.
# An example:
# .
# ├── erc-20
# │   ├── bytecode
# │   └── abi.json
# └── simple-contract
#     ├── bytecode
#     └── abi.json
TEST_CONTRACTS_PATH
# Limits the number of simultaneous API requests being performed at any moment of time.
#
# Setting it to:
# - 0 turns off API requests.
# - `ACCOUNTS_AMOUNT` relieves the limit.
SYNC_API_REQUESTS_LIMIT
# zkSync Chain ID.
L2_CHAIN_ID
# Address of the zkSync web3 API.
L2_RPC_ADDRESS
```

Optional parameters:

```sh
# Optional seed to be used in the test: normally you don't need to set the seed,
# but you can re-use seed from previous run to reproduce the sequence of operations locally.
# Seed must be represented as a hexadecimal string.
SEED
```

## Infrastructure relationship

This crate is meant to be independent of the existing zkSync infrastructure. It is not integrated in `zk` and does not
rely on `zksync_config` crate, and is not meant to be.

The reason is that this application is meant to be used in CI, where any kind of configuration pre-requisites makes the
tool harder to use:

- Additional tools (e.g. `zk`) must be shipped together with the application, or included into the docker container.
- Configuration that lies in files is harder to use in CI, due some sensitive data being stored in GITHUB_SECRETS.
