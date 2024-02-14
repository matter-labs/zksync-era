# Validium example

In order to start the node as a validium and run the example follow the next steps.

## Run the server

To run this example we need to run the server in validium mode. In the `zksync-era` directory, you can run the following
command:

```sh
zk && zk clean --all && zk init --validium-mode && zk server
```

> [!IMPORTANT] Make sure that the flag `--validium-mode` is present when initializing the server.

This will set up the Ethereum node with the validium contracts, and also define an `env` var which the server will pick
up in order to run as a validium node.

## Run the example

In this example we're going to run some transactions.

Once the server is running, run this command in other terminal:

```sh
cargo run --release --bin zksync_full_stack
```

This test does the following:

- Inits a wallet
- Deposits some funds into the wallet
- Deploys a sample ERC20 contract
- Query the contract for the token name and symbol
- Mint 100000 tokens into the address `CD2a3d9F938E13CD947Ec05AbC7FE734Df8DD826`
- Transfer 1000 tokens from `CD2a3d9F938E13CD947Ec05AbC7FE734Df8DD826` to `bBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB`

## Logs and prints

- For each transaction, we use the rpc client to query the transaction details and print them out. The following fields
  are printed:
  - `Transaction hash`: The hash of the transaction
  - `Transaction gas used`: The gas used to perform this transaction.
  - `L2 fee`: The total cost of this transaction.

## Example output

You will have an output similar to this one:

```
Deposit transaction hash: 0x77f378f1857ad7ff8c1041d2ce0f7f167587a90a19f9fd923c9ea91fbdf37650
Deploy
Contract address: 0x4b5df730c2e6b28e17013a1485e5d9bc41efe021
Transaction hash 0xe08786e302027040056555bdba6e0462fdee56768d982485d80f732043013bb5
Transaction gas used 161163
L2 fee: 40290750000000

Mint
Transaction hash 0x2f5b565959c8c5ffe320a364df27f4de451ed93ee6344a838f2212397da7fe5f
Transaction gas used 124046
L2 fee: 31011500000000
L1 max fee per gas: 1200000011

Transfer 1000
Transaction hash 0x7cfadf74a5fa571ed9e2f1a115edc62f8db913d61d387177b7b07e2cb270af75
Transaction gas used 125466
L2 fee: 31366500000000
L1 max fee per gas: 1000000010
```

> [!NOTE] You can observe how the different fields evolve depending on the operation. The `transaction hash` is a
> changing field.
