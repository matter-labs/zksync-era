# Changelog

## [0.15.4](https://github.com/matter-labs/zksync-2-dev/compare/zksync-web3-v0.15.3...zksync-web3-v0.15.4) (2023-07-25)


### Bug Fixes

* **sdk:** allow null for txIndexInL1Batch in formatter ([#2232](https://github.com/matter-labs/zksync-2-dev/issues/2232)) ([474740a](https://github.com/matter-labs/zksync-2-dev/commit/474740a7f9ca648869fd8f82cc4da0fcefd9cbf7))

## [0.15.3](https://github.com/matter-labs/zksync-2-dev/compare/zksync-web3-v0.15.2...zksync-web3-v0.15.3) (2023-07-25)


### Bug Fixes

* **sdk:** Fix getting receipt for transactions rejected in statekeeper ([#2071](https://github.com/matter-labs/zksync-2-dev/issues/2071)) ([c97e494](https://github.com/matter-labs/zksync-2-dev/commit/c97e494c1ef7f58fe8632a3ebf943d775b1703cb))
* **sdk:** make new fields optional in SDK ([#2226](https://github.com/matter-labs/zksync-2-dev/issues/2226)) ([9a3b530](https://github.com/matter-labs/zksync-2-dev/commit/9a3b5307a5593664cfaa510f3511751125edb96e))

## [0.15.2](https://github.com/matter-labs/zksync-2-dev/compare/zksync-web3-v0.15.1...zksync-web3-v0.15.2) (2023-07-06)


### Features

* (DONT MERGE!) Integrate WETH bridge into server & SDK ([#1929](https://github.com/matter-labs/zksync-2-dev/issues/1929)) ([b3caf1e](https://github.com/matter-labs/zksync-2-dev/commit/b3caf1e35718c742e8d1d59427855df3b9109300))
* add tx_index_in_l1_batch field to L2ToL1Log ([#2032](https://github.com/matter-labs/zksync-2-dev/issues/2032)) ([3ce5779](https://github.com/matter-labs/zksync-2-dev/commit/3ce5779f500d5738c92e09eff13d553e20625055))
* **api:** add `gas_per_pubdata` to `zks_getTransactionDetails` ([#2085](https://github.com/matter-labs/zksync-2-dev/issues/2085)) ([dd91bb6](https://github.com/matter-labs/zksync-2-dev/commit/dd91bb673b29a17cea91e12ec95f53deba556798))

## [0.15.1](https://github.com/matter-labs/zksync-2-dev/compare/zksync-web3-v0.15.0...zksync-web3-v0.15.1) (2023-04-24)


### Bug Fixes

* add coeficient to gas limit + method for full fee estimation ([#1622](https://github.com/matter-labs/zksync-2-dev/issues/1622)) ([229cda9](https://github.com/matter-labs/zksync-2-dev/commit/229cda977daa11a98a97515a2f75d709e2e8ed9a))

## [0.15.0](https://github.com/matter-labs/zksync-2-dev/compare/zksync-web3-v0.14.4...zksync-web3-v0.15.0) (2023-04-20)


### ⚠ BREAKING CHANGES

* Implement WETH bridge, support custom bridge in sdk, bootloader gas calculation fix  ([#1633](https://github.com/matter-labs/zksync-2-dev/issues/1633))

### Features

* Implement WETH bridge, support custom bridge in sdk, bootloader gas calculation fix  ([#1633](https://github.com/matter-labs/zksync-2-dev/issues/1633)) ([eb67ec5](https://github.com/matter-labs/zksync-2-dev/commit/eb67ec555bc027137d80122873cd12a93f9234c6))


### Bug Fixes

* **sdk:** Fix getSignInput when gas parameters are 0 ([#1695](https://github.com/matter-labs/zksync-2-dev/issues/1695)) ([cf61772](https://github.com/matter-labs/zksync-2-dev/commit/cf61772ba612bd3532ad3d3b808d18e25c12973f))

## [0.14.4](https://github.com/matter-labs/zksync-2-dev/compare/zksync-web3-v0.14.1...zksync-web3-v0.14.4) (2023-04-13)


### Features

* add getL1BatchDetails method to js SDK ([#1666](https://github.com/matter-labs/zksync-2-dev/issues/1666)) ([babb8a9](https://github.com/matter-labs/zksync-2-dev/commit/babb8a94466a8f8c81a19391d61aa9ea66f9cfa8))
* **sdk:** extend BlockDetails type to include l1BatchNumber ([#1677](https://github.com/matter-labs/zksync-2-dev/issues/1677)) ([67acf90](https://github.com/matter-labs/zksync-2-dev/commit/67acf90301e401004d41361b43f2d3336a48676e))

## [0.14.0](https://github.com/matter-labs/zksync-2-dev/compare/zksync-web3-v0.13.3...zksync-web3-v0.14.0) (2023-03-21)


### ⚠ BREAKING CHANGES

* **contracts:** M6 batch of breaking changes ([#1482](https://github.com/matter-labs/zksync-2-dev/issues/1482))

### Features

* **contracts:** M6 batch of breaking changes ([#1482](https://github.com/matter-labs/zksync-2-dev/issues/1482)) ([d28e01c](https://github.com/matter-labs/zksync-2-dev/commit/d28e01ce0fbf0129c2cbba877efe65da7f7ed367))
* Make server compatible with new SDK ([#1532](https://github.com/matter-labs/zksync-2-dev/issues/1532)) ([1c52738](https://github.com/matter-labs/zksync-2-dev/commit/1c527382d1e36c04df90bdf71fe643db724acb48))
* **SDK:** Use old ABI ([#1558](https://github.com/matter-labs/zksync-2-dev/issues/1558)) ([293882f](https://github.com/matter-labs/zksync-2-dev/commit/293882f2b20c95891ecfc4b72720c82e03babc7e))


### Bug Fixes

* **sdk:** Fix address overflow when applying l2tol1 alias  ([#1527](https://github.com/matter-labs/zksync-2-dev/issues/1527)) ([8509b20](https://github.com/matter-labs/zksync-2-dev/commit/8509b20854fcb2a45ea8d1350b3f2904d99eda93))
