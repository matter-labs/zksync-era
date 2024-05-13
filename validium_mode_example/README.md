## Validium example

In order to start the node as a validium and run the example follow the next steps.

### Run the server

To run this example we need to run the server in validium mode. In the `zksync-era` directory, you can run the following
command:

```sh
zk && zk clean --all && zk init --validium-mode && zk server
```

> [!IMPORTANT] Make sure that the flag `--validium-mode` is present when initilizing the server.

This will set up the Ethereum node with the validium contracts, and also define an `env` var which the server will pick
up in order to run as a validium node.

### Run the example

In this example we're going to run some transactions.

Once the server is running, run this command in other terminal:

```sh
cargo run --release --bin validium_mode_example
```
