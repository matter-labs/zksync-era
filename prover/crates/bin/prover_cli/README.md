# Prover CLI

CLI tool for performing maintenance of a ZKsync Prover

## Installation

```
git clone git@github.com:matter-labs/zksync-era.git
cargo install prover_cli
```

Or

```
cargo +nightly-2024-09-01 install --git https://github.com/matter-labs/zksync-era/ --locked prover_cli --force
```

## Usage

```
Usage: prover_cli [DB_URL] <COMMAND>

Commands:
  debug-proof
  file-info
  config
  delete
  status
  requeue
  restart
  stats        Displays L1 Batch proving stats for a given period
  help         Print this message or the help of the given subcommand(s)

Arguments:
  [DB_URL]  [env: PLI__DB_URL=] [default: postgres://postgres:notsecurepassword@localhost/prover_local]

Options:
  -h, --help     Print help
  -V, --version  Print version
```

Warning: If this tool is being used outside the directory where the zksync-era repository is located, the configuration
is not persistent, so the database URL needs to be set each time a new command that requires it is called!. Work is
being done to improve this.

### `prover_cli file-info`

Displays the information about a given file.

```
Usage: prover_cli file-info --file-path <PATH_TO_BATCH_PROOF_BIN>

For more information, try '--help'.
```

#### Example Output

Example outputs:

```bash
L1 proof
AUX info:
  L1 msg linear hash: [163, 243, 172, 16, 189, 59, 100, 227, 249, 46, 226, 220, 82, 135, 213, 208, 221, 228, 49, 46, 121, 136, 78, 163, 15, 155, 199, 82, 64, 24, 172, 198]
  Rollup_state_diff_for_compression: [157, 150, 29, 193, 105, 162, 176, 61, 83, 241, 72, 206, 68, 20, 143, 69, 119, 162, 138, 101, 80, 139, 193, 211, 188, 250, 156, 86, 254, 148, 117, 60]
  bootloader_heap_initial_content: [112, 2, 120, 255, 156, 227, 172, 92, 134, 48, 247, 243, 148, 241, 11, 122, 6, 189, 46, 164, 89, 78, 209, 118, 115, 239, 195, 15, 225, 143, 97, 204]
  events_queue_state: [202, 78, 244, 233, 150, 17, 247, 25, 183, 51, 245, 110, 135, 31, 115, 109, 84, 193, 17, 1, 153, 32, 39, 199, 102, 25, 63, 216, 220, 68, 212, 233]
Inputs: [Fr(0x00000000775db828700e0ebbe0384f8a017598a271dfb6c96ebb2baf22a7a572)]
```

```bash
 == Circuit ==
Type: basic. Id: 1 (Scheduler)
Geometry: CSGeometry { num_columns_under_copy_permutation: 130, num_witness_columns: 0, num_constant_columns: 4, max_allowed_constraint_degree: 8 }
Circuit size: trace length: Some(1048576) something??: Some(100663296)
Scheduler witness info
Previous block data:
Enumeration counter: 25
State root: [107, 233, 138, 154, 21, 134, 189, 220, 183, 250, 117, 243, 103, 124, 71, 221, 160, 136, 249, 25, 197, 109, 8, 75, 26, 12, 81, 109, 36, 56, 30, 17]
Block meta parameters
bootloader code hash: 452367551810219221093730953379759186922674186246309239546886848509599206765
aa code hash: 452349823575802367618424269668644286404749728714566974110193150711820505769
Previous block meta hash: [63, 236, 0, 236, 23, 236, 175, 242, 75, 187, 203, 193, 88, 80, 202, 53, 40, 206, 28, 40, 125, 58, 53, 254, 233, 122, 108, 101, 101, 88, 102, 193]
Previous block aux hash: [200, 12, 70, 33, 103, 13, 251, 174, 96, 165, 135, 138, 34, 75, 249, 81, 93, 86, 110, 52, 30, 172, 198, 51, 155, 82, 86, 137, 156, 215, 11, 119]
EIP 4844 - witnesses: None
EIP 4844 - proofs: 0
```

### `prover_cli status`

Set of commands to inspect the status of the Prover. This could be the status of the proof for some batch or the status
of the proving in the L1.

#### `prover_cli status batch`

