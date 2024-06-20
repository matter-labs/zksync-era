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

## Spell Checking

In our development workflow, we utilize a spell checking process to ensure the quality and accuracy of our documentation
and code comments. This is achieved using two primary tools: `cspell` and `cargo-spellcheck`. This section outlines how
to use these tools and configure them for your needs.

### Using the Spellcheck Command

The spell check command `zk spellcheck` is designed to check for spelling errors in our documentation and code. To run
the spell check, use the following command:

```
zk spellcheck
Options:
--pattern <pattern>: Specifies the glob pattern for files to check. Default is docs/**/*.
--use-cargo: Utilize cargo spellcheck.
--use-cspell: Utilize cspell.
```

### General Rules

**Code References in Comments**: When referring to code elements within development comments, they should be wrapped in
backticks. For example, reference a variable as `block_number`.

**Code Blocks in Comments**: For larger blocks of pseudocode or commented-out code, use code blocks formatted as
follows:

````
// ```
// let overhead_for_pubdata = {
//     let numerator: U256 = overhead_for_block_gas * total_gas_limit
//         + gas_per_pubdata_byte_limit * U256::from(MAX_PUBDATA_PER_BLOCK);
//     let denominator =
//         gas_per_pubdata_byte_limit * U256::from(MAX_PUBDATA_PER_BLOCK) + overhead_for_block_gas;
// ```
````

**Language Settings**: We use the Hunspell language setting of `en_US`.

**CSpell Usage**: For spell checking within the `docs/` directory, we use `cspell`. The configuration for this tool is
found in `cspell.json`. It's tailored to check our documentation for spelling errors.

**Cargo-Spellcheck for Rust and Dev Comments**: For Rust code and development comments, `cargo-spellcheck` is used. Its
configuration is maintained in `era.cfg`.

### Adding Words to the Dictionary

To add a new word to the spell checker dictionary, navigate to `/spellcheck/era.dic` and include the word. Ensure that
the word is relevant and necessary to be included in the dictionary to maintain the integrity of our documentation.

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
