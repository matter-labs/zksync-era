# Changelog

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
