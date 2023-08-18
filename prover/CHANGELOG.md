# Changelog

## [5.30.1](https://github.com/matter-labs/zksync-2-dev/compare/prover-v5.30.0...prover-v5.30.1) (2023-08-11)


### Bug Fixes

* **prover-fri:** GPU proving use init_cs_for_external_proving with hints ([#2347](https://github.com/matter-labs/zksync-2-dev/issues/2347)) ([ca6866c](https://github.com/matter-labs/zksync-2-dev/commit/ca6866c6ef68a25e650ce8e5a4e97c42e064f292))

## [5.30.0](https://github.com/matter-labs/zksync-2-dev/compare/prover-v5.29.0...prover-v5.30.0) (2023-08-09)


### Features

* **db:** Configure statement timeout for Postgres ([#2317](https://github.com/matter-labs/zksync-2-dev/issues/2317)) ([afdbb6b](https://github.com/matter-labs/zksync-2-dev/commit/afdbb6b94d9e43b9659ff5d3428f2d9a7827b29f))
* **house-keeper:** refactor periodic job to be reusable by adding in lib ([#2333](https://github.com/matter-labs/zksync-2-dev/issues/2333)) ([ad72a16](https://github.com/matter-labs/zksync-2-dev/commit/ad72a1691b661b2b4eeaefd29375a8987b485715))
* **prover-fri:** Add concurrent circuit synthesis for FRI GPU prover ([#2326](https://github.com/matter-labs/zksync-2-dev/issues/2326)) ([aef3491](https://github.com/matter-labs/zksync-2-dev/commit/aef3491cd6af01840dd4fe5b7e530028916ffa8f))


### Bug Fixes

* **prover:** Kill prover process for edge-case in crypto thread code ([#2334](https://github.com/matter-labs/zksync-2-dev/issues/2334)) ([f2b5e1a](https://github.com/matter-labs/zksync-2-dev/commit/f2b5e1a2fcbe3053e372f15992e592bc0c32a88f))

## [5.29.0](https://github.com/matter-labs/zksync-2-dev/compare/prover-v5.28.2...prover-v5.29.0) (2023-08-07)


### Features

* **prover-fri-gpu:** use witness vector for proving ([#2310](https://github.com/matter-labs/zksync-2-dev/issues/2310)) ([0f9d5ee](https://github.com/matter-labs/zksync-2-dev/commit/0f9d5eea9b053abcc380c74a61d24031f9f79563))

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
