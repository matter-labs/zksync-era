# Development guide

This document outlines the steps for setting up and working with ZKsync.

## Prerequisites

If you haven't already, install the prerequisites as described in [Install Dependencies](./setup-dev.md).

## Installing the local ZK Stack CLI

To set up local development, begin by installing
[ZK Stack CLI](https://github.com/matter-labs/zksync-era/blob/main/zkstack_cli/README.md). From the project's root
directory, run the following commands:

```bash
cd ./zkstack_cli/zkstackup
./install --local
```

This installs `zkstackup` in your user binaries directory (e.g., `$HOME/.local/bin/`) and adds it to your `PATH`.

After installation, open a new terminal or reload your shell profile. From the project's root directory, you can now
run:

```bash
zkstackup --local
```

This command installs `zkstack` from the current source directory.

You can proceed to verify the installation and start familiarizing with the CLI by running:

```bash
zkstack --help
```

> NOTE: Whenever you want to update you local installation with your changes, just rerun:
>
> ```bash
> zkstackup --local
> ```
>
> You might find convenient to add this alias to your shell profile:
>
> `alias zkstackup='zkstackup --path /path/to/zksync-era'`

## Configure Ecosystem

The project root directory includes configuration files for an ecosystem with a single chain, `era`. To initialize the
ecosystem, first start the required containers:

```bash
zkstack containers
```

Next, run:

```bash
zkstack ecosystem init
```

These commands will guide you through the configuration options for setting up the ecosystem.

> NOTE: For local development only. You can also use the development defaults by supplying the `--dev` flag.

Initialization may take some time, but key steps (such as downloading and unpacking keys or setting up containers) only
need to be completed once.

To see more detailed output, you can run commands with the `--verbose` flag.

## Cleanup

To clean up the local ecosystem (e.g., removing containers and clearing the contract cache), run:

```bash
zkstack dev clean all
```

You can then reinitialize the ecosystem as described in the [Configure Ecosystem](#configure-ecosystem) section.

```bash
zkstack containers
zkstack ecosystem init
```

## Committing changes

`zksync` uses pre-commit and pre-push git hooks for basic code integrity checks. Hooks are set up automatically within
the workspace initialization process. These hooks will not allow to commit the code which does not pass several checks.

Currently the following criteria are checked:

- Code must be formatted via `zkstack dev fmt`.
- Code must be linted via `zkstack dev lint`.

## Testing

ZKstack CLI offers multiple subcommands to run specific integration and unit test:

```bash
zkstack dev test --help
```

```bash
Usage: zkstack dev test [OPTIONS] <COMMAND>

Commands:
  integration   Run integration tests
  fees          Run fees test
  revert        Run revert tests
  recovery      Run recovery tests
  upgrade       Run upgrade tests
  build         Build all test dependencies
  rust          Run unit-tests, accepts optional cargo test flags
  l1-contracts  Run L1 contracts tests
  prover        Run prover tests
  wallet        Print test wallets information
  loadtest      Run loadtest
  help          Print this message or the help of the given subcommand(s)
```

### Running unit tests

You can run unit tests for the Rust crates in the project by running:

```bash
zkstack dev test rust
```

### Running integration tests

Running integration tests is more complex. Some tests require a running server, while others need the system to be in a
specific state. Please refer to our CI scripts
[ci-core-reusable.yml](https://github.com/matter-labs/zksync-era/blob/main/.github/workflows/ci-core-reusable.yml) to
have a better understanding of the process.

### Running load tests

The current load test implementation only supports the legacy bridge. To use it, you need to create a new chain with
legacy bridge support:

```bash
zkstack chain create --legacy-bridge
zkstack chain init
```

After initializing the chain with a legacy bridge, you can run the load test against it.

```bash
zkstack dev test loadtest
```

> WARNING: Never use legacy bridges in non-testing environments.

## Contracts

### Build contracts

Run:

```bash
zkstack dev contracts --help
```

to see all the options.

### Publish source code on Etherscan

#### Verifier Options

Most commands interacting with smart contracts support the same verification options as Foundry's `forge` command. Just
double check if the following options are available in the subcommand:

```bash
--verifier                  -- Verifier to use
--verifier-api-key          -- Verifier API key
--verifier-url              -- Verifier URL, if using a custom provider
```

#### Using Foundry

You can use `foundry` to verify the source code of the contracts.

```bash
forge verify-contract
```

Verifies a smart contract on a chosen verification provider.

You must provide:

- The contract address
- The contract name or the path to the contract.
- In case of Etherscan verification, you must also provide:
  - Your Etherscan API key, either by passing it as an argument or setting `ETHERSCAN_API_KEY`

For more information check [Foundry's documentation](https://book.getfoundry.sh/reference/forge/forge-verify-contract).
