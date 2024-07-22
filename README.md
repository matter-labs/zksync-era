# Fork Code Conventions !!IMPORTANT

The original repository of this fork is still in Alpha, so many breaking changes are expected. With that in mind, we've
developed a strategy to minimize synchronization conflicts as much as possible, employing the following heuristics:

- Copy, don't modify. Do not modify a source file, instead, clone it with the prefix `via-`.

For example, if we are using a library, let's say `core/lib/dal/src/metrics.rs` and we need to modify the file, we copy
and paste it attaching the prefix, like so: `core/lib/dal/src/via-metrics.rs`.

That way, when we synchronize this fork, we will never have a git conflict because we've created a new file. Then, we
can look at the changes in the original file and decide if we want to implement those changes in the copied file.

This rule also applies to binaries.

## Running our new binaries

If we create a new binary, we should add it to the `./Cargo.toml` under “members” section, and execute it with cargo -p
flag: `cargo run -p new-bin`.

# zkSync Era: A ZK Rollup For Scaling Ethereum

[![Logo](eraLogo.png)](https://zksync.io/)

ZKsync Era is a layer 2 rollup that uses zero-knowledge proofs to scale Ethereum without compromising on security or
decentralization. Since it's EVM compatible (Solidity/Vyper), 99% of Ethereum projects can redeploy without refactoring
or re-auditing a single line of code. ZKsync Era also uses an LLVM-based compiler that will eventually let developers
write smart contracts in C++, Rust and other popular languages.

## Knowledge Index

The following questions will be answered by the following resources:

| Question                                                | Resource                                       |
| ------------------------------------------------------- | ---------------------------------------------- |
| What do I need to develop the project locally?          | [development.md](docs/guides/development.md)   |
| How can I set up my dev environment?                    | [setup-dev.md](docs/guides/setup-dev.md)       |
| How can I run the project?                              | [launch.md](docs/guides/launch.md)             |
| What is the logical project structure and architecture? | [architecture.md](docs/guides/architecture.md) |
| Where can I find protocol specs?                        | [specs.md](docs/specs/README.md)               |
| Where can I find developer docs?                        | [docs](https://docs.zksync.io)                 |

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
