**This is a modified version of ZKsync ERA node with experimental support of Zk OS.**

To run the node, please run:

To run dependencies (postgres, geth):

```
zkstack containers
```

Reinstall zkstack (not always needed)

```
zkstackup --local
```

Do a regenesis:

```
zkstack ecosystem init --deploy-paymaster --deploy-erc20 \
          --deploy-ecosystem --l1-rpc-url=http://localhost:8545 \
          --server-db-url=postgres://postgres:notsecurepassword@localhost:5432 \
          --server-db-name=zksync_server_localhost_era \
          --ignore-prerequisites --verbose \
          --observability=false
```

Now run the server. Note that it also runs prover input generator and prover input server by default

(zkos is enabled by default - you can pass --no-zkos if you want to run old node).

```
zkstack server --ignore-prerequisites --chain era
```

Server is accepting ethereum RPCs on port 3050, and prover jobs on port 3124.

Run integartion tests (most are skipped but the ones running in this suite in should be working)

```
zkstack dev test integration -v -t zkos --chain era
```

### Running prover

To run prover locally run

```
cd zkos_prover
cargo run --release
```

or with GPU (worth setting separate target dir to not recompile when enabling/disabling):

```
CARGO_TARGET_DIR=target/gpu cargo run --release --features gpu
```

You can check which jobs prover can pick up - by querying server on http://localhost:3124/prover-jobs/available

### Local testing

Usually a wallet from governor (check key in ./chains/era/configs/wallets.yaml has some funds on L2).

```
cast send -r http://localhost:3050 0x789E38eFB7fA82D8fD65D3d7f3C7e77DCB8FB2a2 --value 100 --private-key $GOVERNOR
```

### Verifying SNARK

You can upload your SNARK to https://fri-verifier.vercel.app/

For example, download it via curl:

```
curl http://localhost:3124/prover-jobs/FRI/1 -o 1.json
```

And then upload via the webpage: https://fri-verifier.vercel.app/

### Running SNARK wrapper

Go to `zksync_os_snark_prover` directory, and follow instructions from there.

## Updating app.bin & Verification keys

Snark verification key (that is put inside Verifier.sol) is computed based off app.bin and air-bender versions. So any
change to any of those things, means that we have to recompute it.

Here are the steps:

- pick a version of airbender (for example 0.3.1)
- choose the same version of zksync-os-wrapper (if it doesn't exist - you'll have to create it - instructions are in
  zksync-os-wrapper readme)
- create a version of zksync-os - preferably try to depend on the same airbender version as above (otherwise there might
  be some incompatibilities)
  - once you submit the PR with zksync-os dependency - the CI will generate the new app.bin automatically
  - copy this app.bin into zksync-era
- now, to generate the key, use the generate-snark-vk command from
  https://github.com/matter-labs/zkos-wrapper/blob/main/docs/end_to_end.md

```shell
# from zkos-wrapper dir
cargo run --bin wrapper --release -- generate-snark-vk --input-binary ../zksync-era/execution_environment/app.bin  --output-dir /tmp --universal-verifier
```

- make sure to pass the same app.bin that you're using, and the same trusted setup, that your snark wrapper will use
- once you have the vk file - put it into contracts/tools/data/plonk_scheduler_key.json file, and run the command
  zksync_verifier_contract_generator described in contracts/tools/README

```shell

cd contracts/tools && cp /tmp/snark_vk_expected.json data/plonk_scheduler_key.json && cargo run --bin zksync_verifier_contract_generator --release -- --plonk_input_path data/plonk_scheduler_key.json --fflonk_input_path data/fflonk_scheduler_key.json --plonk_output_path ../l1-contracts/contracts/state-transition/verifiers/L1VerifierPlonk.sol --fflonk_output_path ../l1-contracts/contracts/state-transition/verifiers/L1VerifierFflonk.sol
```

- this will update the L2Verifier.sol automatically
- create PR with the new app.bin and the contract changes.
- remember to also update genesis.yaml and commitments.json files - and add new verification key hash there.

# ZKsync Era: A ZK Rollup For Scaling Ethereum

[![Logo](eraLogo.png)](https://zksync.io/)

ZKsync Era is a layer 2 rollup that uses zero-knowledge proofs to scale Ethereum without compromising on security or
decentralization. Since it's EVM compatible (Solidity/Vyper), 99% of Ethereum projects can redeploy without refactoring
or re-auditing a single line of code. ZKsync Era also uses an LLVM-based compiler that will eventually let developers
write smart contracts in C++, Rust and other popular languages.

## Documentation

The most recent documentation can be found here:

- [Core documentation](https://matter-labs.github.io/zksync-era/core/latest/)
- [Prover documentation](https://matter-labs.github.io/zksync-era/prover/latest/)

## Policies

- [Security policy](SECURITY.md)
- [Contribution policy](CONTRIBUTING.md)

## License

ZKsync Era is distributed under the terms of either

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/blog/license/mit/>)

at your option.

## Official Links

- [Website](https://zksync.io/)
- [GitHub](https://github.com/matter-labs)
- [ZK Credo](https://github.com/zksync/credo)
- [Twitter](https://twitter.com/zksync)
- [Twitter for Developers](https://twitter.com/zkSyncDevs)
- [Discord](https://join.zksync.dev/)
- [Mirror](https://zksync.mirror.xyz/)
- [Youtube](https://www.youtube.com/@zkSync-era)

## Disclaimer

ZKsync Era has been through lots of testing and audits. Although it is live, it is still in alpha state and will go
through more audits and bug bounty programs. We would love to hear our community's thoughts and suggestions about it! It
is important to state that forking it now can potentially lead to missing important security updates, critical features,
and performance improvements.
