# Changelog

## [5.0.1](https://github.com/matter-labs/zksync-2-dev/compare/v5.0.0...v5.0.1) (2023-05-30)


### Bug Fixes

* **explorer-api:** remove IFs for zero address ([#1880](https://github.com/matter-labs/zksync-2-dev/issues/1880)) ([2590a69](https://github.com/matter-labs/zksync-2-dev/commit/2590a696caa3a2a3800d97aa2af5b3b355c777a2))
* **vm:** Revert "fix: Improve event spam performance ([#1882](https://github.com/matter-labs/zksync-2-dev/issues/1882))" ([#1896](https://github.com/matter-labs/zksync-2-dev/issues/1896)) ([8a07cdd](https://github.com/matter-labs/zksync-2-dev/commit/8a07cdd3e13b9add6066bafc51a5c33d5b81111d))

## [5.0.0](https://github.com/matter-labs/zksync-2-dev/compare/v4.5.0...v5.0.0) (2023-05-29)


### ⚠ BREAKING CHANGES

* Upgrade to VM1.3.2 ([#1802](https://github.com/matter-labs/zksync-2-dev/issues/1802))

### Features

* **contract-verifier:** binary for loading verified sources ([#1839](https://github.com/matter-labs/zksync-2-dev/issues/1839)) ([44fcacd](https://github.com/matter-labs/zksync-2-dev/commit/44fcacd6e4285fde0e73c90795c416ab9dd6d3c8))
* **explorer-api:** Rework `get_account_transactions_hashes_page` ([#1876](https://github.com/matter-labs/zksync-2-dev/issues/1876)) ([7bbdd0f](https://github.com/matter-labs/zksync-2-dev/commit/7bbdd0f4f814085bb1ca6559736fdb6ca32add30))
* **external node:** Concurrent data fetching ([#1855](https://github.com/matter-labs/zksync-2-dev/issues/1855)) ([fa294aa](https://github.com/matter-labs/zksync-2-dev/commit/fa294aaa929f10b6a72d29407b1f4e9071e57b5e))
* **external node:** Expose 'external_node.synced' metric ([#1843](https://github.com/matter-labs/zksync-2-dev/issues/1843)) ([1c0a5ef](https://github.com/matter-labs/zksync-2-dev/commit/1c0a5ef02317e7316d70d2929199d85b873e12de))
* **external node:** Expose sync lag metric ([#1848](https://github.com/matter-labs/zksync-2-dev/issues/1848)) ([2331175](https://github.com/matter-labs/zksync-2-dev/commit/2331175133057dbb1c35d76ced89c0061b9730d1))
* **merkle tree:** Implement full mode for the new tree ([#1825](https://github.com/matter-labs/zksync-2-dev/issues/1825)) ([438a54e](https://github.com/matter-labs/zksync-2-dev/commit/438a54e994b8f6c62d8718f67388e56dbd5eba8a))
* **merkle tree:** Integrate full mode in new tree in `MetadataCalculator` ([#1858](https://github.com/matter-labs/zksync-2-dev/issues/1858)) ([aee6fc9](https://github.com/matter-labs/zksync-2-dev/commit/aee6fc9bdcc6680a46a1d37814d1bda99343a513))
* **merkle tree:** Parallelize full mode in new tree ([#1844](https://github.com/matter-labs/zksync-2-dev/issues/1844)) ([7b835ef](https://github.com/matter-labs/zksync-2-dev/commit/7b835ef01642a9fdcae607c3ac306211f2df5ca9))
* Upgrade to VM1.3.2 ([#1802](https://github.com/matter-labs/zksync-2-dev/issues/1802)) ([e46da3d](https://github.com/matter-labs/zksync-2-dev/commit/e46da3dc67c19631690dd5c265411c47e8a0716c))


### Bug Fixes

* Add visibility to get number of GPUs ([#1830](https://github.com/matter-labs/zksync-2-dev/issues/1830)) ([8245420](https://github.com/matter-labs/zksync-2-dev/commit/8245420f2bad1c51f1f8856c8be35c6cb65485b8))
* **api:** Don't require ZkSyncConfig to instantiate API ([#1816](https://github.com/matter-labs/zksync-2-dev/issues/1816)) ([263e546](https://github.com/matter-labs/zksync-2-dev/commit/263e546a122982954cb5c37de939f851390308ae))
* **api:** set real nonce during fee estimation ([#1817](https://github.com/matter-labs/zksync-2-dev/issues/1817)) ([a3916ea](https://github.com/matter-labs/zksync-2-dev/commit/a3916eac038f6e2bb7b961e26a713cc176bd1b26))
* Don't require ZkSyncConfig to perform genesis ([#1865](https://github.com/matter-labs/zksync-2-dev/issues/1865)) ([f7e7424](https://github.com/matter-labs/zksync-2-dev/commit/f7e7424c7bd00482e3562869779e3ad344529f62))
* **external node:** Allow reorg detector to be 1 block ahead of the main node ([#1853](https://github.com/matter-labs/zksync-2-dev/issues/1853)) ([3c5a1f6](https://github.com/matter-labs/zksync-2-dev/commit/3c5a1f698af86a3739aa6e311a968e920718e368))
* **external node:** Allow reorg detector to work on executed batches too ([#1869](https://github.com/matter-labs/zksync-2-dev/issues/1869)) ([b1d991c](https://github.com/matter-labs/zksync-2-dev/commit/b1d991ccb604766da0bf7686cedb1d36ab01ad05))
* **external node:** Fix batch status gaps in batch status updater ([#1836](https://github.com/matter-labs/zksync-2-dev/issues/1836)) ([354876e](https://github.com/matter-labs/zksync-2-dev/commit/354876eb22eb2dacd237ab700b37df6ae03269e8))
* **external node:** Shutdown components on the reorg detector failure ([#1842](https://github.com/matter-labs/zksync-2-dev/issues/1842)) ([ac8395c](https://github.com/matter-labs/zksync-2-dev/commit/ac8395c90303b66bd827aa973f4308ca8cfb30d2))
* Improve event spam performance ([#1882](https://github.com/matter-labs/zksync-2-dev/issues/1882)) ([f37f858](https://github.com/matter-labs/zksync-2-dev/commit/f37f85813f2aefee792378b73fba8a64047ab371))
* make iai comparison work even when benchmark sets differ ([#1888](https://github.com/matter-labs/zksync-2-dev/issues/1888)) ([acd4054](https://github.com/matter-labs/zksync-2-dev/commit/acd405411380d75684342b3f54e2ff616aa1db43))
* **merkle tree:** Do not require object store config for external node ([#1875](https://github.com/matter-labs/zksync-2-dev/issues/1875)) ([ca5cf7a](https://github.com/matter-labs/zksync-2-dev/commit/ca5cf7a4a1d6b3778ad4085a43bd08a343efe72d))
* **object store:** Fix `block_on()` in `GoogleCloudStorage` ([#1841](https://github.com/matter-labs/zksync-2-dev/issues/1841)) ([bd60f6b](https://github.com/matter-labs/zksync-2-dev/commit/bd60f6be5f72b363fb1ca9b194f52917ca75153e))
* **setup-key-generator:** update vm version in setup-key generator ([#1867](https://github.com/matter-labs/zksync-2-dev/issues/1867)) ([3d45b1f](https://github.com/matter-labs/zksync-2-dev/commit/3d45b1fadf152c8f22ae1e980f7f45a0a7ffe1df))
* update zk_evm ([#1861](https://github.com/matter-labs/zksync-2-dev/issues/1861)) ([04121d7](https://github.com/matter-labs/zksync-2-dev/commit/04121d7cbbc6776be0aaf1aba235360b283ca794))
* **vm1.3.2:** update crypto dep to fix main vm circuit synthesis ([#1889](https://github.com/matter-labs/zksync-2-dev/issues/1889)) ([855aead](https://github.com/matter-labs/zksync-2-dev/commit/855aeadc15ef2aeca024fe1616bc438ce9910e2a))
* **vm:** include zero hash related recent fixes from 1.3.1 to 1.3.2 ([#1874](https://github.com/matter-labs/zksync-2-dev/issues/1874)) ([7e622be](https://github.com/matter-labs/zksync-2-dev/commit/7e622be4669e359be7bb6d0858e701ae34b2b963))


### Performance Improvements

* **merkle tree:** Garbage collection for tree revert artifacts ([#1866](https://github.com/matter-labs/zksync-2-dev/issues/1866)) ([8e23486](https://github.com/matter-labs/zksync-2-dev/commit/8e23486b03133e269feb412c7d2f129109a21a6a))

## [4.5.0](https://github.com/matter-labs/zksync-2-dev/compare/v4.4.0...v4.5.0) (2023-05-16)


### Features

* **merkle tree:** Parallelize tree traversal ([#1814](https://github.com/matter-labs/zksync-2-dev/issues/1814)) ([4f7bede](https://github.com/matter-labs/zksync-2-dev/commit/4f7bede980cb3e20bea26261d86cf59a78e4a8f6))
* **merkle tree:** Throttle new tree implementation ([#1835](https://github.com/matter-labs/zksync-2-dev/issues/1835)) ([1767b70](https://github.com/matter-labs/zksync-2-dev/commit/1767b70edd862e4a68d39c9c932ab997e4f81a6d))
* **state-keeper:** Implement bounded gas adjuster ([#1811](https://github.com/matter-labs/zksync-2-dev/issues/1811)) ([65e33ad](https://github.com/matter-labs/zksync-2-dev/commit/65e33addd3aadac2a9eefb041ee3678168bfbb01))
* support sepolia network ([#1822](https://github.com/matter-labs/zksync-2-dev/issues/1822)) ([79a2a0c](https://github.com/matter-labs/zksync-2-dev/commit/79a2a0ce009e841ecae1484270dafa61beee905b))


### Bug Fixes

* Add tree readiness check to healtcheck endpoint ([#1789](https://github.com/matter-labs/zksync-2-dev/issues/1789)) ([3010900](https://github.com/matter-labs/zksync-2-dev/commit/30109004986e8a19603db7f31af7a06bea3344bb))
* update zkevm-test-harness (exluding transitive dependencies) ([#1827](https://github.com/matter-labs/zksync-2-dev/issues/1827)) ([faa2900](https://github.com/matter-labs/zksync-2-dev/commit/faa29000a841ba2949bb9769dd9b9d0b01493384))


### Performance Improvements

* make pop_frame correct and use it instead of drain_frame ([#1808](https://github.com/matter-labs/zksync-2-dev/issues/1808)) ([bb58fa1](https://github.com/matter-labs/zksync-2-dev/commit/bb58fa1559985c0663fa2daa44b4ea75f2c98883))

## [4.4.0](https://github.com/matter-labs/zksync-2-dev/compare/v4.3.0...v4.4.0) (2023-05-08)


### Features

* **api:** Expose metrics about open ws ([#1805](https://github.com/matter-labs/zksync-2-dev/issues/1805)) ([5888047](https://github.com/matter-labs/zksync-2-dev/commit/5888047732f61f2916bc03f4516512467fc2d9e9))
* **api:** revert correct errors to api ([#1806](https://github.com/matter-labs/zksync-2-dev/issues/1806)) ([f3b1a6b](https://github.com/matter-labs/zksync-2-dev/commit/f3b1a6bc8fd977a6be0b5ad01d7e0dfcd71e05ba))
* **external node:** Fetch L1 gas price from the main node ([#1796](https://github.com/matter-labs/zksync-2-dev/issues/1796)) ([9b0b771](https://github.com/matter-labs/zksync-2-dev/commit/9b0b771095c78d4b3a3572d75abc1e93d0334ee3))
* **external node:** Reorg detector ([#1747](https://github.com/matter-labs/zksync-2-dev/issues/1747)) ([c3f9b71](https://github.com/matter-labs/zksync-2-dev/commit/c3f9b71d0ed85c2a45ca225de1887e10695b01a1))
* **merkle tree:** Allow using old / new tree based on config ([#1776](https://github.com/matter-labs/zksync-2-dev/issues/1776)) ([78117b8](https://github.com/matter-labs/zksync-2-dev/commit/78117b8b3c1fadcd9ba9d6d4a017fa6d3ba5517d))
* **merkle tree:** Verify tree consistency ([#1795](https://github.com/matter-labs/zksync-2-dev/issues/1795)) ([d590b3f](https://github.com/matter-labs/zksync-2-dev/commit/d590b3f0965a23eb0011779aab829d86d4fdc1d1))
* **wintess-generator:** create dedicated witness-generator binary for new prover ([#1781](https://github.com/matter-labs/zksync-2-dev/issues/1781)) ([83d45b8](https://github.com/matter-labs/zksync-2-dev/commit/83d45b8d29618c9f96e34ba139c45f5cd18f6585))


### Bug Fixes

* **api:** waffle incompatibilities ([#1730](https://github.com/matter-labs/zksync-2-dev/issues/1730)) ([910bb9b](https://github.com/matter-labs/zksync-2-dev/commit/910bb9b3fd2936e2f7fc7a6c7369eaec32a968c5))
* **db:** Error returned from database: syntax error at or near ([#1794](https://github.com/matter-labs/zksync-2-dev/issues/1794)) ([611a05d](https://github.com/matter-labs/zksync-2-dev/commit/611a05de8e5633e13afce31bcce4e3940928f1ad))
* enable/disable history at compile time ([#1803](https://github.com/matter-labs/zksync-2-dev/issues/1803)) ([0720021](https://github.com/matter-labs/zksync-2-dev/commit/0720021b1c1e30c966f06532de21bde3f01fc647))
* **external node:** Reduce amount of configuration variables required for the state keeper ([#1798](https://github.com/matter-labs/zksync-2-dev/issues/1798)) ([b2e63a9](https://github.com/matter-labs/zksync-2-dev/commit/b2e63a977583a02d09f68753e3f34ed2eb375cf9))
* **merkle tree:** Remove double-tree mode from `MetadataCalculator` ([#1801](https://github.com/matter-labs/zksync-2-dev/issues/1801)) ([fca05b9](https://github.com/matter-labs/zksync-2-dev/commit/fca05b91de56ebe992a112b907b5782f77f32d16))
* Optimize vm memory ([#1797](https://github.com/matter-labs/zksync-2-dev/issues/1797)) ([4d78e54](https://github.com/matter-labs/zksync-2-dev/commit/4d78e5404227c61d52e963bf68dd54682b4e5190))

## [4.3.0](https://github.com/matter-labs/zksync-2-dev/compare/v4.2.0...v4.3.0) (2023-05-01)


### Features

* **contract-verifier:** support metadata.bytecodeHash=none ([#1785](https://github.com/matter-labs/zksync-2-dev/issues/1785)) ([c11b7f1](https://github.com/matter-labs/zksync-2-dev/commit/c11b7f10abe105ba7c7698a422a07300df74b079))
* **db-storage-provider:** abstract db storage provide into a sharable lib ([#1775](https://github.com/matter-labs/zksync-2-dev/issues/1775)) ([2b76b66](https://github.com/matter-labs/zksync-2-dev/commit/2b76b66580d02d70e512eeb74e89102fc07a81eb))


### Bug Fixes

* **circuit:** update zkevm to prevent circuit-synthesis failures ([#1786](https://github.com/matter-labs/zksync-2-dev/issues/1786)) ([056e1c9](https://github.com/matter-labs/zksync-2-dev/commit/056e1c9ef449fb48a595895cf99ad92a43b87a47))
* Sync DAL and SQLX ([#1777](https://github.com/matter-labs/zksync-2-dev/issues/1777)) ([06d2903](https://github.com/matter-labs/zksync-2-dev/commit/06d2903af9453d6eb1250100f7de76344416d50b))
* **vm:** get_used_contracts vm method ([#1783](https://github.com/matter-labs/zksync-2-dev/issues/1783)) ([d2911de](https://github.com/matter-labs/zksync-2-dev/commit/d2911de0038a9bbae72ffd4507a1202d9c17b7ab))

## [4.2.0](https://github.com/matter-labs/zksync-2-dev/compare/v4.1.0...v4.2.0) (2023-04-27)


### Features

* **contract-verifier:** add zksolc v1.3.10 ([#1754](https://github.com/matter-labs/zksync-2-dev/issues/1754)) ([f6dd7fe](https://github.com/matter-labs/zksync-2-dev/commit/f6dd7fe31b42b6304478c45481e40bbf9f59fdbb))
* **external node:** Implement the eth_syncing method ([#1761](https://github.com/matter-labs/zksync-2-dev/issues/1761)) ([4432611](https://github.com/matter-labs/zksync-2-dev/commit/44326111c5edea227114fa723285004896cef4ac))
* **merkle tree:** Initial tree implementation ([#1735](https://github.com/matter-labs/zksync-2-dev/issues/1735)) ([edd48fc](https://github.com/matter-labs/zksync-2-dev/commit/edd48fc37bdd58f9f9d85e27d684c01ef2cac8ae))
* **object-store:** Add retires in object-store ([#1734](https://github.com/matter-labs/zksync-2-dev/issues/1734)) ([2306300](https://github.com/matter-labs/zksync-2-dev/commit/2306300249506d5a9995dfe8acf8b9951907ee3b))


### Bug Fixes

* **external node:** Fetch base system contracts from the main node ([#1675](https://github.com/matter-labs/zksync-2-dev/issues/1675)) ([eaa8637](https://github.com/matter-labs/zksync-2-dev/commit/eaa86378bd3b6a6cd2b64dcdb4e1a6c585244d0c))
* **integration-tests:** Fix bugs in our integration tests ([#1758](https://github.com/matter-labs/zksync-2-dev/issues/1758)) ([6914170](https://github.com/matter-labs/zksync-2-dev/commit/691417004f768462b874c20a79f4605e4e327eab))
* Make the DAL interface fully blocking ([#1755](https://github.com/matter-labs/zksync-2-dev/issues/1755)) ([7403c7c](https://github.com/matter-labs/zksync-2-dev/commit/7403c7cf278b71f3720967c509cb197f11b68e05))
* **state-keeper:** remove storage_logs_dedup table ([#1741](https://github.com/matter-labs/zksync-2-dev/issues/1741)) ([0d85310](https://github.com/matter-labs/zksync-2-dev/commit/0d85310adf70f35d1ccb999ff6ffe46c2a2ae0ce))
* Track `wait_for_prev_hash_time` metric in mempool (state-keeper) ([#1757](https://github.com/matter-labs/zksync-2-dev/issues/1757)) ([107ebbe](https://github.com/matter-labs/zksync-2-dev/commit/107ebbe7e6a2fa527be94da0c83404d75f3df356))
* **vm:** fix overflows originating from ceil_div ([#1743](https://github.com/matter-labs/zksync-2-dev/issues/1743)) ([a39a1c9](https://github.com/matter-labs/zksync-2-dev/commit/a39a1c94d256d42cd4d1e8ee37665772d993b9f7))

## [4.1.0](https://github.com/matter-labs/zksync-2-dev/compare/v4.0.0...v4.1.0) (2023-04-25)


### Features

* **api:** store cache between binary search iterations ([#1742](https://github.com/matter-labs/zksync-2-dev/issues/1742)) ([c0d2afa](https://github.com/matter-labs/zksync-2-dev/commit/c0d2afad7d2e33e559e4474cce947ae3ad4cd2d7))
* **contract-verifier:** add zksolc v1.3.9 ([#1732](https://github.com/matter-labs/zksync-2-dev/issues/1732)) ([880d19a](https://github.com/matter-labs/zksync-2-dev/commit/880d19a88f9edd4b1293a65fe83026b64c4a1a5f))
* **external node:** Spawn healthcheck server ([#1728](https://github.com/matter-labs/zksync-2-dev/issues/1728)) ([c092590](https://github.com/matter-labs/zksync-2-dev/commit/c0925908bfe0b116c115659467077010972f9c8e))
* **vm:** Correctly count storage invocations ([#1725](https://github.com/matter-labs/zksync-2-dev/issues/1725)) ([108a8f5](https://github.com/matter-labs/zksync-2-dev/commit/108a8f57d17f55012c7afd2dd02eb25bbd72eef2))
* **vm:** make vm history optional ([#1717](https://github.com/matter-labs/zksync-2-dev/issues/1717)) ([b61452e](https://github.com/matter-labs/zksync-2-dev/commit/b61452e51689ae5e6809817a45685ad1bcc31064))
* **vm:** Trace transaction calls ([#1556](https://github.com/matter-labs/zksync-2-dev/issues/1556)) ([e520e46](https://github.com/matter-labs/zksync-2-dev/commit/e520e4610277ba838c2ed3cbb21f8e890b44c5d7))


### Bug Fixes

* add coeficient to gas limit + method for full fee estimation ([#1622](https://github.com/matter-labs/zksync-2-dev/issues/1622)) ([229cda9](https://github.com/matter-labs/zksync-2-dev/commit/229cda977daa11a98a97515a2f75d709e2e8ed9a))
* **db:** Add index on events (address, miniblock_number, event_index_in_block) ([#1727](https://github.com/matter-labs/zksync-2-dev/issues/1727)) ([6f15141](https://github.com/matter-labs/zksync-2-dev/commit/6f15141c67e20f764c3f84dc17152df7b2e7887a))
* **explorer-api:** filter out fictive transactions and fix mint/burn events deduplication ([#1724](https://github.com/matter-labs/zksync-2-dev/issues/1724)) ([cd2376b](https://github.com/matter-labs/zksync-2-dev/commit/cd2376b0c37cde5eb8c0ee7db8ae9981052b88ed))
* **external node:** Use unique connection pools for critical components ([#1736](https://github.com/matter-labs/zksync-2-dev/issues/1736)) ([9e1b817](https://github.com/matter-labs/zksync-2-dev/commit/9e1b817da59c7201602fc463f3cfa1dc50a3c304))
* **tree:** do not decrease leaf index for non existing leaf ([#1731](https://github.com/matter-labs/zksync-2-dev/issues/1731)) ([3c8918e](https://github.com/matter-labs/zksync-2-dev/commit/3c8918eecb8151e94c810582101e99d8929a6e7a))

## [4.0.0](https://github.com/matter-labs/zksync-2-dev/compare/v3.9.1...v4.0.0) (2023-04-20)


### ⚠ BREAKING CHANGES

* Implement WETH bridge, support custom bridge in sdk, bootloader gas calculation fix  ([#1633](https://github.com/matter-labs/zksync-2-dev/issues/1633))

### Features

* Implement WETH bridge, support custom bridge in sdk, bootloader gas calculation fix  ([#1633](https://github.com/matter-labs/zksync-2-dev/issues/1633)) ([eb67ec5](https://github.com/matter-labs/zksync-2-dev/commit/eb67ec555bc027137d80122873cd12a93f9234c6))


### Bug Fixes

* **external node:** Get timestamp after applying pending miniblocks from IO ([#1722](https://github.com/matter-labs/zksync-2-dev/issues/1722)) ([875921a](https://github.com/matter-labs/zksync-2-dev/commit/875921a3462807aae53ef4cb8e15564d7015e7fa))
* Use stronger server kill for fee projection test ([#1701](https://github.com/matter-labs/zksync-2-dev/issues/1701)) ([d5e65b2](https://github.com/matter-labs/zksync-2-dev/commit/d5e65b234bd904f34c74f959aee10d2f4ad4156e))

## [3.9.1](https://github.com/matter-labs/zksync-2-dev/compare/v3.9.0...v3.9.1) (2023-04-18)


### Bug Fixes

* **vm:** small import refactor ([cfca479](https://github.com/matter-labs/zksync-2-dev/commit/cfca4794620f19911773ccc5276bcb07170a5aab))

## [3.9.0](https://github.com/matter-labs/zksync-2-dev/compare/v3.8.0...v3.9.0) (2023-04-18)


### Features

* **api servers:** panic when a transaction execution results in too many storage accesses ([#1718](https://github.com/matter-labs/zksync-2-dev/issues/1718)) ([fb910fe](https://github.com/matter-labs/zksync-2-dev/commit/fb910fe5ba07fcd02bec1a7a9379806e07d7b3d3))
* **house-keeper:** move polling interval to config ([#1684](https://github.com/matter-labs/zksync-2-dev/issues/1684)) ([49c7ff3](https://github.com/matter-labs/zksync-2-dev/commit/49c7ff360a7b70054f88a48f11776e25bd1980ff))
* **prover:** allow region+zone to be overridden for non-gcp env ([#1715](https://github.com/matter-labs/zksync-2-dev/issues/1715)) ([f1df9b0](https://github.com/matter-labs/zksync-2-dev/commit/f1df9b072eb7ef1d5d55748b9baca11bb361ef04))


### Bug Fixes

* add custom buckets for db/vm ratio ([#1707](https://github.com/matter-labs/zksync-2-dev/issues/1707)) ([811d3ad](https://github.com/matter-labs/zksync-2-dev/commit/811d3adbe834edb745e75bbb196074fc72303f5f))
* **api:** override `max_priority_fee` when estimating ([#1708](https://github.com/matter-labs/zksync-2-dev/issues/1708)) ([14830f2](https://github.com/matter-labs/zksync-2-dev/commit/14830f2198f9b81e5465f2d695f37ef0dfd78679))
* **eth-sender:** resend all txs ([#1710](https://github.com/matter-labs/zksync-2-dev/issues/1710)) ([cb20109](https://github.com/matter-labs/zksync-2-dev/commit/cb20109ea5bebd2bbd7142c3f87890a08ff9ae59))
* update @matterlabs/hardhat-zksync-solc to 3.15 ([#1713](https://github.com/matter-labs/zksync-2-dev/issues/1713)) ([e3fa879](https://github.com/matter-labs/zksync-2-dev/commit/e3fa879ed0dbbd9b9d515c9c413993d6e94106f5))
* **vm:** fix deduplicating factory deps ([#1709](https://github.com/matter-labs/zksync-2-dev/issues/1709)) ([a05cf7e](https://github.com/matter-labs/zksync-2-dev/commit/a05cf7ea2732899bfe3734004b502850a9137a00))
* **vm:** underflow in tests ([#1685](https://github.com/matter-labs/zksync-2-dev/issues/1685)) ([1bac564](https://github.com/matter-labs/zksync-2-dev/commit/1bac56427ebc6473a7dc40bae3e05d3fd56b1dac))

## [3.8.0](https://github.com/matter-labs/zksync-2-dev/compare/v3.7.2...v3.8.0) (2023-04-17)


### Features

* **object-store:** support loading credentials from file ([#1674](https://github.com/matter-labs/zksync-2-dev/issues/1674)) ([4f82574](https://github.com/matter-labs/zksync-2-dev/commit/4f825746a70423b935b79ef6227683cb2afdb63f))


### Bug Fixes

* **contract-verifier:** fix input deserialization ([#1704](https://github.com/matter-labs/zksync-2-dev/issues/1704)) ([c390e5f](https://github.com/matter-labs/zksync-2-dev/commit/c390e5f0e99fd54b21f762f609aa81451598a219))
* **contract-verifier:** parse isSystem setting ([#1686](https://github.com/matter-labs/zksync-2-dev/issues/1686)) ([a8d0e99](https://github.com/matter-labs/zksync-2-dev/commit/a8d0e990e0651a647bcde28051f80552ec662613))
* **tracking:** remove unused import ([adf4e4b](https://github.com/matter-labs/zksync-2-dev/commit/adf4e4b36f4831c69664dd4902a47b7e7c3bc1e5))

## [3.7.2](https://github.com/matter-labs/zksync-2-dev/compare/v3.7.1...v3.7.2) (2023-04-16)


### Bug Fixes

* **logging:** add more logging when saving events in the DB ([85212e6](https://github.com/matter-labs/zksync-2-dev/commit/85212e6210b80a3b1d4e25528dd7c15d03a5e652))
* **logging:** add more logging when saving events in the DB ([b9cb0fa](https://github.com/matter-labs/zksync-2-dev/commit/b9cb0fa8fa1b1e71625d2754211b16a5f012ba3e))
* **logging:** add more logging when saving events in the DB ([0deac3d](https://github.com/matter-labs/zksync-2-dev/commit/0deac3d84d8de085f1fd3d7886ab137a5e9004a2))
* **logging:** add more logging when saving events in the DB ([d330096](https://github.com/matter-labs/zksync-2-dev/commit/d330096f2f35f3b173eb59981cb496d2f654d8e5))

## [3.7.1](https://github.com/matter-labs/zksync-2-dev/compare/v3.7.0...v3.7.1) (2023-04-15)


### Bug Fixes

* **metrics:** item count tracking in state keeper ([#1696](https://github.com/matter-labs/zksync-2-dev/issues/1696)) ([8d7c8d8](https://github.com/matter-labs/zksync-2-dev/commit/8d7c8d889bfc7b4469699f7fb17be65baaf407c4))

## [3.7.0](https://github.com/matter-labs/zksync-2-dev/compare/v3.6.0...v3.7.0) (2023-04-14)


### Features

* add getL1BatchDetails method to js SDK ([#1666](https://github.com/matter-labs/zksync-2-dev/issues/1666)) ([babb8a9](https://github.com/matter-labs/zksync-2-dev/commit/babb8a94466a8f8c81a19391d61aa9ea66f9cfa8))
* **external node:** consistency checker ([#1658](https://github.com/matter-labs/zksync-2-dev/issues/1658)) ([e0d65ef](https://github.com/matter-labs/zksync-2-dev/commit/e0d65ef6604685c8a6213d466a575bc41f8bfe45))
* **healtcheck:** Add new server with healthcheck for all components ([#1667](https://github.com/matter-labs/zksync-2-dev/issues/1667)) ([5f00e5c](https://github.com/matter-labs/zksync-2-dev/commit/5f00e5c4d55f7783480350138d79c7275ecf531c))
* **sdk:** extend BlockDetails type to include l1BatchNumber ([#1677](https://github.com/matter-labs/zksync-2-dev/issues/1677)) ([67acf90](https://github.com/matter-labs/zksync-2-dev/commit/67acf90301e401004d41361b43f2d3336a48676e))
* **state-keeper:** add metrics for how long we wait for a tx  ([#1680](https://github.com/matter-labs/zksync-2-dev/issues/1680)) ([c8b4447](https://github.com/matter-labs/zksync-2-dev/commit/c8b4447cc67e426ca184391d3da11e3d648910ce))
* **state-keeper:** track number of rows when saving blocks to the DB ([#1682](https://github.com/matter-labs/zksync-2-dev/issues/1682)) ([b6f306b](https://github.com/matter-labs/zksync-2-dev/commit/b6f306b97e8e13ef4f80eb657a09ed4389efdb7e))
* **VM:** track time spent on VM storage access ([#1687](https://github.com/matter-labs/zksync-2-dev/issues/1687)) ([9b645be](https://github.com/matter-labs/zksync-2-dev/commit/9b645beacfabc6478c67581fcf4f00d0c2a08516))
* **witness-generator:** split witness-generator into individual components ([#1623](https://github.com/matter-labs/zksync-2-dev/issues/1623)) ([82724e1](https://github.com/matter-labs/zksync-2-dev/commit/82724e1d6db16725684351c184e24f7b767a69f4))


### Bug Fixes

* (logging) total time spent accessing storage = get + set ([#1689](https://github.com/matter-labs/zksync-2-dev/issues/1689)) ([49a3a9b](https://github.com/matter-labs/zksync-2-dev/commit/49a3a9bd3aa25317cfa745f35802f235045864d9))
* **api:** fix `max_fee_per_gas` estimation ([#1671](https://github.com/matter-labs/zksync-2-dev/issues/1671)) ([aed3112](https://github.com/matter-labs/zksync-2-dev/commit/aed3112d63ec4306f98ccfe20841e9cf298bccd1))
* **circuit breaker:** add retries for http-call functions ([#1541](https://github.com/matter-labs/zksync-2-dev/issues/1541)) ([a316446](https://github.com/matter-labs/zksync-2-dev/commit/a316446d6f959198a5ccee8698a549a597e4e716))
* **external node:** Misc external node fixes ([#1673](https://github.com/matter-labs/zksync-2-dev/issues/1673)) ([da9ea17](https://github.com/matter-labs/zksync-2-dev/commit/da9ea172c0813e19c3be6c78166ff012f087ea97))
* **loadtest:** override EIP1559 fields ([#1683](https://github.com/matter-labs/zksync-2-dev/issues/1683)) ([6c3eeb3](https://github.com/matter-labs/zksync-2-dev/commit/6c3eeb38ef9485473f1eb1fa428cf163a07c8e62))
* **loadtest:** update max nonce ahead ([#1668](https://github.com/matter-labs/zksync-2-dev/issues/1668)) ([c5eac45](https://github.com/matter-labs/zksync-2-dev/commit/c5eac45791fba65613c903f06c36d83ce9c1b8b7))
* **metrics:** minor changes to metrics collection ([#1664](https://github.com/matter-labs/zksync-2-dev/issues/1664)) ([5ba5f3b](https://github.com/matter-labs/zksync-2-dev/commit/5ba5f3b180c1373f2c3274e997496ed0d3125394))
* **prover-query:** added waiting_to_queued_witness_job_mover ([#1640](https://github.com/matter-labs/zksync-2-dev/issues/1640)) ([dbacac1](https://github.com/matter-labs/zksync-2-dev/commit/dbacac194a1c5961b372b7e316f7ca9e2cc17495))

## [3.6.0](https://github.com/matter-labs/zksync-2-dev/compare/v3.5.0...v3.6.0) (2023-04-10)


### Features

* **contract-verifier:** support optimization mode ([#1661](https://github.com/matter-labs/zksync-2-dev/issues/1661)) ([3bb85b9](https://github.com/matter-labs/zksync-2-dev/commit/3bb85b95ec2125bc0bad584d5f89612013aba955))

## [3.5.0](https://github.com/matter-labs/zksync-2-dev/compare/v3.4.2...v3.5.0) (2023-04-10)


### Features

* **eth-sender:** abstract max_acceptable_priority_fee in config ([#1651](https://github.com/matter-labs/zksync-2-dev/issues/1651)) ([17c75b2](https://github.com/matter-labs/zksync-2-dev/commit/17c75b291d696545718fe896cbd74276e0a2c148))
* **witness-generator:** emit metrics for each witness-generator type ([#1650](https://github.com/matter-labs/zksync-2-dev/issues/1650)) ([6d72e67](https://github.com/matter-labs/zksync-2-dev/commit/6d72e67994ae90979fc58c9406cd318bb4e75348))


### Bug Fixes

* **external node:** docker workflow & foreign key constraint bug ([#1656](https://github.com/matter-labs/zksync-2-dev/issues/1656)) ([2944a00](https://github.com/matter-labs/zksync-2-dev/commit/2944a004a38b71f44d2f5617c9a9945853659d46))
* **logging:** downgrade non-essential logs to trace level ([#1654](https://github.com/matter-labs/zksync-2-dev/issues/1654)) ([f325995](https://github.com/matter-labs/zksync-2-dev/commit/f3259953d0d5366d75bbdeb840e660861d6eb86a))
* **prover:** make prover-related jobs run less frequently ([#1647](https://github.com/matter-labs/zksync-2-dev/issues/1647)) ([cb47511](https://github.com/matter-labs/zksync-2-dev/commit/cb475116f5f729798e1dbdb95a99872f5867403b))
* **state-keeper:** Do not reject tx if bootloader has not enough gas  ([#1657](https://github.com/matter-labs/zksync-2-dev/issues/1657)) ([6bce00d](https://github.com/matter-labs/zksync-2-dev/commit/6bce00d44009323114a4d9d7030a2a318e49f82c))

## [3.4.2](https://github.com/matter-labs/zksync-2-dev/compare/v3.4.1...v3.4.2) (2023-04-07)


### Bug Fixes

* **api:** use verify-execute mode in `submit_tx` ([#1653](https://github.com/matter-labs/zksync-2-dev/issues/1653)) ([3ed98e2](https://github.com/matter-labs/zksync-2-dev/commit/3ed98e2ca65685aa6087304d57cd2c8eae3a8745))
* **external node:** Read base system contracts from DB instead of disk ([#1642](https://github.com/matter-labs/zksync-2-dev/issues/1642)) ([865c9c6](https://github.com/matter-labs/zksync-2-dev/commit/865c9c64767d10661d769ffeeddda83e60bf3273))
* **object_store:** handle other 404 from crate other than HttpClient … ([#1643](https://github.com/matter-labs/zksync-2-dev/issues/1643)) ([a01f0b2](https://github.com/matter-labs/zksync-2-dev/commit/a01f0b2ec8426d6d009ab40f45ceff5f9f0346ef))

## [3.4.1](https://github.com/matter-labs/zksync-2-dev/compare/v3.4.0...v3.4.1) (2023-04-06)


### Bug Fixes

* **prover-queries:** add prover_job_retry_manager component ([#1637](https://github.com/matter-labs/zksync-2-dev/issues/1637)) ([9c0258a](https://github.com/matter-labs/zksync-2-dev/commit/9c0258a3ae178f10a99ccceb5c984079ab055139))

## [3.4.0](https://github.com/matter-labs/zksync-2-dev/compare/v3.3.1...v3.4.0) (2023-04-05)


### Features

* **contract_verifier:** add zksolc v1.3.8 ([#1630](https://github.com/matter-labs/zksync-2-dev/issues/1630)) ([1575d12](https://github.com/matter-labs/zksync-2-dev/commit/1575d1280f9160ba21acba30ba985c6b643e12c7))
* **external node:** External Node Alpha ([#1614](https://github.com/matter-labs/zksync-2-dev/issues/1614)) ([6304567](https://github.com/matter-labs/zksync-2-dev/commit/6304567285c64dcf129fd7ee0630d219564d969a))
* **state keeper:** computational gas criterion ([#1542](https://github.com/matter-labs/zksync-2-dev/issues/1542)) ([e96a424](https://github.com/matter-labs/zksync-2-dev/commit/e96a424fa594e45b59744b6b74f7f7737bf1ef00))


### Bug Fixes

* **api:** dont bind block number in get_logs ([#1632](https://github.com/matter-labs/zksync-2-dev/issues/1632)) ([7adbbab](https://github.com/matter-labs/zksync-2-dev/commit/7adbbabd582925cf6e0a21f9d5064641ae95d7d6))
* **api:** remove explicit number cast in DB query ([#1621](https://github.com/matter-labs/zksync-2-dev/issues/1621)) ([e4ec312](https://github.com/matter-labs/zksync-2-dev/commit/e4ec31261f75265bfb3d954258bcd602917a5a8d))
* **prover:** fix backoff calculation ([#1629](https://github.com/matter-labs/zksync-2-dev/issues/1629)) ([1b89646](https://github.com/matter-labs/zksync-2-dev/commit/1b89646ae324e69e415eaf38d41ace57dc76551c))
* **state_keeper:** deduplicate factory deps before compressing ([#1620](https://github.com/matter-labs/zksync-2-dev/issues/1620)) ([35719d1](https://github.com/matter-labs/zksync-2-dev/commit/35719d1fef150321a30c9e94d65f938f551a5850))

## [3.3.1](https://github.com/matter-labs/zksync-2-dev/compare/v3.3.0...v3.3.1) (2023-04-04)


### Bug Fixes

* **queued-job-processor:** add exponential back-offs while polling jobs ([#1625](https://github.com/matter-labs/zksync-2-dev/issues/1625)) ([80c6096](https://github.com/matter-labs/zksync-2-dev/commit/80c60960b9901f7427bb002699a3aabc341f2664))

## [3.3.0](https://github.com/matter-labs/zksync-2-dev/compare/v3.2.2...v3.3.0) (2023-04-04)


### Features

* **contract-verifier:** support verification of force deployed contracts ([#1611](https://github.com/matter-labs/zksync-2-dev/issues/1611)) ([be37e09](https://github.com/matter-labs/zksync-2-dev/commit/be37e0951a8eb9e37ea4aba3c4bfaa0ba90ac208))

## [3.2.2](https://github.com/matter-labs/zksync-2-dev/compare/v3.2.1...v3.2.2) (2023-04-02)


### Bug Fixes

* **explorer-api:** Improve finalized block query ([#1618](https://github.com/matter-labs/zksync-2-dev/issues/1618)) ([c9e0fbc](https://github.com/matter-labs/zksync-2-dev/commit/c9e0fbca2191a4b0886e42f779a3e1d629071633))

## [3.2.1](https://github.com/matter-labs/zksync-2-dev/compare/v3.2.0...v3.2.1) (2023-04-01)


### Bug Fixes

* **prover:** increase polling interval in job processors ([2f00e64](https://github.com/matter-labs/zksync-2-dev/commit/2f00e64198f2e728933bac810e29cf8545815e6c))

## [3.2.0](https://github.com/matter-labs/zksync-2-dev/compare/v3.1.0...v3.2.0) (2023-04-01)


### Features

* **external node:** Prepare the execution layer ([#1594](https://github.com/matter-labs/zksync-2-dev/issues/1594)) ([143a112](https://github.com/matter-labs/zksync-2-dev/commit/143a1122d86592601e24a3b2f71cdc4ab3f85d2b))
* **tracking:** track individual circuit block height ([#1613](https://github.com/matter-labs/zksync-2-dev/issues/1613)) ([71a302e](https://github.com/matter-labs/zksync-2-dev/commit/71a302e34319ccadb008a04aaa243ce96ac97eb4))


### Bug Fixes

* **prover:** get rid of exclusive lock ([#1616](https://github.com/matter-labs/zksync-2-dev/issues/1616)) ([3e7443d](https://github.com/matter-labs/zksync-2-dev/commit/3e7443d88415444e424f8cea8bd929c4b4f0c2e5))

## [3.1.0](https://github.com/matter-labs/zksync-2-dev/compare/v3.0.8...v3.1.0) (2023-03-29)


### Features

* **api:** implement health check for jsonrpc ([#1605](https://github.com/matter-labs/zksync-2-dev/issues/1605)) ([267c497](https://github.com/matter-labs/zksync-2-dev/commit/267c49708df9f708a93bc69a8a9f0094b6f97a67))
* **prover-multizone:** Added support for running prover in multi-zone ([#1577](https://github.com/matter-labs/zksync-2-dev/issues/1577)) ([629f63b](https://github.com/matter-labs/zksync-2-dev/commit/629f63b07118c8a17a653c62b5ef3cd4bdfcaaa4))
* **VM:** Update zk evm ([#1609](https://github.com/matter-labs/zksync-2-dev/issues/1609)) ([643187a](https://github.com/matter-labs/zksync-2-dev/commit/643187ab3e03ca540ce7a01eaddddf459a79dd40))


### Bug Fixes

* **api-error:** handle empty CannotEstimateGas ([#1606](https://github.com/matter-labs/zksync-2-dev/issues/1606)) ([135e420](https://github.com/matter-labs/zksync-2-dev/commit/135e420e1d1956a11999f465428f9349f73e5581))
* **api-error:** rename submit tx error from can't estimate tx to gas ([#1548](https://github.com/matter-labs/zksync-2-dev/issues/1548)) ([9a4cbc1](https://github.com/matter-labs/zksync-2-dev/commit/9a4cbc16032a1739820187ff07ca0d1dedef02a0))
* **eth_sender:** do not save identical eth_txs_history rows ([#1603](https://github.com/matter-labs/zksync-2-dev/issues/1603)) ([13f01de](https://github.com/matter-labs/zksync-2-dev/commit/13f01de846a08f35aa2144bc130f0a84c1626d40))
* **eth-sender:** Use transaction in confirm_tx method ([#1604](https://github.com/matter-labs/zksync-2-dev/issues/1604)) ([05cffbe](https://github.com/matter-labs/zksync-2-dev/commit/05cffbedd87042707620c86dddecd98eb2337925))
* **metrics:** fix server.prover.jobs metrics ([#1608](https://github.com/matter-labs/zksync-2-dev/issues/1608)) ([9f351e8](https://github.com/matter-labs/zksync-2-dev/commit/9f351e842ec6178be8d0b1c0b40797ca565319c8))
* **synthesizer:** update filtering to include region zone ([#1607](https://github.com/matter-labs/zksync-2-dev/issues/1607)) ([12d40b9](https://github.com/matter-labs/zksync-2-dev/commit/12d40b91f5f99b44270aec3e00ed0c0f5fe9adb9))

## [3.0.8](https://github.com/matter-labs/zksync-2-dev/compare/v3.0.7...v3.0.8) (2023-03-27)


### Bug Fixes

* **explorer_api:** total_transactions stats ([#1595](https://github.com/matter-labs/zksync-2-dev/issues/1595)) ([824e4f7](https://github.com/matter-labs/zksync-2-dev/commit/824e4f74beedd1b86bf5134f27ab22c2309ef2f0))
* **witness-generator:** update test-harness to fix circuit-synthesis failure ([#1596](https://github.com/matter-labs/zksync-2-dev/issues/1596)) ([7453822](https://github.com/matter-labs/zksync-2-dev/commit/74538225ca45dea134acdd8f8f2540dc5a1d64c4))

## [3.0.0](https://github.com/matter-labs/zksync-2-dev/compare/v2.11.1...v3.0.0) (2023-03-22)


### ⚠ BREAKING CHANGES

* **contracts:** M6 batch of breaking changes ([#1482](https://github.com/matter-labs/zksync-2-dev/issues/1482))

### Features

* **contracts:** M6 batch of breaking changes ([#1482](https://github.com/matter-labs/zksync-2-dev/issues/1482)) ([d28e01c](https://github.com/matter-labs/zksync-2-dev/commit/d28e01ce0fbf0129c2cbba877efe65da7f7ed367))
* env var for state keeper to finish l1 batch and stop ([#1538](https://github.com/matter-labs/zksync-2-dev/issues/1538)) ([eaa0cce](https://github.com/matter-labs/zksync-2-dev/commit/eaa0cce81e683bd10b1c85b06bc04a7de578e02e))
* **external node:** Implement transaction proxy ([#1534](https://github.com/matter-labs/zksync-2-dev/issues/1534)) ([19b6a85](https://github.com/matter-labs/zksync-2-dev/commit/19b6a8595e5e8e8399bacf6e2308e553d567a2b5))
* **external node:** Sync layer implementation ([#1525](https://github.com/matter-labs/zksync-2-dev/issues/1525)) ([47b9a1d](https://github.com/matter-labs/zksync-2-dev/commit/47b9a1d30cc87f7128ef29eb5d0851276d71b7d1))
* **prover-generalized:** added a generalized prover-group for integration test ([#1526](https://github.com/matter-labs/zksync-2-dev/issues/1526)) ([f921886](https://github.com/matter-labs/zksync-2-dev/commit/f9218866cd790975b7f97be6a4a59192a1da8b3a))
* **vm:** vm memory metrics ([#1564](https://github.com/matter-labs/zksync-2-dev/issues/1564)) ([ee45d47](https://github.com/matter-labs/zksync-2-dev/commit/ee45d477e6c393277923bfc64226ea03290a01a0))


### Bug Fixes

* **witness-generator:** Fix witness generation for storage application circuit ([#1568](https://github.com/matter-labs/zksync-2-dev/issues/1568)) ([5268ac4](https://github.com/matter-labs/zksync-2-dev/commit/5268ac4558aea7c2ac72bdfc6c57afd25eff1e8c))


### Reverts

* env var for state keeper to finish l1 batch and stop ([#1545](https://github.com/matter-labs/zksync-2-dev/issues/1545)) ([94701bd](https://github.com/matter-labs/zksync-2-dev/commit/94701bd2fbc590f733346934cfbccae08fc62f1a))

## [2.11.1](https://github.com/matter-labs/zksync-2-dev/compare/v2.11.0...v2.11.1) (2023-03-16)


### Bug Fixes

* **witness-generator:** perform sampling only for basic circuit ([#1535](https://github.com/matter-labs/zksync-2-dev/issues/1535)) ([76c3248](https://github.com/matter-labs/zksync-2-dev/commit/76c324883dd7b5026f01add61bef637b2e1c0c5b))

## [2.11.0](https://github.com/matter-labs/zksync-2-dev/compare/v2.10.0...v2.11.0) (2023-03-15)


### Features

* Make server compatible with new SDK ([#1532](https://github.com/matter-labs/zksync-2-dev/issues/1532)) ([1c52738](https://github.com/matter-labs/zksync-2-dev/commit/1c527382d1e36c04df90bdf71fe643db724acb48))

## [2.10.0](https://github.com/matter-labs/zksync-2-dev/compare/v2.9.0...v2.10.0) (2023-03-14)


### Features

* **explorer api:** L1 batch endpoints ([#1529](https://github.com/matter-labs/zksync-2-dev/issues/1529)) ([f06c95d](https://github.com/matter-labs/zksync-2-dev/commit/f06c95defd79aaea24a3f317236fac537dee63c5))
* **simpler-sampling:** simplify witness-generator sampling using proof % ([#1514](https://github.com/matter-labs/zksync-2-dev/issues/1514)) ([b4378ac](https://github.com/matter-labs/zksync-2-dev/commit/b4378ac2524f2ca936ee5d53351c7596526ea714))
* **vm:** limit validation gas ([#1513](https://github.com/matter-labs/zksync-2-dev/issues/1513)) ([09c9afa](https://github.com/matter-labs/zksync-2-dev/commit/09c9afaf0ebe11c513c6779b7c585e75fde80e09))
* **workload identity support:** Refactor GCS to add workload identity support ([#1503](https://github.com/matter-labs/zksync-2-dev/issues/1503)) ([1880931](https://github.com/matter-labs/zksync-2-dev/commit/188093185241180c54e4edcbc95fb068d890c0e5))


### Bug Fixes

* **circuit-upgrade:** upgrade circuit to fix synthesizer issue ([#1530](https://github.com/matter-labs/zksync-2-dev/issues/1530)) ([368eeb5](https://github.com/matter-labs/zksync-2-dev/commit/368eeb58b027a3b2c7fe6491d3d17306921d8265))
* **prover:** query for hanged gpu proofs ([#1522](https://github.com/matter-labs/zksync-2-dev/issues/1522)) ([3c4b597](https://github.com/matter-labs/zksync-2-dev/commit/3c4b597c2637dd6adaa77f0a52a7e7ada1d52918))
* **synthesizer-alerting:** add sentry_guard variable ([#1524](https://github.com/matter-labs/zksync-2-dev/issues/1524)) ([ced5107](https://github.com/matter-labs/zksync-2-dev/commit/ced51079665a1e64b56f1e712473be90e9a38cb1))
* **witness-generator:** update logic while persist status in db to prevent race ([#1507](https://github.com/matter-labs/zksync-2-dev/issues/1507)) ([9c295c4](https://github.com/matter-labs/zksync-2-dev/commit/9c295c42ce1e725134f1b610f32e55163e6da349))

## [2.9.0](https://github.com/matter-labs/zksync-2-dev/compare/v2.8.0...v2.9.0) (2023-03-09)


### Features

* **external node:** Sync protocol: API changes & fetcher skeleton ([#1498](https://github.com/matter-labs/zksync-2-dev/issues/1498)) ([05da6a8](https://github.com/matter-labs/zksync-2-dev/commit/05da6a857b6d9faa9ba50183272feacc12518482))
* integrate yul contracts into the server ([#1506](https://github.com/matter-labs/zksync-2-dev/issues/1506)) ([c542c29](https://github.com/matter-labs/zksync-2-dev/commit/c542c2969f72996ab874bd089f096cd123c926a4))


### Bug Fixes

* abi encoded message length ([#1516](https://github.com/matter-labs/zksync-2-dev/issues/1516)) ([65766ee](https://github.com/matter-labs/zksync-2-dev/commit/65766ee12fb6ab27382c378334dc7176dc233d26))
* **state-keeper:** Save correct value after executing miniblock ([#1511](https://github.com/matter-labs/zksync-2-dev/issues/1511)) ([5decdda](https://github.com/matter-labs/zksync-2-dev/commit/5decdda60b8880d0ada86f402f2f270572c45601))
* **witness-generator:** increase limit from 155K to 16M while expanding bootloader ([#1515](https://github.com/matter-labs/zksync-2-dev/issues/1515)) ([05711de](https://github.com/matter-labs/zksync-2-dev/commit/05711de1317edb094cbcf375a9dc75e35662a7a7))

## [2.8.0](https://github.com/matter-labs/zksync-2-dev/compare/v2.7.15...v2.8.0) (2023-03-06)


### Features

* **api:** add Geth API errors to our codebase that are not present yet ([#1440](https://github.com/matter-labs/zksync-2-dev/issues/1440)) ([f6cefdd](https://github.com/matter-labs/zksync-2-dev/commit/f6cefdd21083301fce5fa665aa79ceb307b3cc49))
* **house-keeper:** emit prover queued jobs for each group type ([#1480](https://github.com/matter-labs/zksync-2-dev/issues/1480)) ([ab6d7c4](https://github.com/matter-labs/zksync-2-dev/commit/ab6d7c431ac64619571e227a6680f0552aa7b1ee))
* **house-keeper:** increase blob cleanup time from 2days to 30 ([8a7ee85](https://github.com/matter-labs/zksync-2-dev/commit/8a7ee8548a7c24235549f714d8668396ab05f026))
* **house-keeper:** increase blob cleanup time from 2days to 30 ([#1485](https://github.com/matter-labs/zksync-2-dev/issues/1485)) ([8a7ee85](https://github.com/matter-labs/zksync-2-dev/commit/8a7ee8548a7c24235549f714d8668396ab05f026))
* **state keeper:** precise calculation of initial/repeated writes ([#1486](https://github.com/matter-labs/zksync-2-dev/issues/1486)) ([15ae673](https://github.com/matter-labs/zksync-2-dev/commit/15ae673da09eda47566ef11ea10d7c262d44e272))
* **vm:** add a few assert to memory impl ([#1476](https://github.com/matter-labs/zksync-2-dev/issues/1476)) ([dfff514](https://github.com/matter-labs/zksync-2-dev/commit/dfff514703ef48eb7a1026f3e9f0ee4c5e9af2f6))
* **witness-generator:** added last_l1_batch_to_process param for smoo… ([#1477](https://github.com/matter-labs/zksync-2-dev/issues/1477)) ([5d46505](https://github.com/matter-labs/zksync-2-dev/commit/5d4650564799c6e7f22b5fc5cc43ae484eb7f849))


### Bug Fixes

* **api:** fix tx count query ([#1494](https://github.com/matter-labs/zksync-2-dev/issues/1494)) ([fc5c61b](https://github.com/matter-labs/zksync-2-dev/commit/fc5c61bd65772ea9d4b129a1a8e22a0ab9494aba))
* **circuits:** update circuits+vk for invalid memory access issue ([#1496](https://github.com/matter-labs/zksync-2-dev/issues/1496)) ([d84a73a](https://github.com/matter-labs/zksync-2-dev/commit/d84a73a3b54688f808be590e13fc4995666e3068))
* **db:** create index to reduce load from prover_jobs table ([#1251](https://github.com/matter-labs/zksync-2-dev/issues/1251)) ([500f03a](https://github.com/matter-labs/zksync-2-dev/commit/500f03ac753f243e6e525639bc02e28987dcc7dd))
* **gas_adjuster:** Sub 1 from the last block number for fetching  base_fee_history ([#1483](https://github.com/matter-labs/zksync-2-dev/issues/1483)) ([0af2f42](https://github.com/matter-labs/zksync-2-dev/commit/0af2f42b8c7c4635a18af01250213390c2424de9))
