# ZK-Sync deeper dive - bridging & deposits

In the [first article](01_initialization.md), we've managed to setup our system on local machine and verify that it
works. Now let's actually start using it.

## Seeing the status of the accounts

Let's use a small command line tool (web3 - <https://github.com/mm-zk/web3>) to interact with our blockchains.

```shell
git clone https://github.com/mm-zk/web3
make build
```

Then let's create the keypair for our temporary account:

```shell
./web3 account create
```

It will produce a public and private key (for example):

```
Private key: 0x5090c024edb3bdf4ce2ebc2da96bedee925d9d77d729687e5e2d56382cf0a5a6
Public address: 0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd
```

**NOTE:** Keep track of this key and address, as they will be constantly used throughout these articles

Now, let's see how many tokens we have:

```shell
// This checks the tokens on 'L1' (geth)
./web3 --rpc-url http://localhost:8545 balance  0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd

// This checks the tokens on 'L2' (zkSync)
./web3 --rpc-url http://localhost:3050 balance  0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd
```

Unsurprisingly we have 0 on both - let's fix it by first transferring some tokens on L1:

```shell
docker container exec -it zksync-era-geth-1  geth attach http://localhost:8545
// and inside:
eth.sendTransaction({from: personal.listAccounts[0], to: "0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd", value: "7400000000000000000"})
```

And now when we check the balance, we should see:

```shell
./web3 --rpc-url http://localhost:8545 balance  0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd
```

that we have 7.4 ETH.

and now let's bridge it over to L2.

## Bridging over to L2

For an easy way to bridge we'll use [zkSync CLI](https://github.com/matter-labs/zksync-cli)

```shell
npx zksync-cli bridge deposit --chain=dockerized-node --amount 3 --pk=0x5090c024edb3bdf4ce2ebc2da96bedee925d9d77d729687e5e2d56382cf0a5a6 --to=0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd
# Amount of ETH to deposit: 3
# Private key of the sender: 0x5090c024edb3bdf4ce2ebc2da96bedee925d9d77d729687e5e2d56382cf0a5a6
# Recipient address on L2: 0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd
```

If everything goes well, you should be able to see 3 tokens transferred:

```shell
./web3 --rpc-url http://localhost:3050 balance  0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd
```

### Diving deeper - what exactly happened

Let's take a deeper look at what the 'deposit' call actually did.

If we look at what 'deposit' command has printed, we'll see something like this:

```
Transaction submitted 💸💸💸
[...]/tx/0xe27dc466c36ad2046766e191017e7acf29e84356465feef76e821708ff18e179
```

Let's run the `geth attach` (exact command is above) and see the details:

```shell
eth.getTransaction("0xe27dc466c36ad2046766e191017e7acf29e84356465feef76e821708ff18e179")
```

returns

```json
{
  "accessList": [],
  "blockHash": "0xd319b685a1a0b88545ec6df473a3efb903358ac655263868bb14b92797ea7504",
  "blockNumber": 79660,
  "chainId": "0x9",
  "from": "0x618263ce921f7dd5f4f40c29f6c524aaf97b9bbd",
  "gas": 125060,
  "gasPrice": 1500000007,
  "hash": "0xe27dc466c36ad2046766e191017e7acf29e84356465feef76e821708ff18e179",
  "input": "0xeb672419000000000000000000000000618263ce921f7dd5f4f40c29f6c524aaf97b9bbd00000000000000000000000000000000000000000000000029a2241af62c000000000000000000000000000000000000000000000000000000000000000000e0000000000000000000000000000000000000000000000000000000000009cb4200000000000000000000000000000000000000000000000000000000000003200000000000000000000000000000000000000000000000000000000000000100000000000000000000000000618263ce921f7dd5f4f40c29f6c524aaf97b9bbd00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
  "maxFeePerGas": 1500000010,
  "maxPriorityFeePerGas": 1500000000,
  "nonce": 40,
  "r": "0xc9b0548ade9c5d7334f1ebdfba9239cf1acca7873381b8f0bc0e8f49ae1e456f",
  "s": "0xb9dd338283a3409c281b69c3d6f1d66ea6ee5486ee6884c71d82f596d6a934",
  "to": "0x54e8159f006750466084913d5bd288d4afb1ee9a",
  "transactionIndex": 0,
  "type": "0x2",
  "v": "0x1",
  "value": 3000320929000000000
}
```

The deposit command has called the contract on address `0x54e8` (which is exactly the `CONTRACTS_DIAMOND_PROXY_ADDR`
from `deployL1.log`), and it has called the method `0xeb672419` - which is the `requestL2Transaction` from
[Mailbox.sol](https://github.com/matter-labs/era-contracts/blob/f06a58360a2b8e7129f64413998767ac169d1efd/ethereum/contracts/zksync/facets/Mailbox.sol#L220)

#### Quick note on our L1 contracts

We're using the DiamondProxy setup, that allows us to have a fixed immutable entry point (DiamondProxy) - that forwards
the requests to different contracts (facets) that can be independently updated and/or frozen.

![Diamond proxy layout](https://user-images.githubusercontent.com/128217157/229521292-1532a59b-665c-4cc4-8342-d25ad45a8fcd.png)

You can find more detailed description in
[Contract docs](https://github.com/matter-labs/era-contracts/blob/main/docs/Overview.md)

#### requestL2Transaction Function details

You can use some of the online tools (like <https://calldata-decoder.apoorv.xyz/>) and pass the input data to it - and
get the nice result:

```json
"function": "requestL2Transaction(address,uint256,bytes,uint256,uint256,bytes[],address)",
"params": [
    "0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd",
    "3000000000000000000",
    "0x",
    "641858",
    "800",
    [],
    "0x618263CE921F7dd5F4f40C29f6c524Aaf97b9bbd"
  ]

```

This means that we requested that the 3 ETH (2nd argument) is transferred to 0x6182 (1st argument). The Calldata being
0x0 - means that we're talking about ETH (this would be a different value for other ERC tokens). Then we also specify a
gas limit (641k) and set the gas per pubdata byte limit to 800. (TODO: explain what these values mean.)

#### What happens under the hood

The call to requestL2Transaction, is adding the transaction to the priorityQueue and then emits the NewPriorityRequest.

The zk server (that you started with `zk server` command) is listening on events that are emitted from this contract
(via eth_watcher module -
[`loop_iteration` function](https://github.com/matter-labs/zksync-era/blob/main/core/lib/zksync_core/src/eth_watch/mod.rs#L161)
) and adds them to the postgres database (into `transactions` table).

You can actually check it - by running the psql and looking at the contents of the table - then you'll notice that
transaction was successfully inserted, and it was also marked as 'priority' (as it came from L1) - as regular
transactions that are received by the server directly are not marked as priority.

You can verify that this is your transaction, by looking at the `l1_block_number` column (it should match the
`block_number` from the `eth.getTransaction(...)` call above).

Notice that the hash of the transaction in the postgres will be different from the one returned by
`eth.getTransaction(...)`. This is because the postgres keeps the hash of the 'L2' transaction (which was 'inside' the
L1 transaction that `eth.getTransaction(...)` returned).

## Summary

In this article, we've learned how ETH gets bridged from L1 to L2. In the [next article](03_withdrawals.md), we'll look
at the other direction - how we transmit messages (and ETH) from L2 to L1.
