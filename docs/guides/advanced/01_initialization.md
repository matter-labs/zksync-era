# ZKsync deeper dive

The goal of this doc is to show you some more details on how ZKsync works internally.

Please do the dev_setup.md and development.md (these commands do all the heavy lifting on starting the components of the
system).

Now let's take a look at what's inside:

### Initialization (zk init)

Let's take a deeper look into what `zk init` does.

#### zk tool

`zk` itself is implemented in typescript (you can see the code in `infrastructure` directory). If you change anything
there, make sure to run `zk` (that compiles this code), before re-running `zk init`.

#### zk init

As first step, it gets the docker images for postgres and reth.

Reth (one of the Ethereum clients) will be used to setup our own copy of L1 chain (that our local ZKsync would use).

Postgres is one of the two databases, that is used by ZKsync (the other one is RocksDB). Currently most of the data is
stored in postgres (blocks, transactions etc) - while RocksDB is only storing the state (Tree & Map) - and it used by
VM.

Then we compile JS packages (these include our web3 sdk, tools and testing infrastructure).

Then L1 & L2 contracts.

And now we're ready to start setting up the system.

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

You can see the schema for the database in [dal/README.md](../../../core/lib/dal/README.md) TODO: add the link to the
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

```shell
zk server
```

This will actually run a cargo binary (`zksync_server`).

The server will wait for the new transactions to generate the blocks (these can either be sent via JSON RPC, but it also
listens on the logs from the L1 contract - as things like token bridging etc comes from there).

Currently we don't send any transactions there (so the logs might be empty).

But you should see some initial blocks in postgres:

```
select * from miniblocks;
```

#### Our L1 (reth)

Let's finish this article, by taking a look at our L1:

We will use the `web3` tool to communicate with the L1, have a look at [02_deposits.md](02_deposits.md) for installation
instructions. You can check that you're a (localnet) crypto trillionaire, by running:

```shell
./web3 --rpc-url http://localhost:8545 balance 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049
```

This is one of the "rich wallets" we predefined for local L1.

**Note:** This reth shell is running official Ethereum JSON RPC with Reth-specific extensions documented at
[reth docs](https://paradigmxyz.github.io/reth/jsonrpc/intro.html)

In order to communicate with L2 (our ZKsync) - we have to deploy multiple contracts onto L1 (our local reth created
Ethereum). You can look on the `deployL1.log` file - to see the list of contracts that were deployed and their accounts.

First thing in the file, is the deployer/governor wallet - this is the account that can change, freeze and unfreeze the
contracts (basically the owner). You can also verify (using the getBalance method above), that is has a lot of tokens.

Then, there are a bunch of contracts (CRATE2_FACTOR, DIAMOND_PROXY, L1_ALLOW_LIST etc etc) - for each one, the file
contains the address.

You can quickly verify that they were really deployed, by calling:

```shell
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
