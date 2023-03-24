# Changelog

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