Displays the proof status for a given batch or a set of batches.

```
Usage: prover_cli status <COMMAND>

Commands:
  batch
  l1
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

#### Example Output

```
pli status batch -b 4

== Batch 4 Status ==
> In Progress ⌛️

== Proving Stages ==
-- Aggregation Round 0 --
Basic Witness Generator: Done ✅
Prover Jobs:
> In progress ⌛️
-- Aggregation Round 1 --
Leaf Witness Generator: In progress ⌛️
> Prover Jobs: Waiting for proofs ⏱️
-- Aggregation Round 2 --
Node Witness Generator: In progress ⌛️
> Prover Jobs: Waiting for proofs ⏱️
-- Aggregation Round 3 --
Recursion Tip: In progress ⌛️
> Prover Jobs: Waiting for proofs ⏱️
-- Aggregation Round 3 --
Scheduler: In progress ⌛️
> Prover Jobs: Waiting for proofs ⏱️
-- Compressor --
> Compressor job not found 🚫
```

NOTE: With the --verbose flag, much more detailed information about each stage of the process is displayed.

#### `prover_cli status l1`

Retrieve information about the state of the batches sent to L1 and compare the contract hashes in L1 with those stored
in the prover database.

#### Example Output

```
prover_cli status l1

====== L1 Status ======
State keeper: First batch: 0, recent batch: 10
L1 state: block verified: 7, block committed: 9
Eth sender is 1 behind. Last block committed: 9. Most recent sealed state keeper batch: 10.
 -----------------------
Verifier key hash matches: 0x063c9c1e9d39fc0b1633c78a49f1905s65ee0982ad96d97ef7fe3d4f1f1a72c7
 -----------------------
Verification node hash in DB differs from the one in contract.
Contract hash: 0x1186ec268d49f1905f8d9c1e9d39fc33e98c74f91d91a21b8f7ef78bd09a8db8
DB hash: 0x5a3ef282b21e12fe1f4438e5bb158fc5060b160559c5158c6389d62d9fe3d080
 -----------------------
Verification leaf hash in DB differs from the one in contract.
Contract hash: 0x101e08b00193e529145ee09823378ef51a3bc8966504064f1f6ba3f1ba863210
DB hash: 0x400a4b532c6f072c00d1806ef299300d4c104f4ac55bd8698ade78894fcadc0a
 -----------------------
Verification circuits hash in DB differs from the one in contract.
Contract hash: 0x18c1639094f58177409186e8c48d9f577c9410901d2f1d486b3e7d6cf553ae4c
DB hash: 0x0000000000000000000000000000000000000000000000000000000000000000
```

### `prover_cli requeue`

Requeue all the stuck jobs for a specific batch.

```
Usage: prover_cli requeue [OPTIONS] --batch <BATCH>

Options:
  -b, --batch <BATCH>
      --max-attempts <MAX_ATTEMPTS>  Maximum number of attempts to re-queue a job. Default value is 10. NOTE: this argument is temporary and will be deprecated once the `config` command is implemented [default: 10]
  -h, --help                         Print help
```

### `prover_cli delete`

Delete all the data from the prover database.

```
Usage: prover_cli delete [OPTIONS]

Options:
  -a, --all            Delete data from all batches
  -b, --batch <BATCH>  Batch number to delete [default: 0]
  -h, --help           Print help
```

### `prover_cli config`

It allows you to change the CLI configuration; currently, it only lets you change the database URL, but work is being
done to also include information about the L1 contracts for when they are not set in the environment variables.

```
Usage: prover_cli config [DB_URL]

