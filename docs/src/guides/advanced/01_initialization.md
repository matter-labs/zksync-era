# ZKsync Deeper Dive

The goal of this doc is to show you some more details on how ZKsync works internally.

Please do the dev_setup.md and development.md (these commands do all the heavy lifting on starting the components of the
system).

Now let's take a look at what's inside:

### Initialization

Let's take a deeper look into what `zkstack ecosystem init` does.

#### ZK Stack CLI

`zkstack` itself is implemented in Rust (you can see the code in `/zkstack_cli` directory). If you change anything
there, make sure to run `zkstackup --local` from the root folder (that compiles and installs this code), before
re-running any `zkstack` command.

#### Containers

The first step to initialize a ZK Stack ecosystem is to run the command `zkstack containers`. This command gets the
docker images for `postgres` and `reth`. If the `--observability` option is passed to the command, or the corresponding
option is selected in the interactive prompt, then Prometheus, Grafana and other observability-related images are
downloaded and run.

Reth (one of the Ethereum clients) will be used to setup our own copy of L1 chain (that our local ZKsync would use).

Postgres is one of the two databases, that is used by ZKsync (the other one is RocksDB). Currently most of the data is
stored in postgres (blocks, transactions etc) - while RocksDB is only storing the state (Tree & Map) - and it used by
VM.

#### Ecosystem

The next step is to run the command `zkstack ecosystem init`.

This command:

- Collects and finalize the ecosystem configuration.
- Builds and deploys L1 & L2 contracts.
- Initializes each chain defined in the `/chains` folder. (Currently, a single chain `era` is defined there, but you can
  create your own chains running `zkstack chain create`).
- Sets up observability.
- Runs the genesis process.
- Initializes the database.

#### Postgres

First - postgres database: you'll be able to see something like

```
DATABASE_URL = postgres://postgres:notsecurepassword@localhost/zksync_local
```

After which we setup the schema (lots of lines with `Applied XX`).

You can try connecting to postgres now, to see what's inside:

```shell
psql postgres://postgres:notsecurepassword@localhost/zksync_local
```

(and then commands like `\dt` to see the tables, `\d TABLE_NAME` to see the schema, and `select * from XX` to see the
contents).

As our network has just started, the database would be quite empty.

You can see the schema for the database in
[dal/README.md](https://github.com/matter-labs/zksync-era/blob/main/core/lib/dal/README.md) TODO: add the link to the
document with DB schema.

#### Docker

We're running two things in a docker:

- a postgres (that we've covered above)
- a reth (that is the L1 Ethereum chain).

Let's see if they are running:

```shell
docker container ls
```

and then we can look at the Reth logs:

```shell
docker logs zksync-era-reth-1
```

Where `zksync-era-reth-1` is the container name, that we got from the first command.

If everything goes well, you should see that L1 blocks are being produced.

#### Server

Now we can start the main server:

```bash
zkstack server
```

This will actually run a cargo binary (`zksync_server`).

The server will wait for the new transactions to generate the blocks (these can either be sent via JSON RPC, but it also
listens on the logs from the L1 contract - as things like token bridging etc comes from there).

Currently we don't send any transactions there (so the logs might be empty).

But you should see some initial blocks in postgres:

```sql
select * from miniblocks;
```

#### Our L1 (reth)

Let's finish this article, by taking a look at our L1:

We will use the `web3` tool to communicate with the L1, have a look at [02_deposits.md](02_deposits.md) for installation
instructions. You can check that you're a (localnet) crypto trillionaire, by running:

```bash
./web3 --rpc-url http://localhost:8545 balance 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049
```

This is one of the "rich wallets" we predefined for local L1.

**Note:** This reth shell is running official Ethereum JSON RPC with Reth-specific extensions documented at
[reth docs](https://paradigmxyz.github.io/reth/jsonrpc/intro.html)

In order to communicate with L2 (our ZKsync) - we have to deploy multiple contracts onto L1 (our local reth created
Ethereum). You can look on the `deployL1.log` file - to see the list of contracts that were deployed and their accounts.

First thing in the file, is the deployer/governor wallet - this is the account that can change, freeze and unfreeze the
contracts (basically the owner). You can verify the token balance using the `getBalance` method above.

Then, there are a bunch of contracts (CRATE2_FACTOR, DIAMOND_PROXY, L1_ALLOW_LIST etc etc) - for each one, the file
contains the address.

You can quickly verify that they were really deployed, by calling:

```bash
./web3 --rpc-url http://localhost:8545 address XXX
```

Where XXX is the address in the file.

The most important one of them is CONTRACTS_DIAMOND_PROXY_ADDR (which acts as 'loadbalancer/router' for others - and
this is the contract that our server is 'listening' on).

## Summary

Ok - so let's sum up what we have:

- a postgres running in docker (main database)
- a local instance of ethereum (reth running in docker)
  - which also has a bunch of 'magic' contracts deployed
  - and two accounts with lots of tokens
- and a server process

In the [next article](02_deposits.md), we'll start playing with the system (bridging tokens etc).
