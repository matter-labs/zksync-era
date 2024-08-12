# Running the application

This document covers common scenarios for launching ZKsync applications set locally.

## Prerequisites

Prepare dev environment prerequisites: see

[Installing dependencies](./setup-dev.md)

## Setup local dev environment

Setup:

```
zk # installs and builds zk itself
zk init
```

If you face any other problems with the `zk init` command, go to the
[Troubleshooting](https://github.com/matter-labs/zksync-era/blob/main/docs/guides/launch.md#troubleshooting) section at
the end of this file. There are solutions for some common error cases.

To completely reset the dev environment:

- Stop services:

  ```
  zk down
  ```

- Repeat the setup procedure above

If `zk init` has already been executed, and now you only need to start docker containers (e.g. after reboot), simply
launch:

```
zk up
```

### Run observability stack

If you want to run [Dockprom](https://github.com/stefanprodan/dockprom/) stack (Prometheus, Grafana) alongside other
containers - add `--run-observability` parameter during initialisation.

```
zk init --run-observability
```

That will also provision Grafana with
[era-observability](https://github.com/matter-labs/era-observability/tree/main/dashboards) dashboards. You can then
access it at `http://127.0.0.1:3000/` under credentials `admin/admin`.

> If you don't see any data displayed on the Grafana dashboards - try setting the timeframe to "Last 30 minutes". You
> will also have to have `jq` installed on your system.

## (Re)deploy db and contracts

```
zk contract redeploy
```

## Environment configurations

Env config files are held in `etc/env/target/`

List configurations:

```
zk env
```

Switch between configurations:

```
zk env <ENV_NAME>
```

Default configuration is `dev.env`, which is generated automatically from `dev.env.example` during `zk init` command
execution.

## Build and run server

Run server:

```
zk server
```

Server is configured using env files in `./etc/env` directory. After the first initialization, file
`./etc/env/target/dev.env`will be created. By default, this file is copied from the `./etc/env/target/dev.env.example`
template.

Make sure you have environment variables set right, you can check it by running: `zk env`. You should see `* dev` in
output.

## Running server using Google cloud storage object store instead of default In memory store

Get the service_account.json file containing the GCP credentials from kubernetes secret for relevant environment(stage2/
testnet2) add that file to the default location ~/gcloud/service_account.json or update object_store.toml with the file
location

```
zk server
```

## Running prover server

Running on machine without GPU

```shell
zk f cargo +nightly run --release --bin zksync_prover
```

Running on machine with GPU

```shell
zk f cargo +nightly run --features gpu --release --bin zksync_prover
```

## Running the verification key generator

```shell
# ensure that the setup_2^26.key in the current directory, the file can be download from  https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2\^26.key

# To generate all verification keys
cargo run --release --bin zksync_verification_key_generator


```

## Generating binary verification keys for existing json verification keys

```shell
cargo run --release --bin zksync_json_to_binary_vk_converter -- -o /path/to/output-binary-vk
```

## Generating commitment for existing verification keys

```shell
cargo run --release --bin zksync_commitment_generator
```

## Running the contract verifier

```shell
# To process fixed number of jobs
cargo run --release --bin zksync_contract_verifier -- --jobs-number X

# To run until manual exit
zk contract_verifier
```

## Troubleshooting

### SSL error: certificate verify failed

**Problem**. `zk init` fails with the following error:

```
Initializing download: https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2%5E20.key
SSL error: certificate verify failed
```

**Solution**. Make sure that the version of `axel` on your computer is `2.17.10` or higher.

### rmSync is not a function

**Problem**. `zk init` fails with the following error:

```
fs_1.default.rmSync is not a function
```

**Solution**. Make sure that the version of `node.js` installed on your computer is `14.14.0` or higher.

### Invalid bytecode: ()

**Problem**. `zk init` fails with an error similar to:

```
Running `target/release/zksync_server --genesis`
2023-04-05T14:23:40.291277Z  INFO zksync_core::genesis: running regenesis
thread 'main' panicked at 'Invalid bytecode: ()', core/lib/utils/src/bytecode.rs:159:10
stack backtrace:
   0:        0x104551410 - std::backtrace_rs::backtrace::libunwind::trace::hf9c5171f212b04e2
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/../../backtrace/src/backtrace/libunwind.rs:93:5
   1:        0x104551410 - std::backtrace_rs::backtrace::trace_unsynchronized::h179003f6ec753118
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/../../backtrace/src/backtrace/mod.rs:66:5
   2:        0x104551410 - std::sys_common::backtrace::_print_fmt::h92d38f701cf42b17
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/sys_common/backtrace.rs:65:5
   3:        0x104551410 - <std::sys_common::backtrace::_print::DisplayBacktrace as core::fmt::Display>::fmt::hb33e6e8152f78c95
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/sys_common/backtrace.rs:44:22
   4:        0x10456cdb0 - core::fmt::write::hd33da007f7a27e39
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/core/src/fmt/mod.rs:1208:17
   5:        0x10454b41c - std::io::Write::write_fmt::h7edc10723862001e
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/io/mod.rs:1682:15
   6:        0x104551224 - std::sys_common::backtrace::_print::h5e00f05f436af01f
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/sys_common/backtrace.rs:47:5
   7:        0x104551224 - std::sys_common::backtrace::print::h895ee35b3f17b334
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/sys_common/backtrace.rs:34:9
   8:        0x104552d84 - std::panicking::default_hook::{{closure}}::h3b7ee083edc2ea3e
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:267:22
   9:        0x104552adc - std::panicking::default_hook::h4e7c2c28eba716f5
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:286:9
  10:        0x1045533a8 - std::panicking::rust_panic_with_hook::h1672176227032c45
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:688:13
  11:        0x1045531c8 - std::panicking::begin_panic_handler::{{closure}}::h0b2d072f9624d32e
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:579:13
  12:        0x104551878 - std::sys_common::backtrace::__rust_end_short_backtrace::he9abda779115b93c
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/sys_common/backtrace.rs:137:18
  13:        0x104552f24 - rust_begin_unwind
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:575:5
  14:        0x1045f89c0 - core::panicking::panic_fmt::h23ae44661fec0889
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/core/src/panicking.rs:64:14
  15:        0x1045f8ce0 - core::result::unwrap_failed::h414a6cbb12b1e143
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/core/src/result.rs:1791:5
  16:        0x103f79a30 - zksync_utils::bytecode::hash_bytecode::h397dd7c5b6202bf4
  17:        0x103e47e78 - zksync_contracts::BaseSystemContracts::load_from_disk::h0e2da8f63292ac46
  18:        0x102d885a0 - zksync_core::genesis::ensure_genesis_state::{{closure}}::h5143873f2c337e11
  19:        0x102d7dee0 - zksync_core::genesis_init::{{closure}}::h4e94f3d4ad984788
  20:        0x102d9c048 - zksync_server::main::{{closure}}::h3fe943a3627d31e1
  21:        0x102d966f8 - tokio::runtime::park::CachedParkThread::block_on::h2f2fdf7edaf08470
  22:        0x102df0dd4 - tokio::runtime::runtime::Runtime::block_on::h1fd1d83272a23194
  23:        0x102e21470 - zksync_server::main::h500621fd4d160768
  24:        0x102d328f0 - std::sys_common::backtrace::__rust_begin_short_backtrace::h52973e519e2e8a0d
  25:        0x102e08ea8 - std::rt::lang_start::{{closure}}::hbd395afe0ab3b799
  26:        0x10454508c - core::ops::function::impls::<impl core::ops::function::FnOnce<A> for &F>::call_once::ha1c2447b9b665e13
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/core/src/ops/function.rs:606:13
  27:        0x10454508c - std::panicking::try::do_call::ha57d6d1e9532dc1f
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:483:40
  28:        0x10454508c - std::panicking::try::hca0526f287961ecd
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:447:19
  29:        0x10454508c - std::panic::catch_unwind::hdcaa7fa896e0496a
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panic.rs:137:14
  30:        0x10454508c - std::rt::lang_start_internal::{{closure}}::h142ec071d3766871
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/rt.rs:148:48
  31:        0x10454508c - std::panicking::try::do_call::h95f5e55d6f048978
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:483:40
  32:        0x10454508c - std::panicking::try::h0fa00e2f7b4a5c64
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panicking.rs:447:19
  33:        0x10454508c - std::panic::catch_unwind::h1765f149814d4d3e
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/panic.rs:137:14
  34:        0x10454508c - std::rt::lang_start_internal::h00a235e820a7f01c
                               at /rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/std/src/rt.rs:148:20
  35:        0x102e21578 - _main
Error: Genesis is not needed (either Postgres DB or tree's Rocks DB is not empty)
```

**Description**. This means that your bytecode config file has an empty entry: `"bytecode": "0x"`. This happens because
your `zksync-2-dev/etc/system-contracts/package.json`'s dependency on `"@matterlabs/hardhat-zksync-solc"` is outdated.
We don't expect this error to happen as we've updated to latest version which fixes the problem.

**Solution**. Update your dependency and reinit:

```
yarn add -D @matterlabs/hardhat-zksync-solc # in the system-contracts folder
zk clean --all && zk init
```

On the run, it moved from:

```
    "@matterlabs/hardhat-zksync-solc": "^0.3.14-beta.3",
```

to:

```
    "@matterlabs/hardhat-zksync-solc": "^0.3.15",
```

### Error: Bytecode length in 32-byte words must be odd

**Problem**. `zk init` fails with an error similar to:

```
Successfully generated Typechain artifacts!
Error: Error: Bytecode length in 32-byte words must be odd
    at hashL2Bytecode (/Users/emilluta/code/zksync-2-dev/contracts/zksync/src/utils.ts:29:15)
    at computeL2Create2Address (/Users/emilluta/code/zksync-2-dev/contracts/zksync/src/utils.ts:53:26)
    at /Users/emilluta/code/zksync-2-dev/contracts/zksync/src/compileAndDeployLibs.ts:50:63
    at step (/Users/emilluta/code/zksync-2-dev/contracts/zksync/src/compileAndDeployLibs.ts:33:23)
    at Object.next (/Users/emilluta/code/zksync-2-dev/contracts/zksync/src/compileAndDeployLibs.ts:14:53)
    at fulfilled (/Users/emilluta/code/zksync-2-dev/contracts/zksync/src/compileAndDeployLibs.ts:5:58)
error Command failed with exit code 1.
info Visit https://yarnpkg.com/en/docs/cli/run for documentation about this command.
error Command failed.
Exit code: 1
Command: /Users/emilluta/.nvm/versions/node/v16.19.1/bin/node
Arguments: /opt/homebrew/Cellar/yarn/1.22.19/libexec/lib/cli.js compile-and-deploy-libs
Directory: /Users/emilluta/code/zksync-2-dev/contracts/zksync
Output:

info Visit https://yarnpkg.com/en/docs/cli/workspace for documentation about this command.
error Command failed with exit code 1.
info Visit https://yarnpkg.com/en/docs/cli/run for documentation about this command.
Error: Child process exited with code 1
```

**Description**. This means that your bytecode config file has an empty entry: `"bytecode": "0x"`. This happens because
your `zksync-2-dev/contracts/zksync/package.json`'s dependency on `"@matterlabs/hardhat-zksync-solc"` is outdated. We
don't expect this error to happen as we've updated to latest version which fixes the problem.

**Solution**. Update your dependency and reinit:

```
yarn add -D @matterlabs/hardhat-zksync-solc # in the system-contracts folder
zk clean --all && zk init
```

On the run, it moved from:

```
    "@matterlabs/hardhat-zksync-solc": "^0.3.14-beta.3",
```

to:

```
    "@matterlabs/hardhat-zksync-solc": "^0.3.15",
```

### Error: Cannot read properties of undefined (reading 'compilerPath')

**Problem**. `zk init` fails with an error similar to the following:

```text
Yarn project directory: /Users/<user>/Projects/zksync-era/contracts/system-contracts
Error: Cannot read properties of undefined (reading 'compilerPath')
error Command failed with exit code 1.
```

**Description**. The compiler downloader [could not verify](https://github.com/NomicFoundation/hardhat/blob/0d850d021f3ab33b59b1ea2ae70d1e659e579e40/packages/hardhat-core/src/internal/solidity/compiler/downloader.ts#L336-L383) that the Solidity compiler it downloaded actually works.

**Solution**. Delete the cached `*.does.not.work` file to run the check again:

```sh
# NOTE: Compiler version, commit hash may differ.
rm $HOME/Library/Caches/hardhat-nodejs/compilers-v2/macosx-amd64/solc-macosx-amd64-v0.8.20+commit.a1b79de6.does.not.work
```
