# Development guide

This document outlines the steps for setting up and working with ZKsync.

## Installing the local ZK Stack CLI

To set up the local toolkit, begin by installing `zkstackup`. From the project's root directory, run the following
commands:

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

The root directory includes configuration files for an ecosystem with a single chain, `era`. To initialize the
ecosystem, first start the required containers:

```bash
zkstack containers
```

Next, run:

```bash
zkstack ecosystem init
```

These commands will guide you through the configuration options for setting up the ecosystem.

Initialization may take some time, but key steps (such as downloading and unpacking keys or setting up containers) only
need to be completed once.

## Cleanup

To clean up the local ecosystem (e.g., removing containers and clearing the contract cache), run:

```bash
zkstack dev clean all
```

## Re-initialization

You can reinitialize the ecosystem afterward by running:

```bash
zkstack ecosystem init
```

## Committing changes

`zksync` uses pre-commit and pre-push git hooks for basic code integrity checks. Hooks are set up automatically within
the workspace initialization process. These hooks will not allow to commit the code which does not pass several checks.

Currently the following criteria are checked:

- Code must be formatted via `zkstack dev fmt`.
- Code must be linted via `zkstack dev lint`.

## Testing

You can run tests using `zkstack dev test` subcommand. Just run:

```bash
zkstack dev test --help`
```

to see all the options.

## Contracts

### Build contracts

Run:

```bash
zkstack dev contracts --help
```

to see all the options.

### Publish source code on Etherscan

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
