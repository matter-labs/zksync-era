**This is a modified version of ZKsync ERA node with experimental support of Zk OS.**

To run the node, use the same instructions as for the original ZKsync ERA node:
```
zkstack containers
zkstack ecosystem init
zkstack server
```

On server start, the wallets listed [here](https://github.com/matter-labs/zksync-era/blob/zkos-dev/core/node/zkos_state_keeper/src/keeper.rs#L188) are funded.
This list can be modified - added wallets are funded on server restart (no regenesis is needed)

Note: The chain id is hardcoded as `37`, as this value is hardcoded on the Zk OS side.

**Missing features**:

TODOs and missing features

**XL**:
* Gas and pubdata price are hardcoded and/or ignored (zksync-era and zk_ee)
    * `estimate_gas` returns `u32::MAX` instead of running the transaction
    * mempool transactions are not filtered
* Tracing is not implemented (zksync-era and zk_ee)
* Tree is not persisted and is rebuilt on every start. We need a persistent implementation (zksync-era)

**L**:
* Current binary is not compatible with the original ZKsync ERA node - we need to support both systems, passing the target mode as binary parameter (zksync-era)
* L1 Batch and Miniblock header are missing multiple fields. Need to go through them and decide what meaning they have (L, zksync-era and zk_ee)
* 
**M**:
* Genesis is commented out - we need to figure out what's expected there (zksync-era)
* `zk_ee` [requires](https://github.com/matter-labs/zk_ee/blob/main/forward_system/src/run/tree.rs#L9) `'static` for storage provide - currently `RefCell` is used to satisfy it (zksync-era and zk_ee)
* Make the error format and conditinos in `eth_call` compatible with Ethereum (zksync-era and zk_ee)
* Transaction replacement is not supported (M, zksync-era)

**S**:
* we clone the whole tree in state keeper - at least use RefCell instead (zksync-era)


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
