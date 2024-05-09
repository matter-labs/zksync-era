# Prover CLI

CLI tool for performing maintenance of a zkSync Prover

## Installation

```
git clone git@github.com:matter-labs/zksync-era.git
cargo install --path prover/prover_cli/
```

> This should be `cargo install zksync-prover-cli` or something similar ideally.

## Usage

> NOTE: For the moment it is necessary to run the CLI commands with `zk f`.

```
Usage: prover_cli <COMMAND>

Commands:
  file-info
  status
  help       Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

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

#### `prover_cli status l1`

Retrieve information about the state of the batches sent to L1 and compare the contract hashes in L1 with those stored
in the prover database.

#### Example Output

```
zk f run --release -- status l1

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

TODO

### `prover_cli delete`

TODO

### `prover_cli config`

TODO

## Development Status

| **Command** | **Subcommand** | **Flags**                         | **Status** |
| ----------- | -------------- | --------------------------------- | ---------- |
| `status`    | `batch`        | `-n <BATCH_NUMBER>`               | ✅         |
|             |                | `-v, --verbose`                   | 🏗️         |
|             | `l1`           |                                   | 🏗️         |
| `restart`   | `batch`        | `-n <BATCH_NUMBER>`               | ❌         |
|             | `jobs`         | `-n <BATCH_NUMBER>`               | ❌         |
|             |                | `-bwg, --basic-witness-generator` | ❌         |
|             |                | `-lwg, --leaf-witness-generator`  | ❌         |
|             |                | `-nwg, --node-witness-generator`  | ❌         |
|             |                | `-rt, --recursion-tip`            | ❌         |
|             |                | `-s, --scheduler`                 | ❌         |
|             |                | `-c, --compressor`                | ❌         |
|             |                | `-f, --failed`                    | ❌         |
| `delete`    |                | `-n <BATCH_NUMBER>`               | 🏗️         |
|             |                | `-a, --all`                       | 🏗️         |
| `requeue`   |                | `—b, --batch <BATCH_NUMBER>`      | 🏗️         |
|             |                | `-a, --all`                       | 🏗️         |
| `config`    |                | `--db-url <DB_URL>`               | ❌         |
|             |                | `--max-attempts <MAX_ATTEMPTS>`   | ❌         |
