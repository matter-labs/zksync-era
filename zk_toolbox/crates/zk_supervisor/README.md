# Command-Line Help for `zk_supervisor`

This document contains the help content for the `zk_supervisor` command-line program.

**Command Overview:**

- [`zk_supervisor`↴](#zk_supervisor)
- [`zk_supervisor database`↴](#zk_supervisor-database)
- [`zk_supervisor database check-sqlx-data`↴](#zk_supervisor-database-check-sqlx-data)
- [`zk_supervisor database drop`↴](#zk_supervisor-database-drop)
- [`zk_supervisor database migrate`↴](#zk_supervisor-database-migrate)
- [`zk_supervisor database new-migration`↴](#zk_supervisor-database-new-migration)
- [`zk_supervisor database prepare`↴](#zk_supervisor-database-prepare)
- [`zk_supervisor database reset`↴](#zk_supervisor-database-reset)
- [`zk_supervisor database setup`↴](#zk_supervisor-database-setup)
- [`zk_supervisor test`↴](#zk_supervisor-test)
- [`zk_supervisor test integration`↴](#zk_supervisor-test-integration)
- [`zk_supervisor test revert`↴](#zk_supervisor-test-revert)
- [`zk_supervisor test recovery`↴](#zk_supervisor-test-recovery)
- [`zk_supervisor test upgrade`↴](#zk_supervisor-test-upgrade)
- [`zk_supervisor test rust`↴](#zk_supervisor-test-rust)
- [`zk_supervisor test l1-contracts`↴](#zk_supervisor-test-l1-contracts)
- [`zk_supervisor test prover`↴](#zk_supervisor-test-prover)
- [`zk_supervisor clean`↴](#zk_supervisor-clean)
- [`zk_supervisor clean all`↴](#zk_supervisor-clean-all)
- [`zk_supervisor clean containers`↴](#zk_supervisor-clean-containers)
- [`zk_supervisor clean contracts-cache`↴](#zk_supervisor-clean-contracts-cache)
- [`zk_supervisor snapshot`↴](#zk_supervisor-snapshot)
- [`zk_supervisor snapshot create`↴](#zk_supervisor-snapshot-create)
- [`zk_supervisor lint`↴](#zk_supervisor-lint)
- [`zk_supervisor fmt`↴](#zk_supervisor-fmt)
- [`zk_supervisor fmt rustfmt`↴](#zk_supervisor-fmt-rustfmt)
- [`zk_supervisor fmt contract`↴](#zk_supervisor-fmt-contract)
- [`zk_supervisor fmt prettier`↴](#zk_supervisor-fmt-prettier)
- [`zk_supervisor prover-version`↴](#zk_supervisor-prover-version)

## `zk_supervisor`

ZK Toolbox is a set of tools for working with zk stack.

**Usage:** `zk_supervisor [OPTIONS] <COMMAND>`

###### **Subcommands:**

- `database` — Database related commands
- `test` — Run tests
- `clean` — Clean artifacts
- `snapshot` — Snapshots creator
- `lint` — Lint code
- `fmt` — Format code
- `prover-version` — Protocol version used by provers

###### **Options:**

- `-v`, `--verbose` — Verbose mode
- `--chain <CHAIN>` — Chain to use
- `--ignore-prerequisites` — Ignores prerequisites checks

## `zk_supervisor database`

Database related commands

**Usage:** `zk_supervisor database <COMMAND>`

###### **Subcommands:**

- `check-sqlx-data` — Check sqlx-data.json is up to date. If no databases are selected, all databases will be checked.
- `drop` — Drop databases. If no databases are selected, all databases will be dropped.
- `migrate` — Migrate databases. If no databases are selected, all databases will be migrated.
- `new-migration` — Create new migration
- `prepare` — Prepare sqlx-data.json. If no databases are selected, all databases will be prepared.
- `reset` — Reset databases. If no databases are selected, all databases will be reset.
- `setup` — Setup databases. If no databases are selected, all databases will be setup.

## `zk_supervisor database check-sqlx-data`

Check sqlx-data.json is up to date. If no databases are selected, all databases will be checked.

**Usage:** `zk_supervisor database check-sqlx-data [OPTIONS]`

###### **Options:**

- `-p`, `--prover <PROVER>` — Prover database

  Possible values: `true`, `false`

- `-c`, `--core <CORE>` — Core database

  Possible values: `true`, `false`

## `zk_supervisor database drop`

Drop databases. If no databases are selected, all databases will be dropped.

**Usage:** `zk_supervisor database drop [OPTIONS]`

###### **Options:**

- `-p`, `--prover <PROVER>` — Prover database

  Possible values: `true`, `false`

- `-c`, `--core <CORE>` — Core database

  Possible values: `true`, `false`

## `zk_supervisor database migrate`

Migrate databases. If no databases are selected, all databases will be migrated.

**Usage:** `zk_supervisor database migrate [OPTIONS]`

###### **Options:**

- `-p`, `--prover <PROVER>` — Prover database

  Possible values: `true`, `false`

- `-c`, `--core <CORE>` — Core database

  Possible values: `true`, `false`

## `zk_supervisor database new-migration`

Create new migration

**Usage:** `zk_supervisor database new-migration [OPTIONS]`

###### **Options:**

- `--database <DATABASE>` — Database to create new migration for

  Possible values: `prover`, `core`

- `--name <NAME>` — Migration name

## `zk_supervisor database prepare`

Prepare sqlx-data.json. If no databases are selected, all databases will be prepared.

**Usage:** `zk_supervisor database prepare [OPTIONS]`

###### **Options:**

- `-p`, `--prover <PROVER>` — Prover database

  Possible values: `true`, `false`

- `-c`, `--core <CORE>` — Core database

  Possible values: `true`, `false`

## `zk_supervisor database reset`

Reset databases. If no databases are selected, all databases will be reset.

**Usage:** `zk_supervisor database reset [OPTIONS]`

###### **Options:**

- `-p`, `--prover <PROVER>` — Prover database

  Possible values: `true`, `false`

- `-c`, `--core <CORE>` — Core database

  Possible values: `true`, `false`

## `zk_supervisor database setup`

Setup databases. If no databases are selected, all databases will be setup.

**Usage:** `zk_supervisor database setup [OPTIONS]`

###### **Options:**

- `-p`, `--prover <PROVER>` — Prover database

  Possible values: `true`, `false`

- `-c`, `--core <CORE>` — Core database

  Possible values: `true`, `false`

## `zk_supervisor test`

Run tests

**Usage:** `zk_supervisor test <COMMAND>`

###### **Subcommands:**

- `integration` — Run integration tests
- `revert` — Run revert tests
- `recovery` — Run recovery tests
- `upgrade` — Run upgrade tests
- `rust` — Run unit-tests, accepts optional cargo test flags
- `l1-contracts` — Run L1 contracts tests
- `prover` — Run prover tests

## `zk_supervisor test integration`

Run integration tests

**Usage:** `zk_supervisor test integration [OPTIONS]`

###### **Options:**

- `-e`, `--external-node` — Run tests for external node

## `zk_supervisor test revert`

Run revert tests

**Usage:** `zk_supervisor test revert [OPTIONS]`

###### **Options:**

- `--enable-consensus` — Enable consensus
- `-e`, `--external-node` — Run tests for external node

## `zk_supervisor test recovery`

Run recovery tests

**Usage:** `zk_supervisor test recovery [OPTIONS]`

###### **Options:**

- `-s`, `--snapshot` — Run recovery from a snapshot instead of genesis

## `zk_supervisor test upgrade`

Run upgrade tests

**Usage:** `zk_supervisor test upgrade`

## `zk_supervisor test rust`

Run unit-tests, accepts optional cargo test flags

**Usage:** `zk_supervisor test rust [OPTIONS]`

###### **Options:**

- `--options <OPTIONS>` — Cargo test flags

## `zk_supervisor test l1-contracts`

Run L1 contracts tests

**Usage:** `zk_supervisor test l1-contracts`

## `zk_supervisor test prover`

Run prover tests

**Usage:** `zk_supervisor test prover`

## `zk_supervisor clean`

Clean artifacts

**Usage:** `zk_supervisor clean <COMMAND>`

###### **Subcommands:**

- `all` — Remove containers and contracts cache
- `containers` — Remove containers and docker volumes
- `contracts-cache` — Remove contracts caches

## `zk_supervisor clean all`

Remove containers and contracts cache

**Usage:** `zk_supervisor clean all`

## `zk_supervisor clean containers`

Remove containers and docker volumes

**Usage:** `zk_supervisor clean containers`

## `zk_supervisor clean contracts-cache`

Remove contracts caches

**Usage:** `zk_supervisor clean contracts-cache`

## `zk_supervisor snapshot`

Snapshots creator

**Usage:** `zk_supervisor snapshot <COMMAND>`

###### **Subcommands:**

- `create` —

## `zk_supervisor snapshot create`

**Usage:** `zk_supervisor snapshot create`

## `zk_supervisor lint`

Lint code

**Usage:** `zk_supervisor lint [OPTIONS]`

###### **Options:**

- `-c`, `--check`
- `-e`, `--extensions <EXTENSIONS>`

  Possible values: `md`, `sol`, `js`, `ts`, `rs`

## `zk_supervisor fmt`

Format code

**Usage:** `zk_supervisor fmt [OPTIONS] [COMMAND]`

###### **Subcommands:**

- `rustfmt` —
- `contract` —
- `prettier` —

###### **Options:**

- `-c`, `--check`

## `zk_supervisor fmt rustfmt`

**Usage:** `zk_supervisor fmt rustfmt`

## `zk_supervisor fmt contract`

**Usage:** `zk_supervisor fmt contract`

## `zk_supervisor fmt prettier`

**Usage:** `zk_supervisor fmt prettier [OPTIONS]`

###### **Options:**

- `-e`, `--extensions <EXTENSIONS>`

  Possible values: `md`, `sol`, `js`, `ts`, `rs`

## `zk_supervisor prover-version`

Protocol version used by provers

**Usage:** `zk_supervisor prover-version`

<hr/>

<small><i> This document was generated automatically by
<a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>. </i></small>
