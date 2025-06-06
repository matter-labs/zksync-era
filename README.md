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
```
zkstack server --ignore-prerequisites --chain era --zkos
```

Run integartion tests (most are skipped but the ones running in this suite in should be working)
```
zkstack dev test integration -v -t zkos --chain era
```

To run prover locally run 
```
cd zkos_prover
cargo run
```

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
