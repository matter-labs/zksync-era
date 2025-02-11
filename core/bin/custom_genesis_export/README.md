# Custom Genesis Export

The `custom_genesis_export` tool allows exporting a state from a zkSync PostgreSQL database in a format that can be
included as a custom genesis state for a new chain.

This is particularly useful in data migration scenarios where a large existing state needs to be applied to a newly
created chain.

A typical workflow could be:

- Run a chain locally, not connected to the real L1, and add all required data to it.
- Export the data using the `custom_genesis_export` tool.
- Create a new chain connected to the real ecosystem using the exported data.

## How it works

The tool exports all entries from `storage_logs`, and `factory_deps`, except those related to the system context. The
data is then written to a binary file using the Rust standard library following a simple serialisation format.

`custom_genesis_export` can be built using the following command:

```shell
cargo build --release -p custom_genesis_export
```

And then executed using the following command, where:

- `database-url` is the URL of the PostgreSQL database.
- `genesis-config-path` is the path to the `genesis.yaml` configuration file, used to set up a new chain (located in the
  `file_based` directory).
- `output-path` is the path to the generated binary output file.

```shell
custom_genesis_export --database-url=postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_validium --genesis-config-path=/Users/ischasny/Dev/zksync-era/etc/env/file_based/genesis.yaml --output-path=export.bin
```

> Please make sure that the database is not written into before running data export.

After the export is completed, the tool will make the following updates to the `genesis.yaml` file in-place:

- Update `genesis_root_hash`, `rollup_last_leaf_index`, and `genesis_commitment` to match the contents of the export
  file.
- Add a `custom_genesis_state_path` property pointing to the data export.

The modified genesis file can be used to bootstrap an ecosystem or initialize new chains. The data export will be
automatically recognized by the server during the execution of `zkstack ecosystem init ...` and
`zkstack chain create ...` commands.

### Running considerations

- All chains within the same ecosystem must be bootstrapped from the same genesis state. This is enforced at the
  protocol level. If two chains require different states, this can only be achieved by bringing the chain into the
  ecosystem through governance voting.
  - If a chain is added to the ecosystem via a vote, ensure no assets are minted on the old bridge, as this would create
    discrepancies with the new one. One should set gas prices to zero when generating a state to account for that.
- To calculate genesis parameters, the tool must load all VM logs into RAM. This is due to implementation specifics. For
  larger states, ensure the VM has sufficient RAM capacity.
- After the import, block numbers for all VM logs will be reset to zero - if the imported data has been indexed based on
  block number, such indexes will break.
- External Nodes will have to be bootstrapped from data snapshot (i.e. genesis can't be generated locally).