Arguments:
  [DB_URL]  [env: PLI__DB_URL=] [default: postgres://postgres:notsecurepassword@localhost/prover_local]

Options:
  -h, --help  Print help
```

### `prover_cli debug-proof`

Debug proof is an advanced feature that can be used to debug failing circuit proofs. It will re-run the proving circuit
for a given proof file - and print detailed debug logs.

**WARNING** - it does require compilation with `--release --features verbose_circuits` enabled (which includes all the
necessary dependencies).

Example output

```
cargo run --release --features verbose_circuits -- debug-proof --file ~/prover_jobs_23_05.bin

[call_ret_impl/far_call.rs:1012:13] max_passable.witness_hook(&*cs)().unwrap() = 535437
[call_ret_impl/far_call.rs:1024:13] leftover.witness_hook(&*cs)().unwrap() = 8518
[call_ret_impl/far_call.rs:1025:13] ergs_to_pass.witness_hook(&*cs)().unwrap() = 544211
[call_ret_impl/far_call.rs:1036:13] remaining_from_max_passable.witness_hook(&*cs)().unwrap() = 4294958522
[call_ret_impl/far_call.rs:1037:13] leftover_and_remaining_if_no_uf.witness_hook(&*cs)().unwrap() = 4294967040
[call_ret_impl/far_call.rs:1047:13] ergs_to_pass.witness_hook(&*cs)().unwrap() = 535437
[call_ret_impl/far_call.rs:1048:13] remaining_for_this_context.witness_hook(&*cs)().unwrap() = 8518
[call_ret_impl/far_call.rs:1049:13] extra_ergs_from_caller_to_callee.witness_hook(&*cs)().unwrap() = 0
[call_ret_impl/far_call.rs:1050:13] callee_stipend.witness_hook(&*cs)().unwrap() = 0
New frame as a result of FAR CALL: Some(ExecutionContextRecordWitness { this: 0x263eb3945d7cee723110c69da5fabc3c6d5a802f, caller: 0x973a7a18f29699b5b976a5026d795f5169cb3348, code_address: 0x0000000000000000000000000000000000000000, code_page: 2416, base_page: 3075968, heap_upper_bound: 4096, aux_heap_upper_bound: 4096, reverted_queue_head: [0x1d06f395ca74bd80, 0x3c3099adfd7d31cb, 0x119db3dd58b4aca6, 0xb8d2f7bd2c1b5e48], reverted_queue_tail: [0x1d06f395ca74bd80, 0x3c3099adfd7d31cb, 0x119db3dd58b4aca6, 0xb8d2f7bd2c1b5e48], reverted_queue_segment_len: 0, pc: 0, sp: 0, exception_handler_loc: 176, ergs_remaining: 535437, is_static_execution: false, is_kernel_mode: false, this_shard_id: 0, caller_shard_id: 0, code_shard_id: 0, context_u128_value_composite: [0, 0, 0, 0], is_local_call: false, total_pubdata_spent: 0, stipend: 0 })
thread 'main' panicked at circuit_definitions/src/aux_definitions/witness_oracle.rs:506:13:
assertion `left == right` failed
  left: 2097152
 right: 4096
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### `prover_cli debug-proof`

TODO

## Development Status

| **Command**   | **Subcommand** | **Flags**                         | **Status** |
| ------------- | -------------- | --------------------------------- | ---------- |
| `status`      | `batch`        | `-n <BATCH_NUMBER>`               | ✅         |
|               |                | `-v, --verbose`                   | ✅️        |
|               | `l1`           |                                   | ✅️        |
| `restart`     | `batch`        | `-n <BATCH_NUMBER>`               | ✅         |
|               | `jobs`         | `-n <BATCH_NUMBER>`               | ️🏗️        |
|               |                | `-bwg, --basic-witness-generator` | 🏗️         |
|               |                | `-lwg, --leaf-witness-generator`  | 🏗️         |
|               |                | `-nwg, --node-witness-generator`  | 🏗️         |
|               |                | `-rt, --recursion-tip`            | 🏗️         |
|               |                | `-s, --scheduler`                 | 🏗️         |
|               |                | `-c, --compressor`                | 🏗️         |
|               |                | `-f, --failed`                    | 🏗         |
| `delete`      |                | `-n <BATCH_NUMBER>`               | ✅️️       |
|               |                | `-a, --all`                       | ️️✅️️️️️️ |
| `requeue`     |                | `—b, --batch <BATCH_NUMBER>`      | ✅️        |
|               |                | `-a, --all`                       | ✅️️       |
| `config`      |                | `--db-url <DB_URL>`               | 🏗         |
| `debug-proof` |                | `--file <FILE>`                   | ✅️        |
| `file-info`   |                | `--file-path <FILE_PATH>`         | ✅️        |
| `stats`       |                | `--period <PERIOD>`               | ✅️        |
