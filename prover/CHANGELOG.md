# Changelog

## [5.28.2](https://github.com/matter-labs/zksync-2-dev/compare/prover-v5.28.1...prover-v5.28.2) (2023-08-04)


### Bug Fixes

* **doc:** update vk setup data generator doc ([#2322](https://github.com/matter-labs/zksync-2-dev/issues/2322)) ([bda18b0](https://github.com/matter-labs/zksync-2-dev/commit/bda18b0670827e7a190146147724cb1a754c55e9))

## [5.28.1](https://github.com/matter-labs/zksync-2-dev/compare/prover-v5.28.0...prover-v5.28.1) (2023-08-04)


### Bug Fixes

* **docs:** Add doc for FRI prover and update doc for vk-setup data ([#2311](https://github.com/matter-labs/zksync-2-dev/issues/2311)) ([5e3b706](https://github.com/matter-labs/zksync-2-dev/commit/5e3b7069cb53bdd229c44014533742afa26bd247))

## [5.28.0](https://github.com/matter-labs/zksync-2-dev/compare/prover-v5.27.0...prover-v5.28.0) (2023-08-04)


### Features

* **crypto-update:** update crypto deps zkevm_circuits + boojum  to fix Main VM failures ([#2255](https://github.com/matter-labs/zksync-2-dev/issues/2255)) ([d0f2f87](https://github.com/matter-labs/zksync-2-dev/commit/d0f2f876e3c477b0eccf9646a89ca2b0f9855736))
* **prover-fri:** Added vk commitment generator in CI ([#2265](https://github.com/matter-labs/zksync-2-dev/issues/2265)) ([8ad75e0](https://github.com/matter-labs/zksync-2-dev/commit/8ad75e04b0a49dee34c6fa7e3b81a21392afa186))
* **prover-fri:** Integrate GPU proving ([#2269](https://github.com/matter-labs/zksync-2-dev/issues/2269)) ([1c6ed33](https://github.com/matter-labs/zksync-2-dev/commit/1c6ed33781553f989ed73dd2ca6547040066a940))
* **prover-server-split:** Enable prover UT in CI ([#2253](https://github.com/matter-labs/zksync-2-dev/issues/2253)) ([79df2a1](https://github.com/matter-labs/zksync-2-dev/commit/79df2a1a147b00fc394be315bc9a3b4cb1fe7bea))
* **prover-server-split:** unify cargo.lock for prover component ([#2248](https://github.com/matter-labs/zksync-2-dev/issues/2248)) ([0393463](https://github.com/matter-labs/zksync-2-dev/commit/0393463b15ac98a1bbf0156198e0d27d3aa92412))
* **setup-data-fri:** use improved method for GpuSetup data ([#2305](https://github.com/matter-labs/zksync-2-dev/issues/2305)) ([997efed](https://github.com/matter-labs/zksync-2-dev/commit/997efedc3eb6655d58a2ca7e52fe38badeada518))
* Update RockDB bindings ([#2208](https://github.com/matter-labs/zksync-2-dev/issues/2208)) ([211f548](https://github.com/matter-labs/zksync-2-dev/commit/211f548fa9945b7ed5328026e526cd72c09f6a94))
* **vk-setup-data-fri:** expose GPU setup & seggegate GPU setup loading based on feature flag ([#2271](https://github.com/matter-labs/zksync-2-dev/issues/2271)) ([bfcab21](https://github.com/matter-labs/zksync-2-dev/commit/bfcab21c8656ce9c6524aef119a01b5013404cac))


### Bug Fixes

* **api:** Fix bytes deserialization by bumping web3 crate version  ([#2240](https://github.com/matter-labs/zksync-2-dev/issues/2240)) ([59ef24a](https://github.com/matter-labs/zksync-2-dev/commit/59ef24afa6ceddf506a9ac7c4b1e9fc292311095))
* **prover:** Panics in `send_report` will make provers crash ([#2273](https://github.com/matter-labs/zksync-2-dev/issues/2273)) ([85974d3](https://github.com/matter-labs/zksync-2-dev/commit/85974d3f9482307e0dbad0ec179e80886dafa42e))

## [5.25.0](https://github.com/matter-labs/zksync-2-dev/compare/prover-v5.24.0...prover-v5.25.0) (2023-08-02)


### Features

* **crypto-update:** update crypto deps zkevm_circuits + boojum  to fix Main VM failures ([#2255](https://github.com/matter-labs/zksync-2-dev/issues/2255)) ([d0f2f87](https://github.com/matter-labs/zksync-2-dev/commit/d0f2f876e3c477b0eccf9646a89ca2b0f9855736))
* **prover-fri:** Added vk commitment generator in CI ([#2265](https://github.com/matter-labs/zksync-2-dev/issues/2265)) ([8ad75e0](https://github.com/matter-labs/zksync-2-dev/commit/8ad75e04b0a49dee34c6fa7e3b81a21392afa186))
* **prover-fri:** Integrate GPU proving ([#2269](https://github.com/matter-labs/zksync-2-dev/issues/2269)) ([1c6ed33](https://github.com/matter-labs/zksync-2-dev/commit/1c6ed33781553f989ed73dd2ca6547040066a940))
* **prover-server-split:** Enable prover UT in CI ([#2253](https://github.com/matter-labs/zksync-2-dev/issues/2253)) ([79df2a1](https://github.com/matter-labs/zksync-2-dev/commit/79df2a1a147b00fc394be315bc9a3b4cb1fe7bea))
* **prover-server-split:** unify cargo.lock for prover component ([#2248](https://github.com/matter-labs/zksync-2-dev/issues/2248)) ([0393463](https://github.com/matter-labs/zksync-2-dev/commit/0393463b15ac98a1bbf0156198e0d27d3aa92412))
* Update RockDB bindings ([#2208](https://github.com/matter-labs/zksync-2-dev/issues/2208)) ([211f548](https://github.com/matter-labs/zksync-2-dev/commit/211f548fa9945b7ed5328026e526cd72c09f6a94))
* **vk-setup-data-fri:** expose GPU setup & seggegate GPU setup loading based on feature flag ([#2271](https://github.com/matter-labs/zksync-2-dev/issues/2271)) ([bfcab21](https://github.com/matter-labs/zksync-2-dev/commit/bfcab21c8656ce9c6524aef119a01b5013404cac))


### Bug Fixes

* **api:** Fix bytes deserialization by bumping web3 crate version  ([#2240](https://github.com/matter-labs/zksync-2-dev/issues/2240)) ([59ef24a](https://github.com/matter-labs/zksync-2-dev/commit/59ef24afa6ceddf506a9ac7c4b1e9fc292311095))
* **prover:** Panics in `send_report` will make provers crash ([#2273](https://github.com/matter-labs/zksync-2-dev/issues/2273)) ([85974d3](https://github.com/matter-labs/zksync-2-dev/commit/85974d3f9482307e0dbad0ec179e80886dafa42e))
