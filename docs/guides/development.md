# Development guide

This document covers development-related actions in ZKsync.

## Initializing the project

To setup the main toolkit, `zk`, simply run:

```
zk
```

You may also configure autocompletion for your shell via:

```
zk completion install
```

Once all the dependencies were installed, project can be initialized:

```
zk init
```

This command will do the following:

- Generate `$ZKSYNC_HOME/etc/env/target/dev.env` file with settings for the applications.
- Initialize docker containers with `reth` Ethereum node for local development.
- Download and unpack files for cryptographical backend.
- Generate required smart contracts.
- Compile all the smart contracts.
- Deploy smart contracts to the local Ethereum network.
- Create “genesis block” for server.

Initializing may take pretty long, but many steps (such as downloading & unpacking keys and initializing containers) are
required to be done only once.

Usually, it is a good idea to do `zk init` once after each merge to the `main` branch (as application setup may change).

Additionally, there is a subcommand `zk clean` to remove previously generated data. Examples:

```
zk clean --all # Remove generated configs, database and backups.
zk clean --config # Remove configs only.
zk clean --database # Remove database.
zk clean --backups # Remove backups.
zk clean --database --backups # Remove database *and* backups, but not configs.
```

**When do you need it?**

1. If you have an initialized database and want to run `zk init`, you have to remove the database first.
2. If after getting new functionality from the `main` branch your code stopped working and `zk init` doesn't help, you
   may try removing `$ZKSYNC_HOME/etc/env/target/dev.env` and running `zk init` once again. This may help if the
   application configuration has changed.

If you don’t need all of the `zk init` functionality, but just need to start/stop containers, use the following
commands:

```
zk up   # Set up `reth` and `postgres` containers
zk down # Shut down `reth` and `postgres` containers
```

## Reinitializing

When actively changing something that affects infrastructure (for example, contracts code), you normally don't need the
whole `init` functionality, as it contains many external steps (e.g. deploying ERC20 tokens) which don't have to be
redone.

For this case, there is an additional command:

```
zk reinit
```

This command does the minimal subset of `zk init` actions required to "reinitialize" the network. It assumes that
`zk init` was called in the current environment before. If `zk reinit` doesn't work for you, you may want to run
`zk init` instead.

## Committing changes

`zksync` uses pre-commit and pre-push git hooks for basic code integrity checks. Hooks are set up automatically within
the workspace initialization process. These hooks will not allow to commit the code which does not pass several checks.

Currently the following criteria are checked:

- Rust code should always be formatted via `cargo fmt`.
- Other code should always be formatted via `zk fmt`.
- Dummy Prover should not be staged for commit (see below for the explanation).

## Using Dummy Prover

By default, the chosen prover is a "dummy" one, meaning that it doesn't actually compute proofs but rather uses mocks to
avoid expensive computations in the development environment.

To switch dummy prover to real prover, one must change `dummy_verifier` to `false` in `contracts.toml` for your env
(most likely, `etc/env/base/contracts.toml`) and run `zk init` to redeploy smart contracts.

## Testing

- Running the `rust` unit-tests:

  ```
  zk test rust
  ```

- Running a specific `rust` unit-test:

  ```
  zk test rust --package <package_name> --lib <mod>::tests::<test_fn_name>
  # e.g. zk test rust --package zksync_core --lib eth_sender::tests::resend_each_block
  ```

- Running the integration test:

  ```
  zk server           # Has to be run in the 1st terminal
  zk test i server    # Has to be run in the 2nd terminal
  ```

- Running the benchmarks:

  ```
  zk f cargo bench
  ```

- Running the loadtest:

  ```
  zk server # Has to be run in the 1st terminal
  zk prover # Has to be run in the 2nd terminal if you want to use real prover, otherwise it's not required.
  zk run loadtest # Has to be run in the 3rd terminal
  ```

## Contracts

### Re-build contracts

```
zk contract build
```

### Publish source code on etherscan

```
zk contract publish
```
