# Changelog

## [29.1.1](https://github.com/matter-labs/zksync-era/compare/core-v29.1.0...core-v29.1.1) (2025-08-21)


### Bug Fixes

* **transaction-finality-updater:** Handle rpc error properly ([#4414](https://github.com/matter-labs/zksync-era/issues/4414)) ([a6f0dd5](https://github.com/matter-labs/zksync-era/commit/a6f0dd5bf392ba525b25b55611f8f3f6403895c7))
* **zkstack:** Allow to use chain config if ecosystem is redundant ([#4236](https://github.com/matter-labs/zksync-era/issues/4236)) ([066b3b1](https://github.com/matter-labs/zksync-era/commit/066b3b1f901053e318b0cf8c1c315d35d1d5526f))

## [29.1.0](https://github.com/matter-labs/zksync-era/compare/core-v29.0.0...core-v29.1.0) (2025-08-15)


### Features

* add public bucket address config variable ([#4392](https://github.com/matter-labs/zksync-era/issues/4392)) ([43ef62d](https://github.com/matter-labs/zksync-era/commit/43ef62dc1337d54d4208b69a8cc3dca0db6727b4))
* **en:** Cache remote config for en ([#4367](https://github.com/matter-labs/zksync-era/issues/4367)) ([20bc4a8](https://github.com/matter-labs/zksync-era/commit/20bc4a8bee67a7896e37117493d16ad0c7de258f))
* **en:** Do not require main node to be active to start en ([#4395](https://github.com/matter-labs/zksync-era/issues/4395)) ([54f34d9](https://github.com/matter-labs/zksync-era/commit/54f34d9c2e81f8f3c92c3b63254a6fd4af61e0e8))
* Eth proof manager sender ([#4266](https://github.com/matter-labs/zksync-era/issues/4266)) ([93b2086](https://github.com/matter-labs/zksync-era/commit/93b20860dc2b1bf8671ea3187e4bcebc6913552b))
* **eth_sender:** Allow to use validator timelock from the config ([#4370](https://github.com/matter-labs/zksync-era/issues/4370)) ([2cb551b](https://github.com/matter-labs/zksync-era/commit/2cb551be575dbe214f8b72b29403070021d875f0))
* improve JSON-RPC parameter error messages ([#4390](https://github.com/matter-labs/zksync-era/issues/4390)) ([b6bf4c7](https://github.com/matter-labs/zksync-era/commit/b6bf4c7cda015f77c900dfe697cf9db87c6f8494))
* update contracts to latest v29 ([#4386](https://github.com/matter-labs/zksync-era/issues/4386)) ([f75e021](https://github.com/matter-labs/zksync-era/commit/f75e0215aaf6a2898baf2bd3dc29386c16ad4c67))


### Bug Fixes

* **contracts:** force load the contracts for sl ([#4371](https://github.com/matter-labs/zksync-era/issues/4371)) ([4751481](https://github.com/matter-labs/zksync-era/commit/4751481f732a4298f6b5f13f64e4320bf0517318))
* Copy proof manager contracts to server dockerfile ([#4385](https://github.com/matter-labs/zksync-era/issues/4385)) ([a179dee](https://github.com/matter-labs/zksync-era/commit/a179dee347680526a5a211de35a58efca4b9b40d))
* **eth_sender:** Do not fail if execution delay function is absent ([#4373](https://github.com/matter-labs/zksync-era/issues/4373)) ([bd55907](https://github.com/matter-labs/zksync-era/commit/bd55907d211634dc297844e9ebac841849937493))
* **mempool:** remove redundant connections ([#4378](https://github.com/matter-labs/zksync-era/issues/4378)) ([9ea3c37](https://github.com/matter-labs/zksync-era/commit/9ea3c374ff61f7bc0d964b7948a165edaf83ebf3))
* **state-keeper:** Do not seal based on interop roots for v28 ([#4376](https://github.com/matter-labs/zksync-era/issues/4376)) ([8c0e689](https://github.com/matter-labs/zksync-era/commit/8c0e689952215284114002387c4cca3d5ae2e48c))
* **state-keeper:** limit to max interop roots per batch ([#4363](https://github.com/matter-labs/zksync-era/issues/4363)) ([80a20d9](https://github.com/matter-labs/zksync-era/commit/80a20d967fecd25009d8c77ca34b4f996c57c0b3))

## [29.0.0](https://github.com/matter-labs/zksync-era/compare/core-v28.10.0...core-v29.0.0) (2025-07-29)


### ⚠ BREAKING CHANGES

* v29 upgrade testing & zkstack_cli changes ([#4332](https://github.com/matter-labs/zksync-era/issues/4332))

### Features

* Gateway EN Precommit sync ([#4311](https://github.com/matter-labs/zksync-era/issues/4311)) ([52dec97](https://github.com/matter-labs/zksync-era/commit/52dec977cc3e877dbc54d1f326a8e5944dd34f32))
* read execution delay from ValidatorTimelock contract instead of config ([#4349](https://github.com/matter-labs/zksync-era/issues/4349)) ([21ebe9f](https://github.com/matter-labs/zksync-era/commit/21ebe9f9626969d9e89bc0aeaa5019284aa3e6e3))
* v29 upgrade testing & zkstack_cli changes ([#4332](https://github.com/matter-labs/zksync-era/issues/4332)) ([9e4755e](https://github.com/matter-labs/zksync-era/commit/9e4755edb16328baf6f3e0632d700eb4c545eea6))


### Bug Fixes

* **api:** Return back fee params backward compatibility for non gateway chains ([#4361](https://github.com/matter-labs/zksync-era/issues/4361)) ([506822e](https://github.com/matter-labs/zksync-era/commit/506822e5d61984bfcb5d66b32cfdea077f02b7e7))
* **ci:** fix failing gateway integration test ([#4358](https://github.com/matter-labs/zksync-era/issues/4358)) ([e76c079](https://github.com/matter-labs/zksync-era/commit/e76c0797cf818ea112c4eb4c2181762bac8ffeb4))
* **en:** Properly prune the batches with finality status ([#4362](https://github.com/matter-labs/zksync-era/issues/4362)) ([7d345c3](https://github.com/matter-labs/zksync-era/commit/7d345c32ee6f206b82d97a950585c98ae9500cda))
* **eth-sender:** Check the existance of eth txs before getting the correct block for statistics ([#4359](https://github.com/matter-labs/zksync-era/issues/4359)) ([99d90e5](https://github.com/matter-labs/zksync-era/commit/99d90e5c78e2aa4b6bbe7cad62fee9f15ad16000))
* **state-keeper:** set interop roots on first block in batch ([#4324](https://github.com/matter-labs/zksync-era/issues/4324)) ([b108a21](https://github.com/matter-labs/zksync-era/commit/b108a21ad9e57a559d2640a31a2bb3e865d2c88d))

## [28.10.0](https://github.com/matter-labs/zksync-era/compare/core-v28.9.0...core-v28.10.0) (2025-07-24)


### Features

* add gas cap configuration for eth_call requests ([#4299](https://github.com/matter-labs/zksync-era/issues/4299)) ([340c9f7](https://github.com/matter-labs/zksync-era/commit/340c9f717c35051f7b98e0a3c1a4ae1e1d9d979d))
* Draft v29 ([#3960](https://github.com/matter-labs/zksync-era/issues/3960)) ([91843a2](https://github.com/matter-labs/zksync-era/commit/91843a2781768a75a59a907409f2472630c59877))
* **en:** return back JSON RPC syncing ([#4344](https://github.com/matter-labs/zksync-era/issues/4344)) ([24b2990](https://github.com/matter-labs/zksync-era/commit/24b299087711d29f5887309658f88e69323b54dc))
* Gas price conversion logic for chains settling on Gateway ([#4283](https://github.com/matter-labs/zksync-era/issues/4283)) ([b63282c](https://github.com/matter-labs/zksync-era/commit/b63282c915f3ab05b87bfd823955fb8fb96094c0))
* Handling of ZK token address for price conversion ([#4327](https://github.com/matter-labs/zksync-era/issues/4327)) ([887c894](https://github.com/matter-labs/zksync-era/commit/887c8942c9349cec7cf82cb5f3bed13cacb92dfc))
* High Priority L2 transactions ([#4334](https://github.com/matter-labs/zksync-era/issues/4334)) ([0bcc64d](https://github.com/matter-labs/zksync-era/commit/0bcc64da95a04f7ce4676dd4adfd6becf1ca4e62))
* Proof manager watcher ([#4241](https://github.com/matter-labs/zksync-era/issues/4241)) ([6423b0d](https://github.com/matter-labs/zksync-era/commit/6423b0db81ca3a75d053b25b9448a9a70b894b04))
* vm changes from draft-v29 ([#4221](https://github.com/matter-labs/zksync-era/issues/4221)) ([a80f55a](https://github.com/matter-labs/zksync-era/commit/a80f55aaaf8f2a97a897dbf2eaaa35eea2a86faf))


### Bug Fixes

* **da_fetcher:** use same logic as in consistency_checker to define a first batch to process ([#4322](https://github.com/matter-labs/zksync-era/issues/4322)) ([250e666](https://github.com/matter-labs/zksync-era/commit/250e6660dfda67e9b27f476fed97c73798fc523a))
* **en:** make interop_roots optional ([#4345](https://github.com/matter-labs/zksync-era/issues/4345)) ([97de2eb](https://github.com/matter-labs/zksync-era/commit/97de2ebf2be70bf3e9efd59b8083f32e5aa05efe))
* **ff:** do not send precommits before v29 ([#4323](https://github.com/matter-labs/zksync-era/issues/4323)) ([048753f](https://github.com/matter-labs/zksync-era/commit/048753f31f3ab8f549adafe55cf5fa409921494a))
* **integration tests:** restore gateway migration test ([#4320](https://github.com/matter-labs/zksync-era/issues/4320)) ([2b87e7b](https://github.com/matter-labs/zksync-era/commit/2b87e7b8f781b61bc7c13b81639908bd1e0c297d))
* verifier request schema ([#4326](https://github.com/matter-labs/zksync-era/issues/4326)) ([eee0dd4](https://github.com/matter-labs/zksync-era/commit/eee0dd4b1a190c36319c02ee7d155e28d42221a5))

## [28.9.0](https://github.com/matter-labs/zksync-era/compare/core-v28.8.0...core-v28.9.0) (2025-07-08)


### Features

* **avail-client:** use chain-specific bridge API endpoints ([#4297](https://github.com/matter-labs/zksync-era/issues/4297)) ([0d150de](https://github.com/matter-labs/zksync-era/commit/0d150def7aafb56a7240835b7d55e844951273cc))

## [28.8.0](https://github.com/matter-labs/zksync-era/compare/core-v28.7.0...core-v28.8.0) (2025-07-07)


### Features

* **ci:** fast integration tests framework ([#4255](https://github.com/matter-labs/zksync-era/issues/4255)) ([a72cbd8](https://github.com/matter-labs/zksync-era/commit/a72cbd85c28552f97116f4c4ab70305c3d7c148e))
* **eigenda:** EigenDA V2 M0 ([#3983](https://github.com/matter-labs/zksync-era/issues/3983)) ([8302a81](https://github.com/matter-labs/zksync-era/commit/8302a81b5903a49a173a6e2c972845e67206cc40))
* EN commit, prove, execute batch transactions verification and finality status ([#4080](https://github.com/matter-labs/zksync-era/issues/4080)) ([d06697d](https://github.com/matter-labs/zksync-era/commit/d06697df7c67b7528d25517ab6e1d1b481d59451))
* **en:** remove JSON RPC syncing ([#4258](https://github.com/matter-labs/zksync-era/issues/4258)) ([d194604](https://github.com/matter-labs/zksync-era/commit/d19460424c2d23bd3b3c6f0579e979f71093dd9a))
* **state-keeper:** implement block commit/rollback ([#4197](https://github.com/matter-labs/zksync-era/issues/4197)) ([99af8fc](https://github.com/matter-labs/zksync-era/commit/99af8fc0ebed6786426e9ccf56b4dcedb3300374))
* **zkstack:** fast fmt ([#4222](https://github.com/matter-labs/zksync-era/issues/4222)) ([d05c3dd](https://github.com/matter-labs/zksync-era/commit/d05c3ddef1e93b0a906ec7bbb290976c6c014051))


### Bug Fixes

* **consensus:** Debug page server port reuse ([#4273](https://github.com/matter-labs/zksync-era/issues/4273)) ([77d043f](https://github.com/matter-labs/zksync-era/commit/77d043fa136fbbbdd6c059aba6c1d3d7dcdd2abd))
* **eth-watch:** return internal errors from BatchRootProcessor ([#4285](https://github.com/matter-labs/zksync-era/issues/4285)) ([1f668e5](https://github.com/matter-labs/zksync-era/commit/1f668e52d6662bcc2959d27541cf640269764fad))
* use FullPubdataBuilder for all pre-gateway batches ([#4265](https://github.com/matter-labs/zksync-era/issues/4265)) ([7c46480](https://github.com/matter-labs/zksync-era/commit/7c464802db0aaadc4170e0b4623fc925770f5739))

## [28.7.0](https://github.com/matter-labs/zksync-era/compare/core-v28.6.0...core-v28.7.0) (2025-06-30)


### Features

* add `pubdata_limit` as batch parameter ([#4228](https://github.com/matter-labs/zksync-era/issues/4228)) ([238941c](https://github.com/matter-labs/zksync-era/commit/238941c633ecc6d0db648486dffeb19cf43fc4c2))
* Add proof manager contracts submodule ([#4189](https://github.com/matter-labs/zksync-era/issues/4189)) ([0c75985](https://github.com/matter-labs/zksync-era/commit/0c759858daaee7c50a83b305e1d65a699b7fbe40))
* **api:** remove token API ([#4180](https://github.com/matter-labs/zksync-era/issues/4180)) ([893a5bc](https://github.com/matter-labs/zksync-era/commit/893a5bc5ab53e15c4c0b700cf69ba2df3ec2c1f8))
* **api:** stabilize zks_gasPerPubdata ([#4225](https://github.com/matter-labs/zksync-era/issues/4225)) ([120fc13](https://github.com/matter-labs/zksync-era/commit/120fc13541f56929a717dd3fdd8b7dc04d199e90))
* **api:** Support Unix domain sockets for healthcheck server ([#4226](https://github.com/matter-labs/zksync-era/issues/4226)) ([b06bacb](https://github.com/matter-labs/zksync-era/commit/b06bacb3587150e997af0b0654bce48b04b6c177))
* **en:** Use config system for env-based EN configuration ([#4104](https://github.com/matter-labs/zksync-era/issues/4104)) ([b706025](https://github.com/matter-labs/zksync-era/commit/b706025a454a24bfbe5f4ff4bcd067d308e07d84))
* **fee_model:** scale the batch fee unconditionally ([#4111](https://github.com/matter-labs/zksync-era/issues/4111)) ([5e3fc0d](https://github.com/matter-labs/zksync-era/commit/5e3fc0d9c4ec9ac4c04cbdba47fa11ff8d3f3591))
* Introduce whitelisted logic for prividium mode ([#4190](https://github.com/matter-labs/zksync-era/issues/4190)) ([e86306f](https://github.com/matter-labs/zksync-era/commit/e86306f78ef55de54b4b280922bd8f73a6e0d419))
* Prover Cluster follow-up [#2](https://github.com/matter-labs/zksync-era/issues/2) ([#4001](https://github.com/matter-labs/zksync-era/issues/4001)) ([d8ed7f7](https://github.com/matter-labs/zksync-era/commit/d8ed7f7a8a0244bfd3f2894a6cf915c9ea3c41a0))
* **state-keeper:** add `process_block` method ([#4087](https://github.com/matter-labs/zksync-era/issues/4087)) ([c580857](https://github.com/matter-labs/zksync-era/commit/c58085770fd6204c65453e7a6a99efc48522917f))
* **state-keeper:** allow sub-second block interval ([#3925](https://github.com/matter-labs/zksync-era/issues/3925)) ([4265ea8](https://github.com/matter-labs/zksync-era/commit/4265ea8a2093d0b901684f844da451abf4ef0f1c))


### Bug Fixes

* **consensus:** Handle custom reverts on VM calls ([#4174](https://github.com/matter-labs/zksync-era/issues/4174)) ([c511cd0](https://github.com/matter-labs/zksync-era/commit/c511cd085800ff9f2922e8a6b706a96447e96ac1))
* **consensus:** Update consensus dependencies ([#4186](https://github.com/matter-labs/zksync-era/issues/4186)) ([110a527](https://github.com/matter-labs/zksync-era/commit/110a527cd130a044a103134695a3fa2dab9269e9))
* **en:** Fix parsing consensus secrets ([#4216](https://github.com/matter-labs/zksync-era/issues/4216)) ([20c7913](https://github.com/matter-labs/zksync-era/commit/20c7913b2d74fa3647532da16d5b63306c13f2fc))
* **eth-watcher:** handle get_logs timeout in eth watch ([#4224](https://github.com/matter-labs/zksync-era/issues/4224)) ([26e5fc4](https://github.com/matter-labs/zksync-era/commit/26e5fc47110dbbf0454c7d36c7d50fc252504bf1))
* Fix crate features some more ([#4177](https://github.com/matter-labs/zksync-era/issues/4177)) ([2964b93](https://github.com/matter-labs/zksync-era/commit/2964b93c22a4fc60fc44ffd56f820b4bea1c828e))
* Fix node_framework feature for high-level crates ([#4171](https://github.com/matter-labs/zksync-era/issues/4171)) ([d42e98d](https://github.com/matter-labs/zksync-era/commit/d42e98d6517c915458c7b79125b6d3544fb8d8db))
* **prover:** Use unified prometheus initialization ([#4173](https://github.com/matter-labs/zksync-era/issues/4173)) ([db4f036](https://github.com/matter-labs/zksync-era/commit/db4f036a5c4fd450178d709793a62ddca43f54ec))
* **prover:** Use unified Prometheus initialization in gateway and job monitor ([#4191](https://github.com/matter-labs/zksync-era/issues/4191)) ([f93704e](https://github.com/matter-labs/zksync-era/commit/f93704eb9e91660b30905577585f59cfb90cd13b))


### Performance Improvements

* Instrumentation for Jemalloc (pt. 2) ([#4204](https://github.com/matter-labs/zksync-era/issues/4204)) ([5e0bd65](https://github.com/matter-labs/zksync-era/commit/5e0bd65042aeebef57e5d977f315b05f6b75f44f))

## [28.6.0](https://github.com/matter-labs/zksync-era/compare/core-v28.5.0...core-v28.6.0) (2025-06-11)


### Features

* **api:** implement `unstable_gasPerPubdata` ([#4124](https://github.com/matter-labs/zksync-era/issues/4124)) ([925d2eb](https://github.com/matter-labs/zksync-era/commit/925d2eb2dd7f126076a3c3612c509a7a9a523037))
* **config:** Report config params in more ways ([#4126](https://github.com/matter-labs/zksync-era/issues/4126)) ([a78531c](https://github.com/matter-labs/zksync-era/commit/a78531c3fb7f8a2d50a120ab6fbd282d1dd9dd28))
* **en:** make `max_batches_to_recheck` configurable ([#4082](https://github.com/matter-labs/zksync-era/issues/4082)) ([358687f](https://github.com/matter-labs/zksync-era/commit/358687f4df8ccf92eecc12aea1497e35006349b3))
* **eth-sender:** use last value in gas adjuster for gateway txs ([#4149](https://github.com/matter-labs/zksync-era/issues/4149)) ([995920a](https://github.com/matter-labs/zksync-era/commit/995920a7d6dd92f7ab7538e0b06a7bcb9df2feb0))


### Bug Fixes

* **config:** Fix `max_response_body_size_overrides` deserialization ([#4165](https://github.com/matter-labs/zksync-era/issues/4165)) ([0c47b7b](https://github.com/matter-labs/zksync-era/commit/0c47b7b4bd627adc8857ec81edd0c70f8d26db83))
* **config:** Fix parsing null values with units + other config fixes ([#4168](https://github.com/matter-labs/zksync-era/issues/4168)) ([506b458](https://github.com/matter-labs/zksync-era/commit/506b45844b280c1bd79c772fa8408d2ef3c1d3b9))
* **consensus:** Make leader in Consensus config optional ([#4145](https://github.com/matter-labs/zksync-era/issues/4145)) ([5761dbb](https://github.com/matter-labs/zksync-era/commit/5761dbbc4df85c675da6e341da2f26acef6ce8b6))


### Performance Improvements

* Jemalloc instrumentation / stats ([#4159](https://github.com/matter-labs/zksync-era/issues/4159)) ([12271c8](https://github.com/matter-labs/zksync-era/commit/12271c8142e21ccd3e818a769b29e9bac106539d))

## [28.5.0](https://github.com/matter-labs/zksync-era/compare/core-v28.4.0...core-v28.5.0) (2025-06-06)


### Features

* **api:** implement `debug_getRawTransaction(s)` ([#4109](https://github.com/matter-labs/zksync-era/issues/4109)) ([2b9b76d](https://github.com/matter-labs/zksync-era/commit/2b9b76d3567acaec33421dd761fffde8d69f1ec7))
* **avail-client:** async blob dispatch ([#4010](https://github.com/matter-labs/zksync-era/issues/4010)) ([7a18647](https://github.com/matter-labs/zksync-era/commit/7a186478700eeeaea51920d94cfb7c4e2b453ba5))
* **consensus:** Validator committee rotation ([#4014](https://github.com/matter-labs/zksync-era/issues/4014)) ([333efea](https://github.com/matter-labs/zksync-era/commit/333efea309e766c46a20e48868b7bbd0986910ec))
* **en:** Introduce a fallback for gateway url ([#4114](https://github.com/matter-labs/zksync-era/issues/4114)) ([6bc2757](https://github.com/matter-labs/zksync-era/commit/6bc2757fe584e44e4c66aa1dc9a11df9ebbc0627))


### Bug Fixes

* **eth sender:** fix blob sender fee calculation ([#4143](https://github.com/matter-labs/zksync-era/issues/4143)) ([563a15f](https://github.com/matter-labs/zksync-era/commit/563a15fc7572752f4b8360c10c93d71aed3bb06f))
* **gateway:** Do not create l2 client if server is settlment layer ([#4131](https://github.com/matter-labs/zksync-era/issues/4131)) ([20bd93b](https://github.com/matter-labs/zksync-era/commit/20bd93b1fe8173d3c2db152a5668775a5ef7853e))
* **proof_data_handler:** get L1BatchCommitmentMode from database instead of config ([#4107](https://github.com/matter-labs/zksync-era/issues/4107)) ([af5654d](https://github.com/matter-labs/zksync-era/commit/af5654db7ce985b6d0a06a561c39e2ac97b76fcc))


### Performance Improvements

* **en:** Use jemalloc for external node ([#4146](https://github.com/matter-labs/zksync-era/issues/4146)) ([3cf0b9c](https://github.com/matter-labs/zksync-era/commit/3cf0b9ca61d221b1913e9dbadc69d318d7441497))
* **state-keeper:** Reduce retained data in updates manager ([#4140](https://github.com/matter-labs/zksync-era/issues/4140)) ([2547d93](https://github.com/matter-labs/zksync-era/commit/2547d9308d72bb091e97f47e6dd40780357f404b))

## [28.4.0](https://github.com/matter-labs/zksync-era/compare/core-v28.3.0...core-v28.4.0) (2025-06-02)


### Features

* **contract-verifier:** add Etherscan-like endpoints used for contract verification ([#4096](https://github.com/matter-labs/zksync-era/issues/4096)) ([3a28262](https://github.com/matter-labs/zksync-era/commit/3a28262de32dd802891da857524d9c60837abea4))


### Bug Fixes

* **api:** rollback finality-related api changes of 28.3.0 ([#4105](https://github.com/matter-labs/zksync-era/issues/4105)) ([1c797c1](https://github.com/matter-labs/zksync-era/commit/1c797c1a392c930e9c48fcf39aea032b546632bd))
* deprecate submission obj in favor of data obj ([#4083](https://github.com/matter-labs/zksync-era/issues/4083)) ([9ce873d](https://github.com/matter-labs/zksync-era/commit/9ce873d28e46858e3940aefac24746ac9d8fcd64))

## [28.3.0](https://github.com/matter-labs/zksync-era/compare/core-v28.2.1...core-v28.3.0) (2025-05-29)


### Features

* add `en_getProtocolVersionInfo`, fix `eth_protocolVersion` ([#3988](https://github.com/matter-labs/zksync-era/issues/3988)) ([5adb640](https://github.com/matter-labs/zksync-era/commit/5adb64042c7f1d0699c4be50435fc96ee367fa13))
* add prividium mode to zkstack explorer ([#4079](https://github.com/matter-labs/zksync-era/issues/4079)) ([c571914](https://github.com/matter-labs/zksync-era/commit/c5719142456f563956f265e89e2074df8acb7484))
* **avail-gas-relay:** add empty json check ([#4034](https://github.com/matter-labs/zksync-era/issues/4034)) ([55f0dd5](https://github.com/matter-labs/zksync-era/commit/55f0dd56eb749733c8f3b9293781ab84acc2913b))
* **config:** Support serde-style enums in config system ([#4055](https://github.com/matter-labs/zksync-era/issues/4055)) ([84eed67](https://github.com/matter-labs/zksync-era/commit/84eed672bc29d65d2d03e92ca543a4b795556de6))
* Configuration system PoC ([#3851](https://github.com/matter-labs/zksync-era/issues/3851)) ([7b449c2](https://github.com/matter-labs/zksync-era/commit/7b449c216aa250cf99bb79e69df810f566dcc28a))
* **config:** Use native representation for duration params ([#4072](https://github.com/matter-labs/zksync-era/issues/4072)) ([1674906](https://github.com/matter-labs/zksync-era/commit/167490639795ae231b9e57e8453177a860f2c302))
* **contract_verifier:** read compiler versions from cbor metadata if available ([#4002](https://github.com/matter-labs/zksync-era/issues/4002)) ([9bc20a4](https://github.com/matter-labs/zksync-era/commit/9bc20a486d0bd8b169a836c1bf3f805f53315944))
* **eigenda:** Ensure finality ([#4033](https://github.com/matter-labs/zksync-era/issues/4033)) ([b794c4e](https://github.com/matter-labs/zksync-era/commit/b794c4ed577cc38e694d6dea6c27eeb81296d45b))
* **en:** Add revert CLI command for external node ([#4053](https://github.com/matter-labs/zksync-era/issues/4053)) ([5e4bf51](https://github.com/matter-labs/zksync-era/commit/5e4bf514573ee7c0203046c01ddd3fc99a139054))
* **eth_sender:** Add fast finalization into eth_tx_manager ([#4070](https://github.com/matter-labs/zksync-era/issues/4070)) ([c6b815d](https://github.com/matter-labs/zksync-era/commit/c6b815d038c39782838618059c4a35894ca527ee))
* **eth-sender:** limit fees on resend ([#3885](https://github.com/matter-labs/zksync-era/issues/3885)) ([21b52f9](https://github.com/matter-labs/zksync-era/commit/21b52f9936aea91a7b80d65fefa62acf18517c64))
* make PRIORITY_EXPIRATION configurable ([#3764](https://github.com/matter-labs/zksync-era/issues/3764)) ([5a97993](https://github.com/matter-labs/zksync-era/commit/5a97993e84576714d7837273ffea66376aa41a74))
* move optimistic API to `unstable` ([#3976](https://github.com/matter-labs/zksync-era/issues/3976)) ([2d0c41d](https://github.com/matter-labs/zksync-era/commit/2d0c41dfdfb926a70bd7e637f96e79015eb07125))
* **private-rpc:** improved compatibility with ethers library + tests ([#4046](https://github.com/matter-labs/zksync-era/issues/4046)) ([0e2e0d8](https://github.com/matter-labs/zksync-era/commit/0e2e0d89b3bb0e56918cd3ccdb041800780ab088))
* remove `zks_getL2ToL1MsgProof` ([#3965](https://github.com/matter-labs/zksync-era/issues/3965)) ([037dff0](https://github.com/matter-labs/zksync-era/commit/037dff0501c7569edf49ae6015a73bb22e408782))


### Bug Fixes

* **contract-verifier:** improve etherscan error handling ([#4061](https://github.com/matter-labs/zksync-era/issues/4061)) ([0e1a5b2](https://github.com/matter-labs/zksync-era/commit/0e1a5b2314f0b4170ed452682c69ab10dea1e642))
* **deplpoyment_filter:** Allow protocol upgrade ([#4098](https://github.com/matter-labs/zksync-era/issues/4098)) ([5f7a392](https://github.com/matter-labs/zksync-era/commit/5f7a392f6c306aef038386932d065b1a439f8970))
* **en:** Update snapshot applier health during storage logs recovery ([#4028](https://github.com/matter-labs/zksync-era/issues/4028)) ([61b34db](https://github.com/matter-labs/zksync-era/commit/61b34dbdb476b1ab45939a4909bd63fb94181ed7))
* **eth_aggregator:** choose Blobs or Calldata based on the previous batch ([#4073](https://github.com/matter-labs/zksync-era/issues/4073)) ([1009488](https://github.com/matter-labs/zksync-era/commit/1009488545eee8e6ad27d039cc303973e93ffe73))
* Fix prefix for `TeeProofDataHandlerConfig` ([#4043](https://github.com/matter-labs/zksync-era/issues/4043)) ([4e42573](https://github.com/matter-labs/zksync-era/commit/4e425736e30c9678ef4418bdee2d4723313c81b4))
* **gateway_migrator:** add gateway migrator config ([#4041](https://github.com/matter-labs/zksync-era/issues/4041)) ([5ce7240](https://github.com/matter-labs/zksync-era/commit/5ce72402393fb3cd34a5e369689f630d23fda8fc))
* Prefix env vars for `zksync_server` ([#4039](https://github.com/matter-labs/zksync-era/issues/4039)) ([3b2dd70](https://github.com/matter-labs/zksync-era/commit/3b2dd70e858312d1709b8ac64a651f4f7f806e05))
* **vm:** update v27 ([#4035](https://github.com/matter-labs/zksync-era/issues/4035)) ([b7aeab6](https://github.com/matter-labs/zksync-era/commit/b7aeab64ce5c915233a773542ef64e79bf3893ee))


### Performance Improvements

* **api:** Add measures to improve API performance ([#4051](https://github.com/matter-labs/zksync-era/issues/4051)) ([d171162](https://github.com/matter-labs/zksync-era/commit/d1711629c584c4a15fd0185785cf3fbff9a35a6b))
* **api:** Add observability for `contractAddress` logic ([#4066](https://github.com/matter-labs/zksync-era/issues/4066)) ([a179d98](https://github.com/matter-labs/zksync-era/commit/a179d98c55dcf5c82327377e02c846d795690f9f))
* deallocate all heaps that don't need to be kept ([#4093](https://github.com/matter-labs/zksync-era/issues/4093)) ([5fd6769](https://github.com/matter-labs/zksync-era/commit/5fd676911c0eb8480d6917b2832a23b5c7e27ebc))
* **en:** DB-related performance improvements ([#4017](https://github.com/matter-labs/zksync-era/issues/4017)) ([3674332](https://github.com/matter-labs/zksync-era/commit/367433223e48bcddda33ecacce5bd2724bfc9738))

## [28.2.1](https://github.com/matter-labs/zksync-era/compare/core-v28.2.0...core-v28.2.1) (2025-05-15)


### Bug Fixes

* **api:** Add safe status ([#4005](https://github.com/matter-labs/zksync-era/issues/4005)) ([b71c167](https://github.com/matter-labs/zksync-era/commit/b71c167919601a2a1fb06284834abb8ad4cc94e7))
* **vm:** update deps ([#4019](https://github.com/matter-labs/zksync-era/issues/4019)) ([2b51c6d](https://github.com/matter-labs/zksync-era/commit/2b51c6df08b45cc41fd6b4d930c31670d004a93d))


### Performance Improvements

* **db:** Break loading storage logs for recovery into smaller chunks ([#3947](https://github.com/matter-labs/zksync-era/issues/3947)) ([91772a4](https://github.com/matter-labs/zksync-era/commit/91772a48aa48a7fd4d148072f51d8ada4fc8782c))

## [28.2.0](https://github.com/matter-labs/zksync-era/compare/core-v28.1.0...core-v28.2.0) (2025-05-12)


### Features

* bump rustc to `nightly-2025-03-19` ([#3985](https://github.com/matter-labs/zksync-era/issues/3985)) ([d27390e](https://github.com/matter-labs/zksync-era/commit/d27390e14a586de7dccb974a0cb54352de3536b3))


### Bug Fixes

* **da:** gas relay check_finality ([#3994](https://github.com/matter-labs/zksync-era/issues/3994)) ([a664787](https://github.com/matter-labs/zksync-era/commit/a66478756d96dcd497fabaf62f08aab4ff345342))

## [28.1.0](https://github.com/matter-labs/zksync-era/compare/core-v28.0.0...core-v28.1.0) (2025-05-08)


### Features

* **contract_verifier:** add etherscan verification request support to the verifier api ([#3956](https://github.com/matter-labs/zksync-era/issues/3956)) ([87938b3](https://github.com/matter-labs/zksync-era/commit/87938b3b94688ff32bbdd0e35396558c7ab5bb88))
* **en:** remove dependency on pubdata commitment mode ([#3826](https://github.com/matter-labs/zksync-era/issues/3826)) ([a0c78c0](https://github.com/matter-labs/zksync-era/commit/a0c78c022460d6441345b205fa00ac447b0910c8))
* **zkstack_cli:** finish enabling migrating chain from Gateway + remove the gateway feature flag ([#3924](https://github.com/matter-labs/zksync-era/issues/3924)) ([d091c90](https://github.com/matter-labs/zksync-era/commit/d091c90f61b95e9dea4be486d85fd520a706133a))
* **zksync-server:** add support for tee_proof_data_handler component ([#3957](https://github.com/matter-labs/zksync-era/issues/3957)) ([7c573fc](https://github.com/matter-labs/zksync-era/commit/7c573fcce7b1d48f6db431a23abd96dea1d60274))


### Bug Fixes

* Changes to zkstack after testing migration from GW ([#3969](https://github.com/matter-labs/zksync-era/issues/3969)) ([b63e607](https://github.com/matter-labs/zksync-era/commit/b63e60734ff4e2f4fd00c15920c9d3c84ed7c4fd))
* **eth-sender:** fix issues for operator rotation ([#3972](https://github.com/matter-labs/zksync-era/issues/3972)) ([69d3cec](https://github.com/matter-labs/zksync-era/commit/69d3cec83bc98600d4a98f60d2a31181d8c65066))
* **gateway-migrator:** Properly handle unknown settlement layer ([#3961](https://github.com/matter-labs/zksync-era/issues/3961)) ([b43e315](https://github.com/matter-labs/zksync-era/commit/b43e3159b89a4808925ba62616ea5e85fb2d63e3))
* **tee_proof_data_handler:** handle fallback deserialization for blob store data ([#3971](https://github.com/matter-labs/zksync-era/issues/3971)) ([a785fe9](https://github.com/matter-labs/zksync-era/commit/a785fe99311905fae3164d316c3ae5fb14c5de16))


### Performance Improvements

* **state-keeper:** Parallelize loading storage log chunks when recovering SK cache ([#3941](https://github.com/matter-labs/zksync-era/issues/3941)) ([72dda97](https://github.com/matter-labs/zksync-era/commit/72dda979672551c8798ec40edbc4ae69c467407f))

## [28.0.0](https://github.com/matter-labs/zksync-era/compare/core-v27.5.2...core-v28.0.0) (2025-05-05)


### ⚠ BREAKING CHANGES

* update zksync-protocol and zksync-crypto-gpu deps ([#3948](https://github.com/matter-labs/zksync-era/issues/3948))

### Features

* Add Support for Protocol Version v28 ([#3821](https://github.com/matter-labs/zksync-era/issues/3821)) ([5419420](https://github.com/matter-labs/zksync-era/commit/5419420e23a3c083187065219a0722179dab0419))
* update zksync-protocol and zksync-crypto-gpu deps ([#3948](https://github.com/matter-labs/zksync-era/issues/3948)) ([1ddf85a](https://github.com/matter-labs/zksync-era/commit/1ddf85a14b6a54ce926482eafc84ef0979c9afd3))


### Bug Fixes

* **deployment-allowlist:** Add an ability to read deploy list from configs directly ([#3917](https://github.com/matter-labs/zksync-era/issues/3917)) ([897d3df](https://github.com/matter-labs/zksync-era/commit/897d3dff70a6651f7b5a0b6d189e57ad2acd7790))
* **gateway-migration:** Allow prove, execute txs to be sent during the migration ([#3848](https://github.com/matter-labs/zksync-era/issues/3848)) ([3a3f700](https://github.com/matter-labs/zksync-era/commit/3a3f700bc54c824a8b47eeb4f4b93626e6a4993f))
* **gateway-migrator:** Do not fail gateway migrator during the problems with network ([#3938](https://github.com/matter-labs/zksync-era/issues/3938)) ([f91aecc](https://github.com/matter-labs/zksync-era/commit/f91aecc5372f167df6ad3281e76f884ffaa63b84))

## [27.5.2](https://github.com/matter-labs/zksync-era/compare/core-v27.5.1...core-v27.5.2) (2025-05-02)


### Bug Fixes

* Fix issue with EVM bytecodes not being stored in DB ([#3928](https://github.com/matter-labs/zksync-era/issues/3928)) ([0e8f1da](https://github.com/matter-labs/zksync-era/commit/0e8f1dacb27be3b33aaabf842ea4350e20a3cada))

## [27.5.1](https://github.com/matter-labs/zksync-era/compare/core-v27.5.0...core-v27.5.1) (2025-05-01)


### Bug Fixes

* **api:** Fix `contractAddress` in transaction receipts for unparseable deployment calldata ([#3927](https://github.com/matter-labs/zksync-era/issues/3927)) ([82e9e05](https://github.com/matter-labs/zksync-era/commit/82e9e05e3f50658af867454cbdb880818feff37d))
* **eth-sender:** consider null from_addr in `get_next_nonce` ([#3933](https://github.com/matter-labs/zksync-era/issues/3933)) ([5b3c733](https://github.com/matter-labs/zksync-era/commit/5b3c7337f504a25b4cdea2ae550256755eb03432))
* witness inputs filename ([#3932](https://github.com/matter-labs/zksync-era/issues/3932)) ([7aa3c5f](https://github.com/matter-labs/zksync-era/commit/7aa3c5fc3b872c3a1e13fab3a2fc3d467aae44a5))

## [27.5.0](https://github.com/matter-labs/zksync-era/compare/core-v27.4.0...core-v27.5.0) (2025-04-30)


### Features

* add `criterion_capacity_filled` metric ([#3840](https://github.com/matter-labs/zksync-era/issues/3840)) ([75d1bb1](https://github.com/matter-labs/zksync-era/commit/75d1bb176ef2cd18da190a532ae8eadf20078db9))
* add dedicated TEE proof data handler module ([#3872](https://github.com/matter-labs/zksync-era/issues/3872)) ([ac64ee6](https://github.com/matter-labs/zksync-era/commit/ac64ee6b3384c0802cd149d51a2a4d3779228dbb))
* **db:** rework `max_enumeration_index` ([#3906](https://github.com/matter-labs/zksync-era/issues/3906)) ([d0384ac](https://github.com/matter-labs/zksync-era/commit/d0384ac504fea78fa94ad9a9e6101b32aeb90dd0))
* **eth_sender:** add transaction's base fee max cap configuration ([#3880](https://github.com/matter-labs/zksync-era/issues/3880)) ([94c84dc](https://github.com/matter-labs/zksync-era/commit/94c84dcb6b5c69b40e0759ce9c77eced52c4716e))
* **eth-sender:** rework handling of failed tx send attempts in eth sender ([#3879](https://github.com/matter-labs/zksync-era/issues/3879)) ([4c871c9](https://github.com/matter-labs/zksync-era/commit/4c871c988a13765fecea636dbc6d1a04e3dfee59))
* **eth-sender:** set `from_addr` for non-blob txs ([#3898](https://github.com/matter-labs/zksync-era/issues/3898)) ([c699f8a](https://github.com/matter-labs/zksync-era/commit/c699f8a54a07604bb4630742811b247b060ffb0a))
* Port core changes for gateway the new gateway scripts ([#3854](https://github.com/matter-labs/zksync-era/issues/3854)) ([c4212ef](https://github.com/matter-labs/zksync-era/commit/c4212ef51590200bb494a3afc23851129bffb347))
* Proof data handler client ([#3874](https://github.com/matter-labs/zksync-era/issues/3874)) ([daf6f7b](https://github.com/matter-labs/zksync-era/commit/daf6f7b80a018204693f8ad7296574b8b55dc6d9))
* Reversed prover gateway server ([#3855](https://github.com/matter-labs/zksync-era/issues/3855)) ([a78c3ae](https://github.com/matter-labs/zksync-era/commit/a78c3ae1f74c0f4adfc9b94681c7b80b44dd16f1))
* rework prover job identifiers ([#3888](https://github.com/matter-labs/zksync-era/issues/3888)) ([073326f](https://github.com/matter-labs/zksync-era/commit/073326f124eae808ef0e25694e99f0dab5ee7af4))
* Unify proof generation data processor  ([#3850](https://github.com/matter-labs/zksync-era/issues/3850)) ([0b86920](https://github.com/matter-labs/zksync-era/commit/0b8692046dfc34ccb5bd60bde090cf3598470784))
* **zkstack_cli:** update gateway chain scripts ([#3852](https://github.com/matter-labs/zksync-era/issues/3852)) ([542c7a9](https://github.com/matter-labs/zksync-era/commit/542c7a9c146f0b3b16d87a26590fc7958f910c79))


### Bug Fixes

* address issue with evm verification with immutable ref  ([#3866](https://github.com/matter-labs/zksync-era/issues/3866)) ([108ca6a](https://github.com/matter-labs/zksync-era/commit/108ca6a596aa33f9cc665167c910960d5d70ae15))
* **api:** Allow any deployment if initiator is in the allowlist ([#3902](https://github.com/matter-labs/zksync-era/issues/3902)) ([14ac493](https://github.com/matter-labs/zksync-era/commit/14ac493d7454d6f1733393e344bdb111b9c341f4))
* **api:** Return deployment nonce for contracts in `eth_getTransactionCount` ([#3833](https://github.com/matter-labs/zksync-era/issues/3833)) ([d8eecbb](https://github.com/matter-labs/zksync-era/commit/d8eecbb68b01d18853a936068f335411dfde33fe))
* bincode file extension ([#3922](https://github.com/matter-labs/zksync-era/issues/3922)) ([9362146](https://github.com/matter-labs/zksync-era/commit/93621468ff8bda56bd34b48a0cb312080f31d861))
* **consistency-checker:** actually skip consitency check for old batches on different SL ([#3915](https://github.com/matter-labs/zksync-era/issues/3915)) ([2e2603b](https://github.com/matter-labs/zksync-era/commit/2e2603b9023be03c52bca433d293ef276792032a))
* **contracts:** Move server notifier to l1 ([#3864](https://github.com/matter-labs/zksync-era/issues/3864)) ([2b71b76](https://github.com/matter-labs/zksync-era/commit/2b71b7616807de7c473a27531149e1689188b516))
* **en:** Fix chunked genesis recovery ([#3849](https://github.com/matter-labs/zksync-era/issues/3849)) ([709921d](https://github.com/matter-labs/zksync-era/commit/709921d866cd8422a7d3ee6d19459f35ab4fd329))
* **en:** Fix state keeper cache recovery with pruning enabled ([#3876](https://github.com/matter-labs/zksync-era/issues/3876)) ([916edd5](https://github.com/matter-labs/zksync-era/commit/916edd55f8537f84f78d1086168b907d2d5bb7ae))
* Fix `ObservabilityGuard` hanging up on drop ([#3868](https://github.com/matter-labs/zksync-era/issues/3868)) ([602c275](https://github.com/matter-labs/zksync-era/commit/602c275c4f72e20a01e7086fc6f22157b5352dbd))
* **loadtest:** Increase gas limit for loadtest ([#3901](https://github.com/matter-labs/zksync-era/issues/3901)) ([07eb78e](https://github.com/matter-labs/zksync-era/commit/07eb78ee26af9ed486eea9aed77d9ab3fc896fcb))
* **mempool:** keep nonces in memory for stashed accounts ([#3884](https://github.com/matter-labs/zksync-era/issues/3884)) ([358e117](https://github.com/matter-labs/zksync-era/commit/358e117425273d2835a5ecde3065ad7f0a9f4045))
* prover gateway failing to decode response body ([#3892](https://github.com/matter-labs/zksync-era/issues/3892)) ([f823a3d](https://github.com/matter-labs/zksync-era/commit/f823a3d47a3c35867143e274dcabb2e130d28ad3))


### Performance Improvements

* **db:** drop ix_initial_writes_t1 ([#3905](https://github.com/matter-labs/zksync-era/issues/3905)) ([deacdb7](https://github.com/matter-labs/zksync-era/commit/deacdb7ce2f30dfd5d144dfd35c67e486376ba1f))
* **db:** use copy in `insert_initial_writes` ([#3899](https://github.com/matter-labs/zksync-era/issues/3899)) ([c6f1598](https://github.com/matter-labs/zksync-era/commit/c6f159862d7f5f8b0ee16df2609e2c72db356425))

## [27.4.0](https://github.com/matter-labs/zksync-era/compare/core-v27.3.0...core-v27.4.0) (2025-04-15)


### Features

* **eth_sender:** calculate gas limit ([#3785](https://github.com/matter-labs/zksync-era/issues/3785)) ([cee1172](https://github.com/matter-labs/zksync-era/commit/cee117271bc88ce5da8f6d2ca4ebd7b531a966d7))
* Forbid null `to` for EIP-712 transactions ([#3844](https://github.com/matter-labs/zksync-era/issues/3844)) ([e64ee71](https://github.com/matter-labs/zksync-era/commit/e64ee71d30f209f180f6b25ee807871e511dee23))
* **state-keeper:** protocol upgrade sealer ([#3831](https://github.com/matter-labs/zksync-era/issues/3831)) ([e0a6b7c](https://github.com/matter-labs/zksync-era/commit/e0a6b7c595bca555d04d036b25ec9aa232a62e96))
* **zkstack:** Allow to run separate integration test suites ([d287725](https://github.com/matter-labs/zksync-era/commit/d287725778b8dc625ed74088ea5dd2a8982d0224))


### Bug Fixes

* **en:** handling of old batches in consistency checker on SL change ([#3828](https://github.com/matter-labs/zksync-era/issues/3828)) ([e2a1340](https://github.com/matter-labs/zksync-era/commit/e2a1340707ff9011497d9f001e32a528176c3302))
* Use current timestamp as default for `batch_sealed_at` ([#3832](https://github.com/matter-labs/zksync-era/issues/3832)) ([10b3b2e](https://github.com/matter-labs/zksync-era/commit/10b3b2e545161a1eafc57f2124eeed5b8f3b4f09))

## [27.3.0](https://github.com/matter-labs/zksync-era/compare/core-v27.2.0...core-v27.3.0) (2025-04-10)


### Features

* add finality check in DA dispatcher ([#3795](https://github.com/matter-labs/zksync-era/issues/3795)) ([a3f9f0b](https://github.com/matter-labs/zksync-era/commit/a3f9f0bf41672224586d284e4e06b8ae11c41b49))
* **api:** Support permissioned deployments ([#3726](https://github.com/matter-labs/zksync-era/issues/3726)) ([233a4d2](https://github.com/matter-labs/zksync-era/commit/233a4d245abaaead9044664ead3037e1f7c43499))
* **eth-sender:** Calculate max-gas-per-pubdata ([#3782](https://github.com/matter-labs/zksync-era/issues/3782)) ([e8a1ce9](https://github.com/matter-labs/zksync-era/commit/e8a1ce9cb133ace10262d051f8afdf4179c80321))
* Rework serialization of prover API-related types ([#3805](https://github.com/matter-labs/zksync-era/issues/3805)) ([afafc29](https://github.com/matter-labs/zksync-era/commit/afafc292ea003e19d280c0178ca6b49b5b14917f))
* Update prover job ordering ([#3769](https://github.com/matter-labs/zksync-era/issues/3769)) ([5b74022](https://github.com/matter-labs/zksync-era/commit/5b740224fd27b0792d2d1602577c032fd3a31074))


### Bug Fixes

* **en:** Fix EVM bytecode handling during snapshot recovery ([#3792](https://github.com/matter-labs/zksync-era/issues/3792)) ([c579e95](https://github.com/matter-labs/zksync-era/commit/c579e95a9603c33f7e830cf09c479fa90fe36654))
* Fix security issues (bump dependencies) ([#3813](https://github.com/matter-labs/zksync-era/issues/3813)) ([c6def9c](https://github.com/matter-labs/zksync-era/commit/c6def9c0e480bd73fc0ea29a7d3393c297c8afb7))


### Performance Improvements

* **dal:** Optimize some sql queries ([#3824](https://github.com/matter-labs/zksync-era/issues/3824)) ([7a0fde7](https://github.com/matter-labs/zksync-era/commit/7a0fde7390d143a3e27699c1ab33946941ca4e31))

## [27.2.0](https://github.com/matter-labs/zksync-era/compare/core-v27.1.0...core-v27.2.0) (2025-04-03)


### Features

* **api:** Allow EVM bytecode overrides ([#3761](https://github.com/matter-labs/zksync-era/issues/3761)) ([8aee9f8](https://github.com/matter-labs/zksync-era/commit/8aee9f8c96346bfb99575ad213c44106c4e79f2f))
* **en:** Allow tree / state keeper cache recovery for genesis ([#3781](https://github.com/matter-labs/zksync-era/issues/3781)) ([dd18c48](https://github.com/matter-labs/zksync-era/commit/dd18c48c9b3788a665c79d4a8eaf8c804d65fbc8))
* **eth_signer:** Support eip712 txs ([#3752](https://github.com/matter-labs/zksync-era/issues/3752)) ([e278ab5](https://github.com/matter-labs/zksync-era/commit/e278ab584081470159267fb02c42336393feb2fe))
* **gateway:** Handle server shutting down error ([#3777](https://github.com/matter-labs/zksync-era/issues/3777)) ([8e11adc](https://github.com/matter-labs/zksync-era/commit/8e11adcd8429e82183a0c57e63c7add83f936366))
* **gateway:** Migration to Gateway  ([#3654](https://github.com/matter-labs/zksync-era/issues/3654)) ([2858ba0](https://github.com/matter-labs/zksync-era/commit/2858ba028a4e59eb518515e8dd56de9f609c3469))
* **main:** Eigenda add custom quorum params ([#3719](https://github.com/matter-labs/zksync-era/issues/3719)) ([dac58ad](https://github.com/matter-labs/zksync-era/commit/dac58ad8f14c1206d10ce120345f16e61a021ea7))
* Update zksync-protocol deps to 0.151.5 ([#3790](https://github.com/matter-labs/zksync-era/issues/3790)) ([92beffe](https://github.com/matter-labs/zksync-era/commit/92beffe52ed2d40b11d800afbe97e9099d6f90a8))
* **zkos:** remove prev index pointer from leaves ([#3771](https://github.com/matter-labs/zksync-era/issues/3771)) ([5b8fd29](https://github.com/matter-labs/zksync-era/commit/5b8fd290befc3f29fc385c959c5b09bd1b7b0f58))


### Bug Fixes

* insert tokens without PG copy ([#3778](https://github.com/matter-labs/zksync-era/issues/3778)) ([b7a8152](https://github.com/matter-labs/zksync-era/commit/b7a8152fe1a5f46a4f3f6211961cbba69b438d17))
* **prover:** Force set all the `prover_job` labels to 0 ([#3787](https://github.com/matter-labs/zksync-era/issues/3787)) ([3ecc8db](https://github.com/matter-labs/zksync-era/commit/3ecc8db0862f6b893a9ab09f76532d937dc8cf62))
* **prover:** Reevaluation of 'heavy' jobs for WVG ([#3754](https://github.com/matter-labs/zksync-era/issues/3754)) ([2a8d33b](https://github.com/matter-labs/zksync-era/commit/2a8d33b92bb72540d37d35ce4677454d073ba002))

## [27.1.0](https://github.com/matter-labs/zksync-era/compare/core-v27.0.0...core-v27.1.0) (2025-03-27)


### Features

* **consensus:** Add consensus protocol versioning ([#3720](https://github.com/matter-labs/zksync-era/issues/3720)) ([d1b4308](https://github.com/matter-labs/zksync-era/commit/d1b4308ff82da11515d8080c8e83f67c0f1812eb))
* **zkos:** Implement ZK OS tree manager ([#3730](https://github.com/matter-labs/zksync-era/issues/3730)) ([efc0007](https://github.com/matter-labs/zksync-era/commit/efc00076f8211f261825c41625c3ec9bd4f0905a))


### Bug Fixes

* **api:** Fix panic applying nonce override ([#3748](https://github.com/matter-labs/zksync-era/issues/3748)) ([944059b](https://github.com/matter-labs/zksync-era/commit/944059b0cb2911debc3253a3066c4ce855b5196b))
* **contract_verifier:** order deploy events in `get_contract_info_for_verification` ([#3766](https://github.com/matter-labs/zksync-era/issues/3766)) ([6e3c031](https://github.com/matter-labs/zksync-era/commit/6e3c031e8663614a4272f09c84bd385c7e9852dd))
* make proof data handler backwards compatible ([#3767](https://github.com/matter-labs/zksync-era/issues/3767)) ([bdbbaaa](https://github.com/matter-labs/zksync-era/commit/bdbbaaa4974399afec2394e0ffea9f9f6876e1e2))
* **proof_data_handler:** update save_proof_artifacts_metadata UPDATE ([#3758](https://github.com/matter-labs/zksync-era/issues/3758)) ([ed4926f](https://github.com/matter-labs/zksync-era/commit/ed4926f4cd0705ad6f1ed57ff5ee2c8c79af4987))
* **vm:** Fix VM divergence in revert data ([#3570](https://github.com/matter-labs/zksync-era/issues/3570)) ([b82e2e4](https://github.com/matter-labs/zksync-era/commit/b82e2e47cd8a53a02e8122c03b003893d300e604))

## [27.0.0](https://github.com/matter-labs/zksync-era/compare/core-v26.7.0...core-v27.0.0) (2025-03-21)


### ⚠ BREAKING CHANGES

* Remove old prover stack ([#3729](https://github.com/matter-labs/zksync-era/issues/3729))
* V27 update ([#3580](https://github.com/matter-labs/zksync-era/issues/3580))

### Features

* **eigenda:** EigenDA M0 ([#3650](https://github.com/matter-labs/zksync-era/issues/3650)) ([2a3cae9](https://github.com/matter-labs/zksync-era/commit/2a3cae96374c2bd15a0d8c23770a79ad6d82c8b9))
* **eth-watch:** split heavy get logs requests if 503 ([#3706](https://github.com/matter-labs/zksync-era/issues/3706)) ([406a3ff](https://github.com/matter-labs/zksync-era/commit/406a3ff096127b0b18181f5aabc70518664afa77))
* **gateway:** Requirement to stop L1-&gt;L2 transactions before v26 upgrade ([#3707](https://github.com/matter-labs/zksync-era/issues/3707)) ([0a095b7](https://github.com/matter-labs/zksync-era/commit/0a095b704c513dc72dbb417ba2731b09e9a2dd5d))
* Remove old prover stack ([#3729](https://github.com/matter-labs/zksync-era/issues/3729)) ([fbbdc76](https://github.com/matter-labs/zksync-era/commit/fbbdc76b86bf4f474c4c045778b69f80a30e9c60))
* Starting from v26 version, always fetch the address of the validator timelock from CTM ([#3721](https://github.com/matter-labs/zksync-era/issues/3721)) ([d3db521](https://github.com/matter-labs/zksync-era/commit/d3db521a657f28e513137ef9f157cf8183a83ebc))
* **storage:** rocksdb storage extension for zkos ([#3698](https://github.com/matter-labs/zksync-era/issues/3698)) ([2779245](https://github.com/matter-labs/zksync-era/commit/277924511eec53b3e48a277141e617f1a5bbd56b))
* Use JSON-RPC for core &lt;&gt; prover interaction ([#3626](https://github.com/matter-labs/zksync-era/issues/3626)) ([4e74730](https://github.com/matter-labs/zksync-era/commit/4e7473011e6551bbeb3e7862872e99721aeba232))
* V27 update ([#3580](https://github.com/matter-labs/zksync-era/issues/3580)) ([9e18550](https://github.com/matter-labs/zksync-era/commit/9e1855050e3457ecef2b45a75e993dcdc2de370a))
* **zksync_tee_prover:** add support for TDX and None TEE types ([#3711](https://github.com/matter-labs/zksync-era/issues/3711)) ([11d166b](https://github.com/matter-labs/zksync-era/commit/11d166bcd54c84e6cf323dafb5463b97c9202aab))
* **zksync_tee_prover:** read config in TDX from google metadata ([#3702](https://github.com/matter-labs/zksync-era/issues/3702)) ([e50201c](https://github.com/matter-labs/zksync-era/commit/e50201c3fa46eb13199bb2ae13204f5a0c8e4adf))


### Bug Fixes

* **contract-verifier:** Allow reverification of system contracts in Yul ([#3735](https://github.com/matter-labs/zksync-era/issues/3735)) ([e99b548](https://github.com/matter-labs/zksync-era/commit/e99b548e0f29d80d04245bff59e6dd8e285a2107))
* **contract-verifier:** contract verifier to return fully matched verification info when available ([#3734](https://github.com/matter-labs/zksync-era/issues/3734)) ([1a0f27f](https://github.com/matter-labs/zksync-era/commit/1a0f27f02dddcd02021a423c33f4a503ef53bbf8))
* **contract-verifier:** Correctly process partial verification for EVM contracts ([#3688](https://github.com/matter-labs/zksync-era/issues/3688)) ([8292234](https://github.com/matter-labs/zksync-era/commit/82922344c689a5e299ac27625f9e8490b57810ea))
* **contract-verifier:** Ignore suppressable errors during compilation ([#3747](https://github.com/matter-labs/zksync-era/issues/3747)) ([a5955c4](https://github.com/matter-labs/zksync-era/commit/a5955c4e27a5fed5c18e752af00e6d7aa26df527))
* **data-availability-fetcher:** prevent EN database from being populated with unnecessary inclusion data ([#3742](https://github.com/matter-labs/zksync-era/issues/3742)) ([037bac3](https://github.com/matter-labs/zksync-era/commit/037bac34fd59fb484025ebb91f36d379f344d667))
* make eigenda_eth_rpc in Eigen config optional in file-based configs ([#3732](https://github.com/matter-labs/zksync-era/issues/3732)) ([69d0efc](https://github.com/matter-labs/zksync-era/commit/69d0efc08bd9ab5cd6384e7c6940416988b3c2dd))
* make secret optionnal when da client object store is selected ([#3715](https://github.com/matter-labs/zksync-era/issues/3715)) ([f05fffd](https://github.com/matter-labs/zksync-era/commit/f05fffda72393fd86c752e88b7192cc8e0c30b68))
* Serialization issues ([#3589](https://github.com/matter-labs/zksync-era/issues/3589)) ([606d5af](https://github.com/matter-labs/zksync-era/commit/606d5af4641f8940f075dda091fb49e25a642ab9))
* **vm:** Fix another VM divergence in validation ([#3675](https://github.com/matter-labs/zksync-era/issues/3675)) ([85dfc13](https://github.com/matter-labs/zksync-era/commit/85dfc132c247f15920171e3dc9e87a21d7572d31))


### Performance Improvements

* **zkos:** Choose optimal amortization radix for ZK OS Merkle tree ([#3685](https://github.com/matter-labs/zksync-era/issues/3685)) ([fc1e230](https://github.com/matter-labs/zksync-era/commit/fc1e230a6a9dc0ffce5cc53b5b4a1caf64644d90))

## [26.7.0](https://github.com/matter-labs/zksync-era/compare/core-v26.6.0...core-v26.7.0) (2025-03-06)


### Features

* **api:** add endpoint supports_unsafe_deposit_filter ([#3679](https://github.com/matter-labs/zksync-era/issues/3679)) ([3902df3](https://github.com/matter-labs/zksync-era/commit/3902df39107c05ba9c737281f5975562132de9e5))
* **zkos:** Merkle tree follow-ups ([#3662](https://github.com/matter-labs/zksync-era/issues/3662)) ([7c7c24c](https://github.com/matter-labs/zksync-era/commit/7c7c24c0736fe825d3be8d4d30f2d77c59b3d050))
* **zksync_server:** adding --only-verify-config flag ([#3666](https://github.com/matter-labs/zksync-era/issues/3666)) ([022cade](https://github.com/matter-labs/zksync-era/commit/022cade897362fc7d9257a62064e19df88da6d83))

## [26.6.0](https://github.com/matter-labs/zksync-era/compare/core-v26.5.0...core-v26.6.0) (2025-03-05)


### Features

* Add S3 implementation for object_store ([#3664](https://github.com/matter-labs/zksync-era/issues/3664)) ([a848927](https://github.com/matter-labs/zksync-era/commit/a848927082bfb1b5edcc7d5e4dc33d6f39271953))
* **api:** Add delegate call ([#3653](https://github.com/matter-labs/zksync-era/issues/3653)) ([d635851](https://github.com/matter-labs/zksync-era/commit/d635851f69cf156a0a6fcc4142b9d3bb48c566a3))
* **gateway:** dont allow v26 deposits without migration ([#3645](https://github.com/matter-labs/zksync-era/issues/3645)) ([2f9134d](https://github.com/matter-labs/zksync-era/commit/2f9134d0be7b0663d4b5f0419059036b2ccca4ba))


### Bug Fixes

* Correctly fetch transactions from mempool ([#3674](https://github.com/matter-labs/zksync-era/issues/3674)) ([07144f4](https://github.com/matter-labs/zksync-era/commit/07144f4601ce1d608ac3bdf91ace694f39e4786c))

## [26.5.0](https://github.com/matter-labs/zksync-era/compare/core-v26.4.0...core-v26.5.0) (2025-03-03)


### Features

* add a flag for stage1-&gt;stage2 validium migration ([#3562](https://github.com/matter-labs/zksync-era/issues/3562)) ([92e7895](https://github.com/matter-labs/zksync-era/commit/92e78955850e59f16967dff20122d56144509498))
* add custom DA support in external node ([#3445](https://github.com/matter-labs/zksync-era/issues/3445)) ([1a8546d](https://github.com/matter-labs/zksync-era/commit/1a8546ddcd6b126657a99f68576b2a837a4c416d))
* **contract-verifier:** add Etherscan contract verification ([#3609](https://github.com/matter-labs/zksync-era/issues/3609)) ([a4ea0f2](https://github.com/matter-labs/zksync-era/commit/a4ea0f2acae301e12338a862d6a76829899114d4))
* **da-clients:** raise Avail blob size to 1mb ([#3624](https://github.com/matter-labs/zksync-era/issues/3624)) ([0baa7ff](https://github.com/matter-labs/zksync-era/commit/0baa7ff61805e90b1eaac202ed6e26f5cacfb532))
* **eigenda:** implement eigenDA client remaining features ([#3243](https://github.com/matter-labs/zksync-era/issues/3243)) ([88fc971](https://github.com/matter-labs/zksync-era/commit/88fc9714b42e3cb81dab970ec55b2bbfe0c49f52))
* preparation for new precompiles ([#3535](https://github.com/matter-labs/zksync-era/issues/3535)) ([3c1f3fb](https://github.com/matter-labs/zksync-era/commit/3c1f3fb0f24d1c19dce52b98df521703fa1bf638))
* **tee:** add support for recoverable signatures ([#3414](https://github.com/matter-labs/zksync-era/issues/3414)) ([7241a73](https://github.com/matter-labs/zksync-era/commit/7241a73b27d0e71cbe6644a741a685bf45d11d5f))
* **zkos:** Implement ZK OS Merkle tree ([#3625](https://github.com/matter-labs/zksync-era/issues/3625)) ([331e98c](https://github.com/matter-labs/zksync-era/commit/331e98c60508e1d4fbd6135e826400ed05d8a9d1))


### Bug Fixes

* **api:** Fix pending transactions filter again ([#3630](https://github.com/matter-labs/zksync-era/issues/3630)) ([7afa20f](https://github.com/matter-labs/zksync-era/commit/7afa20f74ee54af0b49e2f42f7e673de26a07e4f))
* **api:** lock simultaneous tx insertsion with mutex ([#3616](https://github.com/matter-labs/zksync-era/issues/3616)) ([644b621](https://github.com/matter-labs/zksync-era/commit/644b62144da7bc1d12190c6e7cd2863aaaae985a))
* block.timestamp is not accurate ([#3398](https://github.com/matter-labs/zksync-era/issues/3398)) ([adcb517](https://github.com/matter-labs/zksync-era/commit/adcb5172bf9aba308ae23c9031d32834be6997ea))
* Fflonk versioning ([#3610](https://github.com/matter-labs/zksync-era/issues/3610)) ([fc80840](https://github.com/matter-labs/zksync-era/commit/fc80840d355aab1e8c65ca60bd2f5cb12e0638ab))
* Limit number of connections open for GCS interactions ([#3637](https://github.com/matter-labs/zksync-era/issues/3637)) ([6b003e2](https://github.com/matter-labs/zksync-era/commit/6b003e2c3d3e576a6fedd4a05f07a46a2fe7f348))


### Performance Improvements

* **api:** Use watch channel in values cache updates ([#3663](https://github.com/matter-labs/zksync-era/issues/3663)) ([3a4bdcf](https://github.com/matter-labs/zksync-era/commit/3a4bdcf218012e1c42cb099ccdca3e6d48360f75))

## [26.4.0](https://github.com/matter-labs/zksync-era/compare/core-v26.3.0...core-v26.4.0) (2025-02-13)


### Features

* Avail gas relay upgrade ([#3601](https://github.com/matter-labs/zksync-era/issues/3601)) ([e32fee0](https://github.com/matter-labs/zksync-era/commit/e32fee04bfcfd46f4ed2edf2ea564f7aafd5db10))


### Bug Fixes

* **contract-verifier:** Fix verifier data migration ([#3608](https://github.com/matter-labs/zksync-era/issues/3608)) ([0bb0c88](https://github.com/matter-labs/zksync-era/commit/0bb0c886ad0d65fc97937680ebbc1fa6eb3d0c2c))

## [26.3.0](https://github.com/matter-labs/zksync-era/compare/core-v26.2.1...core-v26.3.0) (2025-02-12)


### Features

* **contract-verifier:** Do not allow verification requests for verified contracts ([#3578](https://github.com/matter-labs/zksync-era/issues/3578)) ([6a1f1b8](https://github.com/matter-labs/zksync-era/commit/6a1f1b801b49cec45e6e4e4b8596d866fa8fe819))
* **contract-verifier:** Partial matching & automatic verification ([#3527](https://github.com/matter-labs/zksync-era/issues/3527)) ([bf9fe85](https://github.com/matter-labs/zksync-era/commit/bf9fe85f4fd1d739105e7b21d0eebb377f752bac))
* **contract-verifier:** Support missing options for EVM in API ([#3592](https://github.com/matter-labs/zksync-era/issues/3592)) ([309fdf4](https://github.com/matter-labs/zksync-era/commit/309fdf43e93ed7a584c46227b0a3c088d658887d))
* **en:** better EN default req entities limit, improved documentation for API limits ([#3546](https://github.com/matter-labs/zksync-era/issues/3546)) ([e7eb716](https://github.com/matter-labs/zksync-era/commit/e7eb716c241c8bf224361fee150f9d1fe3023ebb))
* make `zksync_types` thinner ([#3574](https://github.com/matter-labs/zksync-era/issues/3574)) ([e7f93e4](https://github.com/matter-labs/zksync-era/commit/e7f93e43dd55674a1442111cc1f08c9d229d3e22))
* new da_dispatcher metrics ([#3464](https://github.com/matter-labs/zksync-era/issues/3464)) ([75a7c08](https://github.com/matter-labs/zksync-era/commit/75a7c08868e5f794be0c50b012164fcba5846f08))
* update FFLONK protocol version ([#3572](https://github.com/matter-labs/zksync-era/issues/3572)) ([a352852](https://github.com/matter-labs/zksync-era/commit/a3528522988093fbd2697b8fc35eb24f00166699))
* **vm:** Allow caching signature verification ([#3505](https://github.com/matter-labs/zksync-era/issues/3505)) ([7bb5ed3](https://github.com/matter-labs/zksync-era/commit/7bb5ed377719227f5c9861231e110dd9a5bb2ac0))
* **vm:** Support missed storage invocation limit in fast VM ([#3548](https://github.com/matter-labs/zksync-era/issues/3548)) ([ef67694](https://github.com/matter-labs/zksync-era/commit/ef67694b3dfb45c6f003c01d9c171236c7e1edc1))


### Bug Fixes

* Add debug information to object store retries ([#3576](https://github.com/matter-labs/zksync-era/issues/3576)) ([036315c](https://github.com/matter-labs/zksync-era/commit/036315cd560acf8a7c6b9b288bb77f7fe336947a))
* allow configuring NoDA client via ENV ([#3599](https://github.com/matter-labs/zksync-era/issues/3599)) ([a72ab63](https://github.com/matter-labs/zksync-era/commit/a72ab63e6c35412e8feb8def78099dd5b202950e))
* **api:** Change `contractAddress` assignment for transaction receipts ([#3452](https://github.com/matter-labs/zksync-era/issues/3452)) ([4179711](https://github.com/matter-labs/zksync-era/commit/4179711c82c210e0a1236c37a2a97fb9311ef820))
* **api:** Improve estimation for gas_per_pubdata_limit ([#3475](https://github.com/matter-labs/zksync-era/issues/3475)) ([bda1b25](https://github.com/matter-labs/zksync-era/commit/bda1b25c67aed2d5cb7f7a1cc6eb0afc533c83b7))
* Avail gas relay decoding issues ([#3547](https://github.com/matter-labs/zksync-era/issues/3547)) ([a171433](https://github.com/matter-labs/zksync-era/commit/a17143307ec3db898cf145665392d0a1530c17d0))
* **ci:** commenting out getFilterChanges test until fix is ready ([#3582](https://github.com/matter-labs/zksync-era/issues/3582)) ([99c3905](https://github.com/matter-labs/zksync-era/commit/99c3905a9e92416e76d37b0858da7f6c7e123e0b))
* Support newer versions of foundry-zksync ([#3556](https://github.com/matter-labs/zksync-era/issues/3556)) ([d39fb6d](https://github.com/matter-labs/zksync-era/commit/d39fb6da7e7c19a7435a4f59d0e2eb9361db218f))
* **vm:** Fix VM divergences related to validation ([#3567](https://github.com/matter-labs/zksync-era/issues/3567)) ([170d194](https://github.com/matter-labs/zksync-era/commit/170d194fb0c629c61277e877a4493d52c3153c63))


### Performance Improvements

* **db:** Remove `events.tx_initiator_address` writes and index ([#3559](https://github.com/matter-labs/zksync-era/issues/3559)) ([298abd2](https://github.com/matter-labs/zksync-era/commit/298abd2f988229a7fe5ee77de1015940d1d9ad41))

## [26.2.1](https://github.com/matter-labs/zksync-era/compare/core-v26.2.0...core-v26.2.1) (2025-01-28)


### Bug Fixes

* add . to readme ([#3538](https://github.com/matter-labs/zksync-era/issues/3538)) ([512dd45](https://github.com/matter-labs/zksync-era/commit/512dd459307e57762dd4cc2c78ff4151634b6941))

## [26.2.0](https://github.com/matter-labs/zksync-era/compare/core-v26.1.0...core-v26.2.0) (2025-01-24)


### Features

* Compressor optimizations ([#3476](https://github.com/matter-labs/zksync-era/issues/3476)) ([3e931be](https://github.com/matter-labs/zksync-era/commit/3e931be6bddaacbd7d029c537db03a3c191fdc21))


### Bug Fixes

* **en:** better defaults, i.e. the same as used by main node ([#3521](https://github.com/matter-labs/zksync-era/issues/3521)) ([2b5fe98](https://github.com/matter-labs/zksync-era/commit/2b5fe983acf78f73fb6e90a6a7d041e8aef1c595))
* **en:** Fix race condition in EN storage initialization ([#3515](https://github.com/matter-labs/zksync-era/issues/3515)) ([c916797](https://github.com/matter-labs/zksync-era/commit/c916797d49d636c9e642264786d4124ebd338ec3))
* JSON proof serialization ([#3514](https://github.com/matter-labs/zksync-era/issues/3514)) ([516e521](https://github.com/matter-labs/zksync-era/commit/516e5210ed70b25a15a68a58c8065331aab542e0))

## [26.1.0](https://github.com/matter-labs/zksync-era/compare/core-v26.0.0...core-v26.1.0) (2025-01-21)


### Features

* update l2 erc20 bridge address in updater as well ([#3500](https://github.com/matter-labs/zksync-era/issues/3500)) ([fe3c7b2](https://github.com/matter-labs/zksync-era/commit/fe3c7b2583bc4f9277e186334e5822ddf95bdcd0))
* **vm:** Implement call tracing for fast VM ([#2905](https://github.com/matter-labs/zksync-era/issues/2905)) ([731b824](https://github.com/matter-labs/zksync-era/commit/731b8240abd4c0cfa42f2ce89c23f8ebf67e1bf2))


### Bug Fixes

* copy special case to fast VM call tracer ([#3509](https://github.com/matter-labs/zksync-era/issues/3509)) ([995e583](https://github.com/matter-labs/zksync-era/commit/995e583aa9b4ef6e0d8697fbb040e4b991a4248d))
* fix execute encoding for transactions ([#3501](https://github.com/matter-labs/zksync-era/issues/3501)) ([4c381a8](https://github.com/matter-labs/zksync-era/commit/4c381a84346f8ab88d3f01dc2848c7fb5f2b788d))
* **gateway:** erc20 workaround for gateway upgrade ([#3511](https://github.com/matter-labs/zksync-era/issues/3511)) ([c140ba8](https://github.com/matter-labs/zksync-era/commit/c140ba8f57caabf9c9bdd4bd8c9743a9ccf668be))


### Performance Improvements

* optimize get_unsealed_l1_batch_inner ([#3491](https://github.com/matter-labs/zksync-era/issues/3491)) ([9b121c9](https://github.com/matter-labs/zksync-era/commit/9b121c96bbb2e53be74aa81e0ca250ce9251f8db))

## [26.0.0](https://github.com/matter-labs/zksync-era/compare/core-v25.4.0...core-v26.0.0) (2025-01-17)


### ⚠ BREAKING CHANGES

* **contracts:** gateway integration ([#1934](https://github.com/matter-labs/zksync-era/issues/1934))

### Features

* Adapt server for new EVM bytecode hash encoding ([#3396](https://github.com/matter-labs/zksync-era/issues/3396)) ([5a1e6d2](https://github.com/matter-labs/zksync-era/commit/5a1e6d2445d4d4310fc1e54ccd44dc4254e5bcbc))
* Add logging & metrics for mempool ([#3447](https://github.com/matter-labs/zksync-era/issues/3447)) ([64d861d](https://github.com/matter-labs/zksync-era/commit/64d861d1e1d2d46339938ee3174c58cdc3f348c3))
* **api_server:** report gas price based on open batch ([#2868](https://github.com/matter-labs/zksync-era/issues/2868)) ([f30aca0](https://github.com/matter-labs/zksync-era/commit/f30aca00962aa34c8a7acd6e4116290a2b214dcb))
* **contracts:** gateway integration ([#1934](https://github.com/matter-labs/zksync-era/issues/1934)) ([f06cb79](https://github.com/matter-labs/zksync-era/commit/f06cb79883bf320f50089099e0abeb95eaace470))
* da_dispatcher refactoring ([#3409](https://github.com/matter-labs/zksync-era/issues/3409)) ([591cd86](https://github.com/matter-labs/zksync-era/commit/591cd86a1a1e6e4214d3cec74b4c601356060203))
* **en:** make documentation more chain agnostic ([#3376](https://github.com/matter-labs/zksync-era/issues/3376)) ([361243f](https://github.com/matter-labs/zksync-era/commit/361243f3f15e01cf1f3e49b73a579cb962cf0124))
* **eth-sender:** make base fee grow at least as fast as priority fee ([#3386](https://github.com/matter-labs/zksync-era/issues/3386)) ([78af2bf](https://github.com/matter-labs/zksync-era/commit/78af2bf786bb4f7a639fef9fd169594101818b79))
* **eth-watch:** Change protocol upgrade schema ([#3435](https://github.com/matter-labs/zksync-era/issues/3435)) ([2c778fd](https://github.com/matter-labs/zksync-era/commit/2c778fdd3fcd1e774bcb945f14a640ccf4227a2f))
* Features for an easier upgrade ([#3422](https://github.com/matter-labs/zksync-era/issues/3422)) ([3037ee6](https://github.com/matter-labs/zksync-era/commit/3037ee6aa976744a09882b5830d6242ad8336717))
* FFLONK support for compressor ([#3359](https://github.com/matter-labs/zksync-era/issues/3359)) ([1a297be](https://github.com/matter-labs/zksync-era/commit/1a297bedd226c56fc2ba02dc54d79129a271a1eb))
* pubdata type changes from sync-layer-stable ([#3425](https://github.com/matter-labs/zksync-era/issues/3425)) ([f09087b](https://github.com/matter-labs/zksync-era/commit/f09087bab397778976af42c321cbba93f9706b5a))


### Bug Fixes

* **api:** Propagate fallback errors in traces ([#3469](https://github.com/matter-labs/zksync-era/issues/3469)) ([84e3e31](https://github.com/matter-labs/zksync-era/commit/84e3e312688e3aaffe81828471d276e24432d496))
* **en:** make EN use main node's fee input ([#3489](https://github.com/matter-labs/zksync-era/issues/3489)) ([cbf2c31](https://github.com/matter-labs/zksync-era/commit/cbf2c31e353fd7a5167fcca7e2df87026050c21a))
* eth aggregator restriction ([#3490](https://github.com/matter-labs/zksync-era/issues/3490)) ([6cc9b9e](https://github.com/matter-labs/zksync-era/commit/6cc9b9e405b03a7e30f3c92735b7452099c165d0))


### Performance Improvements

* **eth-sender:** optimize sql query ([#3437](https://github.com/matter-labs/zksync-era/issues/3437)) ([0731f60](https://github.com/matter-labs/zksync-era/commit/0731f607a72d18decd1ff74139f190c253d807ef))

## [25.4.0](https://github.com/matter-labs/zksync-era/compare/core-v25.3.0...core-v25.4.0) (2024-12-19)


### Features

* add support for custom genesis state ([#3259](https://github.com/matter-labs/zksync-era/issues/3259)) ([3cffdb2](https://github.com/matter-labs/zksync-era/commit/3cffdb2d5e144f2e3d8617fa22aacf6cce5998a2))
* **consensus:** Added view_timeout to consensus config ([#3383](https://github.com/matter-labs/zksync-era/issues/3383)) ([fc02a8f](https://github.com/matter-labs/zksync-era/commit/fc02a8f1c9f0bffb438fb27769d6dced3ce14cd9))
* Support stable compiler for VM (and some other crates) ([#3248](https://github.com/matter-labs/zksync-era/issues/3248)) ([cbee99d](https://github.com/matter-labs/zksync-era/commit/cbee99d8661b38aa6b49784c3934b8070a743fb4))
* vm2 account validation ([#2863](https://github.com/matter-labs/zksync-era/issues/2863)) ([af149a0](https://github.com/matter-labs/zksync-era/commit/af149a01e6ce0c62d4b8a6acf9481e807ac24a8f))


### Bug Fixes

* **contract-verifier:** Fix version extraction in gh resolver ([#3378](https://github.com/matter-labs/zksync-era/issues/3378)) ([9a10dcf](https://github.com/matter-labs/zksync-era/commit/9a10dcf764e25c4e60b7ae5ddfa728c9cf576248))

## [25.3.0](https://github.com/matter-labs/zksync-era/compare/core-v25.2.0...core-v25.3.0) (2024-12-11)


### Features

* change seal criteria for gateway ([#3320](https://github.com/matter-labs/zksync-era/issues/3320)) ([a0a74aa](https://github.com/matter-labs/zksync-era/commit/a0a74aaeb42f076d20c4ae8a32925eff2de11d0c))
* **contract-verifier:** Download compilers from GH automatically ([#3291](https://github.com/matter-labs/zksync-era/issues/3291)) ([a10c4ba](https://github.com/matter-labs/zksync-era/commit/a10c4baa312f26ebac2a10115fb7bd314d18b9c1))
* integrate gateway changes for some components ([#3274](https://github.com/matter-labs/zksync-era/issues/3274)) ([cbc91e3](https://github.com/matter-labs/zksync-era/commit/cbc91e35f84d04f2e4c8e81028596db009e478d1))
* **proof-data-handler:** exclude batches without object file in GCS  ([#2980](https://github.com/matter-labs/zksync-era/issues/2980)) ([3e309e0](https://github.com/matter-labs/zksync-era/commit/3e309e06b24649c74bfe120e8ca45247cb2b5628))
* **pruning:** Record L1 batch root hash in pruning logs ([#3266](https://github.com/matter-labs/zksync-era/issues/3266)) ([7b6e590](https://github.com/matter-labs/zksync-era/commit/7b6e59083cf0cafeaef5dd4b2dd39257ff91316d))
* **state-keeper:** mempool io opens batch if there is protocol upgrade tx ([#3360](https://github.com/matter-labs/zksync-era/issues/3360)) ([f6422cd](https://github.com/matter-labs/zksync-era/commit/f6422cd59dab2c105bb7c125c172f2621fe39464))
* **tee:** add error handling for unstable_getTeeProofs API endpoint ([#3321](https://github.com/matter-labs/zksync-era/issues/3321)) ([26f630c](https://github.com/matter-labs/zksync-era/commit/26f630cb75958c711d67d13bc77ddbb1117156c3))
* **zksync_cli:** Health checkpoint improvements ([#3193](https://github.com/matter-labs/zksync-era/issues/3193)) ([440fe8d](https://github.com/matter-labs/zksync-era/commit/440fe8d8afdf0fc2768692a1b40b0910873e2faf))


### Bug Fixes

* **api:** batch fee input scaling for `debug_traceCall` ([#3344](https://github.com/matter-labs/zksync-era/issues/3344)) ([7ace594](https://github.com/matter-labs/zksync-era/commit/7ace594fb3140212bd94ffd6bffcac99805cf4b1))
* **tee:** correct previous fix for race condition in batch locking ([#3358](https://github.com/matter-labs/zksync-era/issues/3358)) ([b12da8d](https://github.com/matter-labs/zksync-era/commit/b12da8d1fddc7870bf17d5e08312d20773815269))
* **tee:** fix race condition in batch locking ([#3342](https://github.com/matter-labs/zksync-era/issues/3342)) ([a7dc0ed](https://github.com/matter-labs/zksync-era/commit/a7dc0ed5007f6b2f789f4c61cb3d137843151860))
* **tracer:** adds vm error to flatCallTracer error field if exists ([#3374](https://github.com/matter-labs/zksync-era/issues/3374)) ([5d77727](https://github.com/matter-labs/zksync-era/commit/5d77727cd3ba5f4d84643fee1873f03656310b4d))

## [25.2.0](https://github.com/matter-labs/zksync-era/compare/core-v25.1.0...core-v25.2.0) (2024-11-19)


### Features

* add more metrics for the tee_prover ([#3276](https://github.com/matter-labs/zksync-era/issues/3276)) ([8b62434](https://github.com/matter-labs/zksync-era/commit/8b62434a3b48aea2e66b5dd833f52c58e26969cb))
* **api-server:** add `yParity` for non-legacy txs ([#3246](https://github.com/matter-labs/zksync-era/issues/3246)) ([6ea36d1](https://github.com/matter-labs/zksync-era/commit/6ea36d14940a19f638512556ccc4c5127150b5c9))
* **consensus:** fallback json rpc syncing for consensus ([#3211](https://github.com/matter-labs/zksync-era/issues/3211)) ([726203b](https://github.com/matter-labs/zksync-era/commit/726203bab540e3d6ada10b6bc12bd3c09220d895))
* **contract-verifier:** Adapt contract verifier API for EVM bytecodes ([#3234](https://github.com/matter-labs/zksync-era/issues/3234)) ([4509179](https://github.com/matter-labs/zksync-era/commit/4509179f62ead4b837dfb67760f52de76fac2e37))
* **contract-verifier:** Support Solidity contracts with EVM bytecode in contract verifier ([#3225](https://github.com/matter-labs/zksync-era/issues/3225)) ([8a3a82c](https://github.com/matter-labs/zksync-era/commit/8a3a82ca16479183e96505bc91011fc07bfc6889))
* **contract-verifier:** Support Vyper toolchain for EVM bytecodes ([#3251](https://github.com/matter-labs/zksync-era/issues/3251)) ([75f7db9](https://github.com/matter-labs/zksync-era/commit/75f7db9b535b4dee4c6662be609aec996555383c))
* **en:** Support Merkle tree recovery with pruning enabled ([#3172](https://github.com/matter-labs/zksync-era/issues/3172)) ([7b8640a](https://github.com/matter-labs/zksync-era/commit/7b8640a89fa8666e14934481317c94f07280e591))
* ProverJobProcessor & circuit prover ([#3287](https://github.com/matter-labs/zksync-era/issues/3287)) ([98823f9](https://github.com/matter-labs/zksync-era/commit/98823f95c0b95feeb37eb9086cc88d4ac5220904))
* **prover:** Move prover_autoscaler config into crate ([#3222](https://github.com/matter-labs/zksync-era/issues/3222)) ([1b33b5e](https://github.com/matter-labs/zksync-era/commit/1b33b5e9ec04bea0010350798332a90413c482d3))
* **vm_executor:** Add new histogram metric for gas per tx in vm_executor ([#3215](https://github.com/matter-labs/zksync-era/issues/3215)) ([3606fc1](https://github.com/matter-labs/zksync-era/commit/3606fc1d8f103b4f7174301f9a985ace2b89038d))
* **vm:** add gateway changes to fast vm ([#3236](https://github.com/matter-labs/zksync-era/issues/3236)) ([f3a2517](https://github.com/matter-labs/zksync-era/commit/f3a2517a132b036ca70bc18aa8ac9f6da1cbc049))


### Bug Fixes

* **merkle-tree:** Repair stale keys for tree in background ([#3200](https://github.com/matter-labs/zksync-era/issues/3200)) ([363b4f0](https://github.com/matter-labs/zksync-era/commit/363b4f09937496fadeb38857f5c0c73146995ce5))
* **tracer:** Add error to flat tracer ([#3306](https://github.com/matter-labs/zksync-era/issues/3306)) ([7c93c47](https://github.com/matter-labs/zksync-era/commit/7c93c47916845a90fc5a092e1465567aae611307))
* use_dummy_inclusion_data condition ([#3244](https://github.com/matter-labs/zksync-era/issues/3244)) ([6e3c36e](https://github.com/matter-labs/zksync-era/commit/6e3c36e6426621bee82399db7814ca6756b613cb))
* **vm:** Do not require experimental VM config ([#3270](https://github.com/matter-labs/zksync-era/issues/3270)) ([54e4b00](https://github.com/matter-labs/zksync-era/commit/54e4b007b2d32d86b2701b01cd3bef3b3bc97087))

## [25.1.0](https://github.com/matter-labs/zksync-era/compare/core-v25.0.0...core-v25.1.0) (2024-11-04)


### Features

* add `block.timestamp` asserter for AA ([#3031](https://github.com/matter-labs/zksync-era/issues/3031)) ([069d38d](https://github.com/matter-labs/zksync-era/commit/069d38d6c9ddd8b6c404596c479f94b9fc86db40))
* allow vm2 tracers to stop execution ([#3183](https://github.com/matter-labs/zksync-era/issues/3183)) ([9dae839](https://github.com/matter-labs/zksync-era/commit/9dae839935d82a1e73be220d17567f3382131039))
* **api:** get rid of tx receipt root ([#3187](https://github.com/matter-labs/zksync-era/issues/3187)) ([6c034f6](https://github.com/matter-labs/zksync-era/commit/6c034f6e180cc92e99766f14c8840c90efa56cec))
* **api:** Integrate new VM into API server (no tracers) ([#3033](https://github.com/matter-labs/zksync-era/issues/3033)) ([8e75d4b](https://github.com/matter-labs/zksync-era/commit/8e75d4b812b21bc26e2c38ceeb711a8a530d7bc2))
* base token integration tests ([#2509](https://github.com/matter-labs/zksync-era/issues/2509)) ([8db7e93](https://github.com/matter-labs/zksync-era/commit/8db7e9306e5fa23f066be106363e6455531bbc09))
* **consensus:** enabled syncing pregenesis blocks over p2p ([#3192](https://github.com/matter-labs/zksync-era/issues/3192)) ([6adb224](https://github.com/matter-labs/zksync-era/commit/6adb2249ff0946ec6d02f25437c9f71b1079ad79))
* **da-clients:** add Celestia client ([#2983](https://github.com/matter-labs/zksync-era/issues/2983)) ([d88b875](https://github.com/matter-labs/zksync-era/commit/d88b875464ec5ac7e54aba0cc7c0a68c01969782))
* **da-clients:** add EigenDA client ([#3155](https://github.com/matter-labs/zksync-era/issues/3155)) ([5161eed](https://github.com/matter-labs/zksync-era/commit/5161eeda5905d33f4d038a2a04ced3e06f39d593))
* gateway preparation ([#3006](https://github.com/matter-labs/zksync-era/issues/3006)) ([16f2757](https://github.com/matter-labs/zksync-era/commit/16f275756cd28024a6b11ac1ac327eb5b8b446e1))
* Implement gas relay mode and inclusion data for data attestation ([#3070](https://github.com/matter-labs/zksync-era/issues/3070)) ([561fc1b](https://github.com/matter-labs/zksync-era/commit/561fc1bddfc79061dab9d8d150baa06acfa90692))
* **metadata-calculator:** Add debug endpoints for tree API ([#3167](https://github.com/matter-labs/zksync-era/issues/3167)) ([3815252](https://github.com/matter-labs/zksync-era/commit/3815252790fd0e9094f308b58dfde3a8b1a82277))
* **proof-data-handler:** add first processed batch option ([#3112](https://github.com/matter-labs/zksync-era/issues/3112)) ([1eb69d4](https://github.com/matter-labs/zksync-era/commit/1eb69d467802d07f3fc6502de97ff04a69f952fc))
* **proof-data-handler:** add tee_proof_generation_timeout_in_secs param ([#3128](https://github.com/matter-labs/zksync-era/issues/3128)) ([f3724a7](https://github.com/matter-labs/zksync-era/commit/f3724a71c7466451d380981b05d68d8afd70cdca))
* **prover:** Add queue metric to report autoscaler view of the queue. ([#3206](https://github.com/matter-labs/zksync-era/issues/3206)) ([2721396](https://github.com/matter-labs/zksync-era/commit/272139690e028d3bdebdb6bcb1824fec23cefd0f))
* **prover:** Add sending scale requests for Scaler targets ([#3194](https://github.com/matter-labs/zksync-era/issues/3194)) ([767c5bc](https://github.com/matter-labs/zksync-era/commit/767c5bc6a62c402c099abe93b7dbecbb59e4acb7))
* **prover:** Add support for scaling WGs and compressor ([#3179](https://github.com/matter-labs/zksync-era/issues/3179)) ([c41db9e](https://github.com/matter-labs/zksync-era/commit/c41db9ecec1c21b80969604f703ac6990f6f3434))
* **vm:** Support EVM emulation in fast VM ([#3163](https://github.com/matter-labs/zksync-era/issues/3163)) ([9ad1f0d](https://github.com/matter-labs/zksync-era/commit/9ad1f0d77e5a5b411f7866ef6a1819373c07f91b))


### Bug Fixes

* **consensus:** better logging of errors ([#3170](https://github.com/matter-labs/zksync-era/issues/3170)) ([a5028da](https://github.com/matter-labs/zksync-era/commit/a5028da65608898ad41c6a4fd5c6ec4c28a45703))
* **consensus:** made attestation controller non-critical ([#3180](https://github.com/matter-labs/zksync-era/issues/3180)) ([6ee9f1f](https://github.com/matter-labs/zksync-era/commit/6ee9f1f431f95514d58db87a4562e09df9d09f86))
* **consensus:** payload encoding protected by protocol_version ([#3168](https://github.com/matter-labs/zksync-era/issues/3168)) ([8089b78](https://github.com/matter-labs/zksync-era/commit/8089b78b3f2cdbe8d0a23e9b8412a8022d78ada2))
* **da-clients:** add padding to the data within EigenDA blob ([#3203](https://github.com/matter-labs/zksync-era/issues/3203)) ([8ae06b2](https://github.com/matter-labs/zksync-era/commit/8ae06b237647715937fb3656d881c0fd460f2a07))
* **da-clients:** enable tls-roots feature for tonic ([#3201](https://github.com/matter-labs/zksync-era/issues/3201)) ([42f177a](https://github.com/matter-labs/zksync-era/commit/42f177ac43b86cd24321ad9222121fc8a91c49e0))
* extend allowed storage slots for validation as per EIP-7562 ([#3166](https://github.com/matter-labs/zksync-era/issues/3166)) ([c76da16](https://github.com/matter-labs/zksync-era/commit/c76da16efc769243a02c6e859376182d95ab941d))
* **merkle-tree:** Fix tree truncation ([#3178](https://github.com/matter-labs/zksync-era/issues/3178)) ([9654097](https://github.com/matter-labs/zksync-era/commit/96540975d917761d8e464ebbdf52704955bcd898))
* **tee_prover:** add prometheus pull listener ([#3169](https://github.com/matter-labs/zksync-era/issues/3169)) ([1ffd22f](https://github.com/matter-labs/zksync-era/commit/1ffd22ffbe710469de0e7f27c6aae29453ec6d3e))
* update logging in cbt l1 behaviour ([#3149](https://github.com/matter-labs/zksync-era/issues/3149)) ([d0f61b0](https://github.com/matter-labs/zksync-era/commit/d0f61b0552dcacc2e8e33fdbcae6f1e5fbb43820))

## [25.0.0](https://github.com/matter-labs/zksync-era/compare/core-v24.29.0...core-v25.0.0) (2024-10-23)


### ⚠ BREAKING CHANGES

* **contracts:** integrate protocol defense changes ([#2737](https://github.com/matter-labs/zksync-era/issues/2737))

### Features

* Add CoinMarketCap external API ([#2971](https://github.com/matter-labs/zksync-era/issues/2971)) ([c1cb30e](https://github.com/matter-labs/zksync-era/commit/c1cb30e59ca1d0b5fea5fe0980082aea0eb04aa2))
* **api:** Implement eth_maxPriorityFeePerGas ([#3135](https://github.com/matter-labs/zksync-era/issues/3135)) ([35e84cc](https://github.com/matter-labs/zksync-era/commit/35e84cc03a7fdd315932fb3020fe41c95a6e4bca))
* **api:** Make acceptable values cache lag configurable ([#3028](https://github.com/matter-labs/zksync-era/issues/3028)) ([6747529](https://github.com/matter-labs/zksync-era/commit/67475292ff770d2edd6884be27f976a4144778ae))
* **contracts:** integrate protocol defense changes ([#2737](https://github.com/matter-labs/zksync-era/issues/2737)) ([c60a348](https://github.com/matter-labs/zksync-era/commit/c60a3482ee09b3e371163e62f49e83bc6d6f4548))
* **external-node:** save protocol version before opening a batch ([#3136](https://github.com/matter-labs/zksync-era/issues/3136)) ([d6de4f4](https://github.com/matter-labs/zksync-era/commit/d6de4f40ddce339c760c95e2bf4b8aceb571af7f))
* Prover e2e test ([#2975](https://github.com/matter-labs/zksync-era/issues/2975)) ([0edd796](https://github.com/matter-labs/zksync-era/commit/0edd7962429b3530ae751bd7cc947c97193dd0ca))
* **prover:** Add min_provers and dry_run features. Improve metrics and test. ([#3129](https://github.com/matter-labs/zksync-era/issues/3129)) ([7c28964](https://github.com/matter-labs/zksync-era/commit/7c289649b7b3c418c7193a35b51c264cf4970f3c))
* **tee_verifier:** speedup SQL query for new jobs ([#3133](https://github.com/matter-labs/zksync-era/issues/3133)) ([30ceee8](https://github.com/matter-labs/zksync-era/commit/30ceee8a48046e349ff0234ebb24d468a0e0876c))
* vm2 tracers can access storage ([#3114](https://github.com/matter-labs/zksync-era/issues/3114)) ([e466b52](https://github.com/matter-labs/zksync-era/commit/e466b52948e3c4ed1cb5af4fd999a52028e4d216))
* **vm:** Return compressed bytecodes from `push_transaction()` ([#3126](https://github.com/matter-labs/zksync-era/issues/3126)) ([37f209f](https://github.com/matter-labs/zksync-era/commit/37f209fec8e7cb65c0e60003d46b9ea69c43caf1))


### Bug Fixes

* **call_tracer:** Flat call tracer fixes for blocks ([#3095](https://github.com/matter-labs/zksync-era/issues/3095)) ([30ddb29](https://github.com/matter-labs/zksync-era/commit/30ddb292977340beab37a81f75c35480cbdd59d3))
* **consensus:** preventing config update reverts ([#3148](https://github.com/matter-labs/zksync-era/issues/3148)) ([caee55f](https://github.com/matter-labs/zksync-era/commit/caee55fef4eed0ec58cceaeba277bbdedf5c6f51))
* **en:** Return `SyncState` health check ([#3142](https://github.com/matter-labs/zksync-era/issues/3142)) ([abeee81](https://github.com/matter-labs/zksync-era/commit/abeee8190d3c3a5e577d71024bdfb30ff516ad03))
* **external-node:** delete empty unsealed batch on EN initialization ([#3125](https://github.com/matter-labs/zksync-era/issues/3125)) ([5d5214b](https://github.com/matter-labs/zksync-era/commit/5d5214ba983823b306495d34fdd1d46abacce07a))
* Fix counter metric type to be Counter. ([#3153](https://github.com/matter-labs/zksync-era/issues/3153)) ([08a3fe7](https://github.com/matter-labs/zksync-era/commit/08a3fe7ffd0410c51334193068649905337d5e84))
* **mempool:** minor mempool improvements ([#3113](https://github.com/matter-labs/zksync-era/issues/3113)) ([cd16083](https://github.com/matter-labs/zksync-era/commit/cd160830a0b7ebe5af4ecbd944da1cd51af3528a))
* **prover:** Run for zero queue to allow scaling down to 0 ([#3115](https://github.com/matter-labs/zksync-era/issues/3115)) ([bbe1919](https://github.com/matter-labs/zksync-era/commit/bbe191937fa5c5711a7164fd4f0c2ae65cda0833))
* restore instruction count functionality ([#3081](https://github.com/matter-labs/zksync-era/issues/3081)) ([6159f75](https://github.com/matter-labs/zksync-era/commit/6159f7531a0340a69c4926c4e0325811ed7cabb8))
* **state-keeper:** save call trace for upgrade txs ([#3132](https://github.com/matter-labs/zksync-era/issues/3132)) ([e1c363f](https://github.com/matter-labs/zksync-era/commit/e1c363f8f5e03c8d62bba1523f17b87d6a0e25ad))
* **tee_prover:** add zstd compression ([#3144](https://github.com/matter-labs/zksync-era/issues/3144)) ([7241ae1](https://github.com/matter-labs/zksync-era/commit/7241ae139b2b6bf9a9966eaa2f22203583a3786f))
* **tee_verifier:** correctly initialize storage for re-execution ([#3017](https://github.com/matter-labs/zksync-era/issues/3017)) ([9d88373](https://github.com/matter-labs/zksync-era/commit/9d88373f1b745c489e98e5ef542644a70e815498))

## [24.29.0](https://github.com/matter-labs/zksync-era/compare/core-v24.28.0...core-v24.29.0) (2024-10-14)


### Features

* Add  initial version prover_autoscaler ([#2993](https://github.com/matter-labs/zksync-era/issues/2993)) ([ebf9604](https://github.com/matter-labs/zksync-era/commit/ebf9604c5ab2a1cae1ffd2f9c922f35a1d0ad876))
* add metric to track current cbt ratio ([#3020](https://github.com/matter-labs/zksync-era/issues/3020)) ([3fd2fb1](https://github.com/matter-labs/zksync-era/commit/3fd2fb14e7283c6858731e162522e70051a8e162))
* **configs:** Add port parameter to ConsensusConfig ([#2986](https://github.com/matter-labs/zksync-era/issues/2986)) ([25112df](https://github.com/matter-labs/zksync-era/commit/25112df39d052f083bc45964f0298b3af5842cac))
* **configs:** Add port parameter to ConsensusConfig ([#3051](https://github.com/matter-labs/zksync-era/issues/3051)) ([038c397](https://github.com/matter-labs/zksync-era/commit/038c397ce842601da5109c460b09dbf9d51cf2fc))
* **consensus:** smooth transition to p2p syncing (BFT-515) ([#3075](https://github.com/matter-labs/zksync-era/issues/3075)) ([5d339b4](https://github.com/matter-labs/zksync-era/commit/5d339b46fee66bc3a45493586626d318380680dd))
* **consensus:** Support for syncing blocks before consensus genesis over p2p network ([#3040](https://github.com/matter-labs/zksync-era/issues/3040)) ([d3edc3d](https://github.com/matter-labs/zksync-era/commit/d3edc3d817c151ed00d4fa822fdae0a746e33356))
* **en:** periodically fetch bridge addresses ([#2949](https://github.com/matter-labs/zksync-era/issues/2949)) ([e984bfb](https://github.com/matter-labs/zksync-era/commit/e984bfb8a243bc746549ab9347dc0a367fe02790))
* **eth-sender:** add time_in_mempool_cap config ([#3018](https://github.com/matter-labs/zksync-era/issues/3018)) ([f6d86bd](https://github.com/matter-labs/zksync-era/commit/f6d86bd7935a1cdbb528b13437424031fda3cb8e))
* **eth-watch:** catch another reth error ([#3026](https://github.com/matter-labs/zksync-era/issues/3026)) ([4640c42](https://github.com/matter-labs/zksync-era/commit/4640c4233af46c97f207d2dbce5fedd1bcb66c43))
* Handle new yul compilation flow ([#3038](https://github.com/matter-labs/zksync-era/issues/3038)) ([4035361](https://github.com/matter-labs/zksync-era/commit/40353616f278800dc80fcbe5f2a6483019033b20))
* **state-keeper:** pre-insert unsealed L1 batches ([#2846](https://github.com/matter-labs/zksync-era/issues/2846)) ([e5b5a3b](https://github.com/matter-labs/zksync-era/commit/e5b5a3b7b62e8d4035fe89c2a287bf3606d17bc5))
* **vm:** EVM emulator support – base ([#2979](https://github.com/matter-labs/zksync-era/issues/2979)) ([deafa46](https://github.com/matter-labs/zksync-era/commit/deafa460715334a77edf9fe8aa76fa90029342c4))
* **zk_toolbox:** added support for setting attester committee defined in a separate file ([#2992](https://github.com/matter-labs/zksync-era/issues/2992)) ([6105514](https://github.com/matter-labs/zksync-era/commit/610551427d5ab129f91e69b5efb318da917457d7))
* **zk_toolbox:** Redesign zk_toolbox commands ([#3003](https://github.com/matter-labs/zksync-era/issues/3003)) ([114834f](https://github.com/matter-labs/zksync-era/commit/114834f357421c62d596a1954fac8ce615cfde49))
* **zktoolbox:** added checking the contract owner in set-attester-committee command ([#3061](https://github.com/matter-labs/zksync-era/issues/3061)) ([9b0a606](https://github.com/matter-labs/zksync-era/commit/9b0a6067923c5276f560f3abccedc4e6a5167dda))


### Bug Fixes

* **api:** Accept integer block count in `eth_feeHistory` ([#3077](https://github.com/matter-labs/zksync-era/issues/3077)) ([4d527d4](https://github.com/matter-labs/zksync-era/commit/4d527d4b44b6b083e2a813d48c79d8021ea6f843))
* **api:** Adapt `eth_getCode` to EVM emulator ([#3073](https://github.com/matter-labs/zksync-era/issues/3073)) ([15fe5a6](https://github.com/matter-labs/zksync-era/commit/15fe5a62f03cd103afd7fa5eb03e27db25686ba9))
* bincode deserialization for VM run data ([#3044](https://github.com/matter-labs/zksync-era/issues/3044)) ([b0ec79f](https://github.com/matter-labs/zksync-era/commit/b0ec79fcb7fa120f095d987f53c67fdab92e2c79))
* bincode deserialize for WitnessInputData ([#3055](https://github.com/matter-labs/zksync-era/issues/3055)) ([91d0595](https://github.com/matter-labs/zksync-era/commit/91d0595631cc5f5bffc42a4b04d5015d2be659b1))
* **external-node:** make fetcher rely on unsealed batches  ([#3088](https://github.com/matter-labs/zksync-era/issues/3088)) ([bb5d147](https://github.com/matter-labs/zksync-era/commit/bb5d1470d5e1e8e69d9b79c60284ea8adaee4038))
* **state-keeper:** ensure unsealed batch is present during IO init ([#3071](https://github.com/matter-labs/zksync-era/issues/3071)) ([bdeb411](https://github.com/matter-labs/zksync-era/commit/bdeb411c593ac3d5e16158e64c4210bb00edcb0c))
* **vm:** Check protocol version for fast VM ([#3080](https://github.com/matter-labs/zksync-era/issues/3080)) ([a089f3f](https://github.com/matter-labs/zksync-era/commit/a089f3feb916ccc9007d9c32ec909db694b7d9f4))
* **vm:** Prepare new VM for use in API server and fix divergences ([#2994](https://github.com/matter-labs/zksync-era/issues/2994)) ([741b77e](https://github.com/matter-labs/zksync-era/commit/741b77e080f75c6a93d3ee779b1c9ce4297618f9))


### Reverts

* **configs:** Add port parameter to ConsensusConfig ([#2986](https://github.com/matter-labs/zksync-era/issues/2986)) ([#3046](https://github.com/matter-labs/zksync-era/issues/3046)) ([abe35bf](https://github.com/matter-labs/zksync-era/commit/abe35bf7aea1120b77fdbd413d927e45da48d26c))

## [24.28.0](https://github.com/matter-labs/zksync-era/compare/core-v24.27.0...core-v24.28.0) (2024-10-02)


### Features

* **da-clients:** add secrets ([#2954](https://github.com/matter-labs/zksync-era/issues/2954)) ([f4631e4](https://github.com/matter-labs/zksync-era/commit/f4631e4466de620cc1401b326d864cdb8b48a05d))
* **eth-sender:** add a cap to time_in_mempool ([#2978](https://github.com/matter-labs/zksync-era/issues/2978)) ([650d42f](https://github.com/matter-labs/zksync-era/commit/650d42fea6124d80b60a8270a303d72ad6ac741e))
* **eth-watch:** redesign to support multiple chains ([#2867](https://github.com/matter-labs/zksync-era/issues/2867)) ([aa72d84](https://github.com/matter-labs/zksync-era/commit/aa72d849c24a664acd083eba73795ddc5d31d55f))
* Expose http debug page ([#2952](https://github.com/matter-labs/zksync-era/issues/2952)) ([e0b6488](https://github.com/matter-labs/zksync-era/commit/e0b64888aae7324aec2d40fa0cd51ea7e1450cd9))
* **zk_toolbox:** add fees integration test to toolbox ([#2898](https://github.com/matter-labs/zksync-era/issues/2898)) ([e7ead76](https://github.com/matter-labs/zksync-era/commit/e7ead760ce0417dd36af3839ac557f7e9ab238a4))
* **zk_toolbox:** Add SQL format for zk supervisor ([#2950](https://github.com/matter-labs/zksync-era/issues/2950)) ([540e5d7](https://github.com/matter-labs/zksync-era/commit/540e5d7554f54e80d52f1bfae37e03ca8f787baf))


### Bug Fixes

* **api:** Fix batch fee input for `debug` namespace ([#2948](https://github.com/matter-labs/zksync-era/issues/2948)) ([79b6fcf](https://github.com/matter-labs/zksync-era/commit/79b6fcf8b5d10a0ccdceb846370dd6870b6a32b5))
* chainstack block limit exceeded ([#2974](https://github.com/matter-labs/zksync-era/issues/2974)) ([4ffbf42](https://github.com/matter-labs/zksync-era/commit/4ffbf426de166c11aaf5d7b5ed7d199644fba229))
* **eth-watch:** add missing check that from_block is not larger than finalized_block ([#2969](https://github.com/matter-labs/zksync-era/issues/2969)) ([3f406c7](https://github.com/matter-labs/zksync-era/commit/3f406c7d0c0e76d798c2d838abde57ca692822c0))
* ignore unknown fields in rpc json response ([#2962](https://github.com/matter-labs/zksync-era/issues/2962)) ([692ea73](https://github.com/matter-labs/zksync-era/commit/692ea73f75a5fb9db2b4ac33ad24d20568638742))


### Performance Improvements

* **api:** More efficient gas estimation ([#2937](https://github.com/matter-labs/zksync-era/issues/2937)) ([3b69e37](https://github.com/matter-labs/zksync-era/commit/3b69e37e470dab859a55787f6cc971e7083de2fd))

## [24.27.0](https://github.com/matter-labs/zksync-era/compare/core-v24.26.0...core-v24.27.0) (2024-09-25)


### Features

* **vm:** Split old and new VM implementations ([#2915](https://github.com/matter-labs/zksync-era/issues/2915)) ([93bc66f](https://github.com/matter-labs/zksync-era/commit/93bc66f21f9f67a440f06f1c4402e0d687698741))


### Bug Fixes

* **api:** Return correct flat call tracer ([#2917](https://github.com/matter-labs/zksync-era/issues/2917)) ([218646a](https://github.com/matter-labs/zksync-era/commit/218646aa1c56200f4ffee99b7f83366e2689354f))

## [24.26.0](https://github.com/matter-labs/zksync-era/compare/core-v24.25.0...core-v24.26.0) (2024-09-23)


### Features

* added seed_peers to consensus global config ([#2920](https://github.com/matter-labs/zksync-era/issues/2920)) ([e9d1d90](https://github.com/matter-labs/zksync-era/commit/e9d1d905f1ce86f9de2cf39d79be4b5aada4a81d))
* **circuit_prover:** Add circuit prover ([#2908](https://github.com/matter-labs/zksync-era/issues/2908)) ([48317e6](https://github.com/matter-labs/zksync-era/commit/48317e640a00b016bf7bf782cc94fccaf077ed6d))
* **prover:** Add endpoint to PJM to get queue reports ([#2918](https://github.com/matter-labs/zksync-era/issues/2918)) ([2cec83f](https://github.com/matter-labs/zksync-era/commit/2cec83f26e0b9309387135ca43718af4fcd6f6b1))
* **vm:** Do not panic on VM divergence ([#2705](https://github.com/matter-labs/zksync-era/issues/2705)) ([7aa5721](https://github.com/matter-labs/zksync-era/commit/7aa5721d22e253d05d369a60d5bcacbf52021c48))
* **vm:** Extract oneshot VM executor – environment types ([#2885](https://github.com/matter-labs/zksync-era/issues/2885)) ([a2d4126](https://github.com/matter-labs/zksync-era/commit/a2d4126f9e0c9dc46f49a861549d076fdbcf66d3))


### Bug Fixes

* **api_server:** fix blob_gas length ([#2673](https://github.com/matter-labs/zksync-era/issues/2673)) ([44a8f79](https://github.com/matter-labs/zksync-era/commit/44a8f79739704e1216a7b34a0580ad7ba1cce3bd))
* **eth-sender:** print better error message in case of missing blob prices ([#2927](https://github.com/matter-labs/zksync-era/issues/2927)) ([38fc824](https://github.com/matter-labs/zksync-era/commit/38fc824f75e8b0e84f10348d1502fc8a26d12015))

## [24.25.0](https://github.com/matter-labs/zksync-era/compare/core-v24.24.0...core-v24.25.0) (2024-09-19)


### Features

* (DB migration) Rename recursion_scheduler_level_vk_hash to snark_wrapper_vk_hash ([#2809](https://github.com/matter-labs/zksync-era/issues/2809)) ([64f9551](https://github.com/matter-labs/zksync-era/commit/64f95514c99f95da2a19a97ff064c29a97efc22f))
* add da clients ([#2743](https://github.com/matter-labs/zksync-era/issues/2743)) ([9218612](https://github.com/matter-labs/zksync-era/commit/9218612fdb2b63c20841e2e2e5a45bbd23c01fbc))
* attester committees data extractor (BFT-434) ([#2684](https://github.com/matter-labs/zksync-era/issues/2684)) ([92dde03](https://github.com/matter-labs/zksync-era/commit/92dde039ee8a0bc08e2019b7fa6f243a34d9816f))
* emit errors in prover API metrics ([#2890](https://github.com/matter-labs/zksync-era/issues/2890)) ([2ac7cc5](https://github.com/matter-labs/zksync-era/commit/2ac7cc5836e69fc82c98df2005fedee01c1084e1))
* **en:** Resume incomplete snapshot in snapshot creator in more cases ([#2886](https://github.com/matter-labs/zksync-era/issues/2886)) ([f095b4a](https://github.com/matter-labs/zksync-era/commit/f095b4a3223222ac712de53592fe1e68f766600f))
* make `to` address optional for transaction data ([#2852](https://github.com/matter-labs/zksync-era/issues/2852)) ([8363c1d](https://github.com/matter-labs/zksync-era/commit/8363c1d8697ad9bd2fe5d326218476bc3dad38af))
* **prover:** Optimize setup keys loading ([#2847](https://github.com/matter-labs/zksync-era/issues/2847)) ([19887ef](https://github.com/matter-labs/zksync-era/commit/19887ef21a8bbd26977353f8ee277b711850dfd2))
* Selector generator tool ([#2844](https://github.com/matter-labs/zksync-era/issues/2844)) ([b359b08](https://github.com/matter-labs/zksync-era/commit/b359b085895da6582f1d28722107bc5b25f1232c))
* **tee:** use hex serialization for RPC responses ([#2887](https://github.com/matter-labs/zksync-era/issues/2887)) ([abe0440](https://github.com/matter-labs/zksync-era/commit/abe0440811ae4daf4a0f307922a282e9664308e0))
* **utils:** Rework locate_workspace, introduce Workspace type ([#2830](https://github.com/matter-labs/zksync-era/issues/2830)) ([d256092](https://github.com/matter-labs/zksync-era/commit/d2560928cc67b40a97a5497ac8542915bf6f91a9))
* **zk_toolbox:** Add external_node consensus support ([#2821](https://github.com/matter-labs/zksync-era/issues/2821)) ([4a10d7d](https://github.com/matter-labs/zksync-era/commit/4a10d7d9554d6c1aa2f4fc46557d40baaad8ff2f))


### Bug Fixes

* count SECP256 precompile to account validation gas limit as well ([#2859](https://github.com/matter-labs/zksync-era/issues/2859)) ([fee0c2a](https://github.com/matter-labs/zksync-era/commit/fee0c2ad08a5ab4a04252765b367eb9fbb1f3db7))
* **en:** Fix connection starvation during snapshot recovery ([#2836](https://github.com/matter-labs/zksync-era/issues/2836)) ([52f4f76](https://github.com/matter-labs/zksync-era/commit/52f4f763674d25f8a5e7f3a111354a559f798d52))
* **eth_watch:** fix `get_events_inner` ([#2882](https://github.com/matter-labs/zksync-era/issues/2882)) ([c957dd8](https://github.com/matter-labs/zksync-era/commit/c957dd8011213e0e95fa5962e2310321b29a0d16))
* handling of HTTP 403 thrown by proxyd ([#2835](https://github.com/matter-labs/zksync-era/issues/2835)) ([2d71c74](https://github.com/matter-labs/zksync-era/commit/2d71c7408a0eed3662fc51f70fa9f525d66e4c6f))
* **state-keeper:** Restore processed tx metrics in state keeper ([#2815](https://github.com/matter-labs/zksync-era/issues/2815)) ([4d8862b](https://github.com/matter-labs/zksync-era/commit/4d8862b76a55ac78edd481694fefd2107736ffd9))
* **tee-prover:** fix deserialization of `std::time::Duration` in `envy` config ([#2817](https://github.com/matter-labs/zksync-era/issues/2817)) ([df8641a](https://github.com/matter-labs/zksync-era/commit/df8641a912a8d480ceecff58b0bfaef05e04f0c8))

## [24.24.0](https://github.com/matter-labs/zksync-era/compare/core-v24.23.0...core-v24.24.0) (2024-09-05)


### Features

* conditional cbt l1 updates ([#2748](https://github.com/matter-labs/zksync-era/issues/2748)) ([6d18061](https://github.com/matter-labs/zksync-era/commit/6d18061df4a18803d3c6377305ef711ce60317e1))
* **eth-watch:** do not query events from earliest block ([#2810](https://github.com/matter-labs/zksync-era/issues/2810)) ([1da3f7e](https://github.com/matter-labs/zksync-era/commit/1da3f7ea1df94312e7c6818c17bf4109f888e547))
* **genesis:** Validate genesis config against L1 ([#2786](https://github.com/matter-labs/zksync-era/issues/2786)) ([b2dd9a5](https://github.com/matter-labs/zksync-era/commit/b2dd9a5c08fecf0a878632b33a32a78aac11c065))
* Integrate tracers and implement circuits tracer in vm2 ([#2653](https://github.com/matter-labs/zksync-era/issues/2653)) ([87b02e3](https://github.com/matter-labs/zksync-era/commit/87b02e3ab5c1f61d59dd0f0eefa9ec33a7b55488))
* Move prover data to /home/popzxc/workspace/current/zksync-era/prover/data ([#2778](https://github.com/matter-labs/zksync-era/issues/2778)) ([62e4d46](https://github.com/matter-labs/zksync-era/commit/62e4d4619dde9d6bd9102f1410eea75b0e2051c5))
* Remove prover db from house keeper ([#2795](https://github.com/matter-labs/zksync-era/issues/2795)) ([85b7346](https://github.com/matter-labs/zksync-era/commit/85b734664b4306e988da07005860a7ea0fb7d22d))
* **vm-runner:** Implement batch data prefetching ([#2724](https://github.com/matter-labs/zksync-era/issues/2724)) ([d01840d](https://github.com/matter-labs/zksync-era/commit/d01840d5de2cb0f4bead8f1c384b24ba713e6a66))
* **vm:** Extract batch executor to separate crate ([#2702](https://github.com/matter-labs/zksync-era/issues/2702)) ([b82dfa4](https://github.com/matter-labs/zksync-era/commit/b82dfa4d29fce107223c3638fe490b5cb0f28d8c))
* **vm:** Simplify VM interface ([#2760](https://github.com/matter-labs/zksync-era/issues/2760)) ([c3bde47](https://github.com/matter-labs/zksync-era/commit/c3bde47c1e7d16bc00f9b089516ed3691e4f3eb1))
* **zk_toolbox:** add multi-chain CI integration test ([#2594](https://github.com/matter-labs/zksync-era/issues/2594)) ([05c940e](https://github.com/matter-labs/zksync-era/commit/05c940efbd93023c315e5e13c98faee2153cc1cd))


### Bug Fixes

* **config:** Do not panic for observability config ([#2639](https://github.com/matter-labs/zksync-era/issues/2639)) ([1e768d4](https://github.com/matter-labs/zksync-era/commit/1e768d402012f6c7ce83fdd46c55f830ec31416a))
* **core:** Batched event processing support for Reth ([#2623](https://github.com/matter-labs/zksync-era/issues/2623)) ([958dfdc](https://github.com/matter-labs/zksync-era/commit/958dfdcac358897bfd4d2a2ddc1633a23dbfcdc9))
* return correct witness inputs ([#2770](https://github.com/matter-labs/zksync-era/issues/2770)) ([2516e2e](https://github.com/matter-labs/zksync-era/commit/2516e2e5c83673687d61d143daa70e98ccecce53))
* **tee-prover:** increase retries to reduce spurious alerts ([#2776](https://github.com/matter-labs/zksync-era/issues/2776)) ([4fdc806](https://github.com/matter-labs/zksync-era/commit/4fdc80636437090f6ebcfa4e2f1eb50edf53631a))
* **tee-prover:** mitigate panic on redeployments ([#2764](https://github.com/matter-labs/zksync-era/issues/2764)) ([178b386](https://github.com/matter-labs/zksync-era/commit/178b38644f507c5f6d12ba862d0c699e87985dd7))
* **tee:** lowercase enum TEE types ([#2798](https://github.com/matter-labs/zksync-era/issues/2798)) ([0f2f9bd](https://github.com/matter-labs/zksync-era/commit/0f2f9bd9ef4c2c7ba98a1fdbfca15d1de2b29997))
* **vm-runner:** Fix statement timeouts in VM playground ([#2772](https://github.com/matter-labs/zksync-era/issues/2772)) ([d3cd553](https://github.com/matter-labs/zksync-era/commit/d3cd553888a5c903c6eae13a88e92c11602e93de))


### Performance Improvements

* **vm:** Fix VM performance regression on CI loadtest ([#2782](https://github.com/matter-labs/zksync-era/issues/2782)) ([bc0d7d5](https://github.com/matter-labs/zksync-era/commit/bc0d7d5935c8f5409a8e53f1c04c5141409aef31))

## [24.23.0](https://github.com/matter-labs/zksync-era/compare/core-v24.22.0...core-v24.23.0) (2024-08-28)


### Features

* Refactor metrics/make API use binaries ([#2735](https://github.com/matter-labs/zksync-era/issues/2735)) ([8ed086a](https://github.com/matter-labs/zksync-era/commit/8ed086afecfcad30bfda44fc4d29a00beea71cca))


### Bug Fixes

* **api:** Fix duplicate DB connection acquired in `eth_call` ([#2763](https://github.com/matter-labs/zksync-era/issues/2763)) ([74b764c](https://github.com/matter-labs/zksync-era/commit/74b764c12e6daa410c611cec42455a00e68ed912))
* **vm:** Fix used bytecodes divergence ([#2741](https://github.com/matter-labs/zksync-era/issues/2741)) ([923e33e](https://github.com/matter-labs/zksync-era/commit/923e33e81bba83f72b97ca9590c5cdf2da2a311b))

## [24.22.0](https://github.com/matter-labs/zksync-era/compare/core-v24.21.0...core-v24.22.0) (2024-08-27)


### Features

* add flag to enable/disable DA inclusion verification ([#2647](https://github.com/matter-labs/zksync-era/issues/2647)) ([b425561](https://github.com/matter-labs/zksync-era/commit/b4255618708349c51f60f5c7fc26f9356d32b6ff))
* **Base token:** add cbt metrics ([#2720](https://github.com/matter-labs/zksync-era/issues/2720)) ([58438eb](https://github.com/matter-labs/zksync-era/commit/58438eb174c30edf62e2ff8abb74567de2a4bea8))
* Change default_protective_reads_persistence_enabled to false ([#2716](https://github.com/matter-labs/zksync-era/issues/2716)) ([8d0eee7](https://github.com/matter-labs/zksync-era/commit/8d0eee7ca8fe117b2ee286c6080bfa0057ee31ae))
* **vm:** Extract oneshot VM executor interface ([#2671](https://github.com/matter-labs/zksync-era/issues/2671)) ([951d5f2](https://github.com/matter-labs/zksync-era/commit/951d5f208e5d16a5d95878dd345a8bd2a4144aa7))
* **zk_toolbox:** Add holesky testnet as layer1 network ([#2632](https://github.com/matter-labs/zksync-era/issues/2632)) ([d9266e5](https://github.com/matter-labs/zksync-era/commit/d9266e5ef3910732666c00c1324256fb5b54452d))


### Bug Fixes

* **api:** `tx.gas_price` field ([#2734](https://github.com/matter-labs/zksync-era/issues/2734)) ([aea3726](https://github.com/matter-labs/zksync-era/commit/aea3726c88b4e881bcd0f4a60ff32a730f200938))
* **base_token_adjuster:** bug with a wrong metrics namespace ([#2744](https://github.com/matter-labs/zksync-era/issues/2744)) ([64b2ff8](https://github.com/matter-labs/zksync-era/commit/64b2ff8b81dcc146cd0535eb0d2d898c18ad5f7f))
* **eth-sender:** missing Gateway migration changes ([#2732](https://github.com/matter-labs/zksync-era/issues/2732)) ([a4170e9](https://github.com/matter-labs/zksync-era/commit/a4170e9e7f321a1062495ec586e0ce9186269088))
* **proof_data_handler:** TEE blob fetching error handling ([#2674](https://github.com/matter-labs/zksync-era/issues/2674)) ([c162510](https://github.com/matter-labs/zksync-era/commit/c162510598b45dc062c2c91085868f8aa966360e))

## [24.21.0](https://github.com/matter-labs/zksync-era/compare/core-v24.20.0...core-v24.21.0) (2024-08-22)


### Features

* External prover API metrics, refactoring ([#2630](https://github.com/matter-labs/zksync-era/issues/2630)) ([c83cca8](https://github.com/matter-labs/zksync-era/commit/c83cca8fe7fa105ec6b1491e4efb9f9e4bd66d41))

## [24.20.0](https://github.com/matter-labs/zksync-era/compare/core-v24.19.0...core-v24.20.0) (2024-08-21)


### Features

* Add `gateway_url` to EN config ([#2698](https://github.com/matter-labs/zksync-era/issues/2698)) ([cfdda01](https://github.com/matter-labs/zksync-era/commit/cfdda019afe26810234285411eba79ada472c888))
* **vm:** Enable parallelization in VM playground ([#2679](https://github.com/matter-labs/zksync-era/issues/2679)) ([c9ad59e](https://github.com/matter-labs/zksync-era/commit/c9ad59e1ec918f29a7a4b26fe5a6f62cf94a5ba1))


### Bug Fixes

* base token ratio startup as a separate component ([#2704](https://github.com/matter-labs/zksync-era/issues/2704)) ([d65588f](https://github.com/matter-labs/zksync-era/commit/d65588f42391ce03fc636daa541b1978fad13429))
* **upgrade.test.ts:** minting from a clean state ([#2402](https://github.com/matter-labs/zksync-era/issues/2402)) ([efa3bd6](https://github.com/matter-labs/zksync-era/commit/efa3bd6c09fbaf75d9807349afa626eb99fc3dfe))

## [24.19.0](https://github.com/matter-labs/zksync-era/compare/core-v24.18.0...core-v24.19.0) (2024-08-21)


### Features

* **db:** Allow creating owned Postgres connections ([#2654](https://github.com/matter-labs/zksync-era/issues/2654)) ([47a082b](https://github.com/matter-labs/zksync-era/commit/47a082b3312cae7aa0f2317a45a26fa5f22d043c))
* **eth-sender:** add option to pause aggregator for gateway migration ([#2644](https://github.com/matter-labs/zksync-era/issues/2644)) ([56d8ee8](https://github.com/matter-labs/zksync-era/commit/56d8ee8c0546cc26d412b95cb72bbb1b9a3a6580))
* **eth-sender:** added chain_id column to eth_txs + support for gateway in tx_aggregator ([#2685](https://github.com/matter-labs/zksync-era/issues/2685)) ([97aa6fb](https://github.com/matter-labs/zksync-era/commit/97aa6fb9a01c7e43d8f9a8d33f78fc6dca61548b))
* **eth-sender:** gateway support for eth tx manager ([#2593](https://github.com/matter-labs/zksync-era/issues/2593)) ([25aff59](https://github.com/matter-labs/zksync-era/commit/25aff59933bb996963700544ad31e5f9d9c27ad7))
* **prover_cli:** Add test for status, l1 and config commands. ([#2263](https://github.com/matter-labs/zksync-era/issues/2263)) ([6a2e3b0](https://github.com/matter-labs/zksync-era/commit/6a2e3b05b7d9c9e8b476fb207631c2285e1bd881))
* **prover_cli:** Stuck status ([#2441](https://github.com/matter-labs/zksync-era/issues/2441)) ([232a817](https://github.com/matter-labs/zksync-era/commit/232a817a73fa842ca4b3be419bc775c85204901e))
* **prover:** Add ProverJobMonitor ([#2666](https://github.com/matter-labs/zksync-era/issues/2666)) ([e22cfb6](https://github.com/matter-labs/zksync-era/commit/e22cfb6cffd2c4b2ad1ec3f3f433616fcd738511))
* **prover:** parallelized memory queues simulation in BWG ([#2652](https://github.com/matter-labs/zksync-era/issues/2652)) ([b4ffcd2](https://github.com/matter-labs/zksync-era/commit/b4ffcd237ee594fc659ccfa96668868f5a87d5e3))
* update base token rate on L1 ([#2589](https://github.com/matter-labs/zksync-era/issues/2589)) ([f84aaaf](https://github.com/matter-labs/zksync-era/commit/f84aaaf723c876ba8397f74577b8c5a207700f7b))
* **zk_toolbox:** Add zk_supervisor run unit tests command ([#2610](https://github.com/matter-labs/zksync-era/issues/2610)) ([fa866cd](https://github.com/matter-labs/zksync-era/commit/fa866cd5c7b1b189901b4f7ce6f91886e7aec7e4))
* **zk_toolbox:** Run formatters and linterrs ([#2675](https://github.com/matter-labs/zksync-era/issues/2675)) ([caedd1c](https://github.com/matter-labs/zksync-era/commit/caedd1c86eedd94f8628bd2ba1cf875cad9a53d1))


### Bug Fixes

* **contract-verifier:** Check for 0x in zkvyper output ([#2693](https://github.com/matter-labs/zksync-era/issues/2693)) ([0d77588](https://github.com/matter-labs/zksync-era/commit/0d7758884f84d7fa7b033b98d301c8b13d7d40ad))
* make set token multiplier optional ([#2696](https://github.com/matter-labs/zksync-era/issues/2696)) ([16dff4f](https://github.com/matter-labs/zksync-era/commit/16dff4fd79edf9f7633e5856bc889337343ef69e))
* **prover:** change bucket for RAM permutation witnesses ([#2672](https://github.com/matter-labs/zksync-era/issues/2672)) ([8b4cbf4](https://github.com/matter-labs/zksync-era/commit/8b4cbf43e52203aac829324aa48450575b70c656))
* use lower fair l2 gas price for cbt ([#2690](https://github.com/matter-labs/zksync-era/issues/2690)) ([e1146fc](https://github.com/matter-labs/zksync-era/commit/e1146fc893f4a801d6f980d0cbbc45bd7ec1c9c6))


### Performance Improvements

* **logs-bloom:** do not run heavy query if migration was completed ([#2680](https://github.com/matter-labs/zksync-era/issues/2680)) ([f9ef00e](https://github.com/matter-labs/zksync-era/commit/f9ef00e7088b723a6b4c82f1348dbaaf1934f0ab))

## [24.18.0](https://github.com/matter-labs/zksync-era/compare/core-v24.17.0...core-v24.18.0) (2024-08-14)


### Features

* add logs bloom ([#2633](https://github.com/matter-labs/zksync-era/issues/2633)) ([1067462](https://github.com/matter-labs/zksync-era/commit/10674620d1a04333507ca17b9a34ab3cb58846cf))
* **zk_toolbox:** Minting base token ([#2571](https://github.com/matter-labs/zksync-era/issues/2571)) ([ae2dd3b](https://github.com/matter-labs/zksync-era/commit/ae2dd3bbccdffc25b040313b2c7983a936f36aac))

## [24.17.0](https://github.com/matter-labs/zksync-era/compare/core-v24.16.0...core-v24.17.0) (2024-08-13)


### Features

* Allow tracking l2 fees for L2-based chains ([#2563](https://github.com/matter-labs/zksync-era/issues/2563)) ([e3f7804](https://github.com/matter-labs/zksync-era/commit/e3f78042b93b25d609e5767e2ba76502ede84415))
* Remove old EN code ([#2595](https://github.com/matter-labs/zksync-era/issues/2595)) ([8d31ebc](https://github.com/matter-labs/zksync-era/commit/8d31ebceaf958c7147c973243c618c87c42d53d8))
* **tee:** introduce get_tee_proofs RPC method for TEE proofs ([#2474](https://github.com/matter-labs/zksync-era/issues/2474)) ([d40ff5f](https://github.com/matter-labs/zksync-era/commit/d40ff5f3aa41801c054d0557f9aea11715af9c31))
* **vm:** Fast VM integration ([#1949](https://github.com/matter-labs/zksync-era/issues/1949)) ([b752a54](https://github.com/matter-labs/zksync-era/commit/b752a54bebe6eb3bf0bea044996f5116cc5dc4e2))


### Bug Fixes

* query for prover API ([#2628](https://github.com/matter-labs/zksync-era/issues/2628)) ([b8609eb](https://github.com/matter-labs/zksync-era/commit/b8609eb131ac9ce428cd45a3be9ba4062cd7bbe2))
* **vm:** Fix missing experimental VM config ([#2629](https://github.com/matter-labs/zksync-era/issues/2629)) ([e07a39d](https://github.com/matter-labs/zksync-era/commit/e07a39daa564d6032ad61a135da78775a4f2c9ce))

## [24.16.0](https://github.com/matter-labs/zksync-era/compare/core-v24.15.0...core-v24.16.0) (2024-08-08)


### Features

* External prover API ([#2538](https://github.com/matter-labs/zksync-era/issues/2538)) ([129a181](https://github.com/matter-labs/zksync-era/commit/129a1819262d64a36d651af01fdab93c5ff91712))
* **node-framework:** Add API fee params resource ([#2621](https://github.com/matter-labs/zksync-era/issues/2621)) ([aff7b65](https://github.com/matter-labs/zksync-era/commit/aff7b6535ef92aaced0dd7fa1cc08d656cba027e))
* **vlog:** Expose more resource values via opentelemetry ([#2620](https://github.com/matter-labs/zksync-era/issues/2620)) ([7ae07e4](https://github.com/matter-labs/zksync-era/commit/7ae07e446c9732a896ca8246d324e82c6e6d5a46))
* **vlog:** Report observability config, flush, and shutdown ([#2622](https://github.com/matter-labs/zksync-era/issues/2622)) ([e23e661](https://github.com/matter-labs/zksync-era/commit/e23e6611731835ef3abd34f3f9867f9dc533eb21))


### Bug Fixes

* Bump prover dependencies & rust toolchain ([#2600](https://github.com/matter-labs/zksync-era/issues/2600)) ([849c6a5](https://github.com/matter-labs/zksync-era/commit/849c6a5dcd095e8fead0630a2a403f282c26a2aa))
* **en:** Initialize SyncState in OutputHandler ([#2618](https://github.com/matter-labs/zksync-era/issues/2618)) ([f0c8506](https://github.com/matter-labs/zksync-era/commit/f0c85062fac96180c2e0dec52086714ee3783fcf))
* restrictive genesis parsing ([#2605](https://github.com/matter-labs/zksync-era/issues/2605)) ([d5f8f38](https://github.com/matter-labs/zksync-era/commit/d5f8f3892a14180f590cabab921d3a68dec903e3))

## [24.15.0](https://github.com/matter-labs/zksync-era/compare/core-v24.14.0...core-v24.15.0) (2024-08-07)


### Features

* optimize LWG and NWG ([#2512](https://github.com/matter-labs/zksync-era/issues/2512)) ([0d00650](https://github.com/matter-labs/zksync-era/commit/0d00650f3e97248617b88a7e082c515ac48d5d5b))
* Poll the main node API for attestation status - relaxed (BFT-496) ([#2583](https://github.com/matter-labs/zksync-era/issues/2583)) ([b45aa91](https://github.com/matter-labs/zksync-era/commit/b45aa9168dd66d07ca61c8bb4c01f73dda822040))
* **zk_toolbox:** allow to run `zk_inception chain create` non-interactively ([#2579](https://github.com/matter-labs/zksync-era/issues/2579)) ([555fcf7](https://github.com/matter-labs/zksync-era/commit/555fcf79bc950f79e218697be9f1a316e4723322))


### Bug Fixes

* **core:** Handle GCS Response retriable errors ([#2588](https://github.com/matter-labs/zksync-era/issues/2588)) ([4b74092](https://github.com/matter-labs/zksync-era/commit/4b74092dbe4631824d518f72318efcb3bf8d37be))
* **node:** respect namespaces configuration ([#2578](https://github.com/matter-labs/zksync-era/issues/2578)) ([e2d9060](https://github.com/matter-labs/zksync-era/commit/e2d90606d1286e61f3b4c601d4b2237ed26aeb80))
* **vm-runner:** Fix data race in storage loader ([1810b78](https://github.com/matter-labs/zksync-era/commit/1810b78594083f1f98d2901f3643b5687ce9d8e8))


### Reverts

* "feat: Poll the main node for the next batch to sign (BFT-496)" ([#2574](https://github.com/matter-labs/zksync-era/issues/2574)) ([72d3be8](https://github.com/matter-labs/zksync-era/commit/72d3be87efcb059f70b4633cddd707346612c4db))

## [24.14.0](https://github.com/matter-labs/zksync-era/compare/core-v24.13.0...core-v24.14.0) (2024-08-01)


### Features

* Adding SLChainID ([#2547](https://github.com/matter-labs/zksync-era/issues/2547)) ([656e830](https://github.com/matter-labs/zksync-era/commit/656e830e4fd60b5ace87dfc1604a102f06ae59e1))
* **consensus:** add tracing instrumentation to consensus store ([#2546](https://github.com/matter-labs/zksync-era/issues/2546)) ([1e53940](https://github.com/matter-labs/zksync-era/commit/1e53940d5592410be34e56f48469065c516fcb54))
* Poll the main node for the next batch to sign (BFT-496) ([#2544](https://github.com/matter-labs/zksync-era/issues/2544)) ([22cf820](https://github.com/matter-labs/zksync-era/commit/22cf820abbd14b852dffe60f6b564713fe4c8919))
* Support sending logs via OTLP ([#2556](https://github.com/matter-labs/zksync-era/issues/2556)) ([1d206c0](https://github.com/matter-labs/zksync-era/commit/1d206c0af8f28eb00eb1498d6f2cdbb45ffef72a))

## [24.13.0](https://github.com/matter-labs/zksync-era/compare/core-v24.12.0...core-v24.13.0) (2024-07-31)


### Features

* Add recovery tests to zk_supervisor ([#2444](https://github.com/matter-labs/zksync-era/issues/2444)) ([0c0d10a](https://github.com/matter-labs/zksync-era/commit/0c0d10af703d3f8958c49d0ed46d6cda64945fa1))
* Added a JSON RPC to simulating L1 for consensus attestation ([#2480](https://github.com/matter-labs/zksync-era/issues/2480)) ([c6b3adf](https://github.com/matter-labs/zksync-era/commit/c6b3adf3f29d3a89daa2cfffa1c0e5cb9770eb0d))
* added dropping all attester certificates when doing hard fork ([#2529](https://github.com/matter-labs/zksync-era/issues/2529)) ([5acd686](https://github.com/matter-labs/zksync-era/commit/5acd68640da6b22897185d76101180bcd838ac67))
* **configs:** Do not panic if config is only partially filled ([#2545](https://github.com/matter-labs/zksync-era/issues/2545)) ([db13fe3](https://github.com/matter-labs/zksync-era/commit/db13fe3550598c69f59cd66b4bb9618ebea041ca))
* **eth-sender:** Make eth-sender tests use blob txs + refactor of eth-sender tests ([#2316](https://github.com/matter-labs/zksync-era/issues/2316)) ([c8c8334](https://github.com/matter-labs/zksync-era/commit/c8c83349c10710d75c0030409f926db313c9660d))
* Introduce more tracing instrumentation ([#2523](https://github.com/matter-labs/zksync-era/issues/2523)) ([79d407a](https://github.com/matter-labs/zksync-era/commit/79d407ac47ac51667196aa2cd028d05b1622130f))
* Remove unused VKs, add docs for BWG ([#2468](https://github.com/matter-labs/zksync-era/issues/2468)) ([2fa6bf0](https://github.com/matter-labs/zksync-era/commit/2fa6bf0ffa5d7a5ff62d595a0efeff9dcd9e5a1a))
* Revisit base config values ([#2532](https://github.com/matter-labs/zksync-era/issues/2532)) ([3fac8ac](https://github.com/matter-labs/zksync-era/commit/3fac8ac62cc9ac14845f32240af9241386f4034d))
* Server 10k gwei limit on gas price and 1M limit on pubdata price ([#2460](https://github.com/matter-labs/zksync-era/issues/2460)) ([be238cc](https://github.com/matter-labs/zksync-era/commit/be238ccb35e4581de22156f76903211bd85526b7))
* **vlog:** Implement otlp guard with force flush on drop ([#2536](https://github.com/matter-labs/zksync-era/issues/2536)) ([c9f76e5](https://github.com/matter-labs/zksync-era/commit/c9f76e571e5570d2c4194feee03bb260b24c378f))
* **vlog:** New vlog interface + opentelemtry improvements ([#2472](https://github.com/matter-labs/zksync-era/issues/2472)) ([c0815cd](https://github.com/matter-labs/zksync-era/commit/c0815cdaf878afcd9c41dddd9fe56bcf8d910633))
* **zk_toolbox:** add test upgrade subcommand to zk_toolbox ([#2515](https://github.com/matter-labs/zksync-era/issues/2515)) ([1a12f5f](https://github.com/matter-labs/zksync-era/commit/1a12f5f908add42c090170a2f4fb26b731d6971b))
* **zk_toolbox:** use configs from the main repo ([#2470](https://github.com/matter-labs/zksync-era/issues/2470)) ([4222d13](https://github.com/matter-labs/zksync-era/commit/4222d135b62eb4de103c4aebb35e9c302d94ad63))


### Bug Fixes

* **contract verifier:** Fix config values  ([#2510](https://github.com/matter-labs/zksync-era/issues/2510)) ([3729468](https://github.com/matter-labs/zksync-era/commit/3729468436114642e62ce8a531533921015455a7))
* fixed panic propagation ([#2525](https://github.com/matter-labs/zksync-era/issues/2525)) ([e0fc58b](https://github.com/matter-labs/zksync-era/commit/e0fc58b536debf804920887e3eb4bc050a9fe9d6))
* **proof_data_handler:** Unlock jobs on transient errors ([#2486](https://github.com/matter-labs/zksync-era/issues/2486)) ([7c336b1](https://github.com/matter-labs/zksync-era/commit/7c336b1e180b9d5ba1ba74169c61ce27a251e2fc))
* **prover:** Parallelize circuit metadata uploading for BWG ([#2520](https://github.com/matter-labs/zksync-era/issues/2520)) ([f49720f](https://github.com/matter-labs/zksync-era/commit/f49720fbafdab8f102d908b2be3fa869482a92fa))
* VM performance diff: don't show 0 as N/A ([#2276](https://github.com/matter-labs/zksync-era/issues/2276)) ([2fa2249](https://github.com/matter-labs/zksync-era/commit/2fa2249dca15b1968fceec11e485850395f03c9d))

## [24.12.0](https://github.com/matter-labs/zksync-era/compare/core-v24.11.0...core-v24.12.0) (2024-07-25)


### Features

* add general config and secrets opts to snapshot creator ([#2471](https://github.com/matter-labs/zksync-era/issues/2471)) ([0f475c9](https://github.com/matter-labs/zksync-era/commit/0f475c949a28c4602539b4d75ee79e605f44e2de))
* Update to consensus 0.1.0-rc.4 (BFT-486) ([#2475](https://github.com/matter-labs/zksync-era/issues/2475)) ([ff6b10c](https://github.com/matter-labs/zksync-era/commit/ff6b10c4a994cf70297a034202bcb55152748cba))


### Bug Fixes

* consensus secrets generator ([#2484](https://github.com/matter-labs/zksync-era/issues/2484)) ([dea6969](https://github.com/matter-labs/zksync-era/commit/dea6969d1b67c54a0985278de68a8d50f1084dc1))


### Performance Improvements

* writing tx to bootloader memory is no longer quadratic ([#2479](https://github.com/matter-labs/zksync-era/issues/2479)) ([1c443e5](https://github.com/matter-labs/zksync-era/commit/1c443e5ecfd000279830262a4a35cbc83a9aacec))

## [24.11.0](https://github.com/matter-labs/zksync-era/compare/core-v24.10.0...core-v24.11.0) (2024-07-23)


### Features

* add revert tests (external node) to zk_toolbox ([#2408](https://github.com/matter-labs/zksync-era/issues/2408)) ([3fbbee1](https://github.com/matter-labs/zksync-era/commit/3fbbee10be99e8c5a696bfd50d81230141bccbf4))
* add state override for gas estimates ([#1358](https://github.com/matter-labs/zksync-era/issues/1358)) ([761bda1](https://github.com/matter-labs/zksync-era/commit/761bda19844fb3935f8a57c47df39010f88ef9dc))
* added consensus_config to general config ([#2462](https://github.com/matter-labs/zksync-era/issues/2462)) ([c5650a4](https://github.com/matter-labs/zksync-era/commit/c5650a4f1747f59d7a2d4e1986a91ae3fa7d75b0))
* added key generation command to EN ([#2461](https://github.com/matter-labs/zksync-era/issues/2461)) ([9861415](https://github.com/matter-labs/zksync-era/commit/986141562646c4d96dca205593e48e4d8df46fba))
* remove leftovers after BWIP ([#2456](https://github.com/matter-labs/zksync-era/issues/2456)) ([990676c](https://github.com/matter-labs/zksync-era/commit/990676c5f84afd2ff8cd337f495c82e8d1f305a4))

## [24.10.0](https://github.com/matter-labs/zksync-era/compare/core-v24.9.0...core-v24.10.0) (2024-07-22)


### Features

* Add blob size metrics ([#2411](https://github.com/matter-labs/zksync-era/issues/2411)) ([41c535a](https://github.com/matter-labs/zksync-era/commit/41c535af2bcc72000116277d5dd9e04b5c0b2372))
* **en:** Switch EN to use node framework ([#2427](https://github.com/matter-labs/zksync-era/issues/2427)) ([0cee530](https://github.com/matter-labs/zksync-era/commit/0cee530b2f2e8304b7e20a093a32abe116463b57))
* **eth-sender:** add early return in sending new transactions to not spam logs with errors ([#2425](https://github.com/matter-labs/zksync-era/issues/2425)) ([192f2a3](https://github.com/matter-labs/zksync-era/commit/192f2a374d83eaecb52f198fdcfa615262378530))
* **eth-watch:** Integrate decentralized upgrades ([#2401](https://github.com/matter-labs/zksync-era/issues/2401)) ([5a48e10](https://github.com/matter-labs/zksync-era/commit/5a48e1026260024c6ae2b4d1100ee9b798a83e8d))
* L1 batch signing (BFT-474) ([#2414](https://github.com/matter-labs/zksync-era/issues/2414)) ([ab699db](https://github.com/matter-labs/zksync-era/commit/ab699dbe8cffa8bd291d6054579061b47fd4aa0e))
* **prover:** Make it possible to run prover out of GCP ([#2448](https://github.com/matter-labs/zksync-era/issues/2448)) ([c9da549](https://github.com/matter-labs/zksync-era/commit/c9da5497e2aa9d85f204ab7b74fefcfe941793ff))
* **zk_toolbox:** Small adjustment for zk toolbox ([#2424](https://github.com/matter-labs/zksync-era/issues/2424)) ([ce43c42](https://github.com/matter-labs/zksync-era/commit/ce43c422fddccfe88c07ee22a2b8726dd0bd5f61))


### Bug Fixes

* **eth-sender:** add bump of min 10% when resending txs to avoid "replacement transaction underpriced" ([#2422](https://github.com/matter-labs/zksync-era/issues/2422)) ([a7bcf5d](https://github.com/matter-labs/zksync-era/commit/a7bcf5d7f75eb45384312d7c97f25a50a91e7a31))
* Set attesters in Connection::adjust_genesis (BFT-489) ([#2429](https://github.com/matter-labs/zksync-era/issues/2429)) ([ca4cb3c](https://github.com/matter-labs/zksync-era/commit/ca4cb3cba04757dc1760397c667a838931cd2d11))

## [24.9.0](https://github.com/matter-labs/zksync-era/compare/core-v24.8.0...core-v24.9.0) (2024-07-10)


### Features

* add block timestamp to `eth_getLogs` ([#2374](https://github.com/matter-labs/zksync-era/issues/2374)) ([50422b8](https://github.com/matter-labs/zksync-era/commit/50422b897d2b0fdbb82f1c4cdb97c1a39ace02c7))
* add revert tests to zk_toolbox ([#2317](https://github.com/matter-labs/zksync-era/issues/2317)) ([c9ad002](https://github.com/matter-labs/zksync-era/commit/c9ad002d17ed91d1e5f225e19698c12cb3adc665))
* add zksync_tee_prover and container to nix ([#2403](https://github.com/matter-labs/zksync-era/issues/2403)) ([e0975db](https://github.com/matter-labs/zksync-era/commit/e0975db317ae7934ce47b5267790b696fc9a1113))
* Adding unstable RPC endpoint to return the execution_info ([#2332](https://github.com/matter-labs/zksync-era/issues/2332)) ([3d047ea](https://github.com/matter-labs/zksync-era/commit/3d047ea953d6fed4d0463fce60f743086f4a13b9))
* **api:** Retry `read_value` ([#2352](https://github.com/matter-labs/zksync-era/issues/2352)) ([256a43c](https://github.com/matter-labs/zksync-era/commit/256a43cdd01619b89e348419bc361454ba4fdabb))
* Base Token Fundamentals ([#2204](https://github.com/matter-labs/zksync-era/issues/2204)) ([39709f5](https://github.com/matter-labs/zksync-era/commit/39709f58071ac77bfd447145e1c3342b7da70560))
* **base-token:** Base token price ratio cache update frequency configurable ([#2388](https://github.com/matter-labs/zksync-era/issues/2388)) ([fb4d700](https://github.com/matter-labs/zksync-era/commit/fb4d7008db919281f7a328c0baaaa5b93c5166c1))
* BWIP ([#2258](https://github.com/matter-labs/zksync-era/issues/2258)) ([75bdfcc](https://github.com/matter-labs/zksync-era/commit/75bdfcc0ef4a99d93ac152db12a59ef2b2af0d27))
* **config:** Make getaway_url optional ([#2412](https://github.com/matter-labs/zksync-era/issues/2412)) ([200bc82](https://github.com/matter-labs/zksync-era/commit/200bc825032b18ad9d8f3f49d4eb7cb0e1b5b645))
* consensus support for pruning (BFT-473) ([#2334](https://github.com/matter-labs/zksync-era/issues/2334)) ([abc4256](https://github.com/matter-labs/zksync-era/commit/abc4256570b899e2b47ed8362e69ae0150247490))
* **contract-verifier:** Add file based config for contract verifier ([#2415](https://github.com/matter-labs/zksync-era/issues/2415)) ([f4410e3](https://github.com/matter-labs/zksync-era/commit/f4410e3254dafdfe400e1c2c420f664ba951e2cd))
* **en:** file based configs for en ([#2110](https://github.com/matter-labs/zksync-era/issues/2110)) ([7940fa3](https://github.com/matter-labs/zksync-era/commit/7940fa32a27ee4de43753c7083f92ca8c2ebe86b))
* **en:** Unify snapshot recovery and recovery from L1 ([#2256](https://github.com/matter-labs/zksync-era/issues/2256)) ([e03a929](https://github.com/matter-labs/zksync-era/commit/e03a9293852288b36d23f5ccbc784876435dd18d))
* **eth-sender:** Add transient ethereum gateway errors metric ([#2323](https://github.com/matter-labs/zksync-era/issues/2323)) ([287958d](https://github.com/matter-labs/zksync-era/commit/287958db6ca54959fd56c04d4a7a3cbfc9baa877))
* **eth-sender:** handle transactions for different operators separately to increase throughtput ([#2341](https://github.com/matter-labs/zksync-era/issues/2341)) ([0619ecc](https://github.com/matter-labs/zksync-era/commit/0619eccc335311298bfc0c75f0a4bf8562db759e))
* **eth-sender:** separate gas calculations for blobs transactions ([#2247](https://github.com/matter-labs/zksync-era/issues/2247)) ([627aab9](https://github.com/matter-labs/zksync-era/commit/627aab9703c47795247f8b6d21533520498ed025))
* **gas_adjuster:** Use eth_feeHistory for both base fee and blobs ([#2322](https://github.com/matter-labs/zksync-era/issues/2322)) ([9985c26](https://github.com/matter-labs/zksync-era/commit/9985c2659177656788a1f6143120eafccfccdae9))
* L1 batch QC database (BFT-476) ([#2340](https://github.com/matter-labs/zksync-era/issues/2340)) ([5886b8d](https://github.com/matter-labs/zksync-era/commit/5886b8df304ded15104ec228e0477bc5f44b7fbe))
* **metadata-calculator:** option to use VM runner for protective reads ([#2318](https://github.com/matter-labs/zksync-era/issues/2318)) ([c147b0c](https://github.com/matter-labs/zksync-era/commit/c147b0c68e6e1db5bd658c4f7a591bf3cddb9417))
* Minimal External API Fetcher ([#2383](https://github.com/matter-labs/zksync-era/issues/2383)) ([9f255c0](https://github.com/matter-labs/zksync-era/commit/9f255c073cfdab60832fcf9a6d3a4a9258641ef3))
* **node_framework:** Document implementations ([#2319](https://github.com/matter-labs/zksync-era/issues/2319)) ([7b3877f](https://github.com/matter-labs/zksync-era/commit/7b3877fd35b5c894fbe18666953eace8910dba0c))
* **node_framework:** Implement FromContext and IntoContext derive macro ([#2330](https://github.com/matter-labs/zksync-era/issues/2330)) ([34f2a45](https://github.com/matter-labs/zksync-era/commit/34f2a45e073052519697f41f264d05fa187ea678))
* **node_framework:** Support shutdown hooks + more ([#2293](https://github.com/matter-labs/zksync-era/issues/2293)) ([2b2c790](https://github.com/matter-labs/zksync-era/commit/2b2c790b64beb59a885ce785ab01d5c1bd089c43))
* **node_framework:** Unify Task types + misc improvements   ([#2325](https://github.com/matter-labs/zksync-era/issues/2325)) ([298a97e](https://github.com/matter-labs/zksync-era/commit/298a97e800b4c156628050789de7a490a7565d60))
* **node-framework:** New wiring interface ([#2384](https://github.com/matter-labs/zksync-era/issues/2384)) ([f2f4056](https://github.com/matter-labs/zksync-era/commit/f2f405669ec9f6edd3f2d5e5c1248582c5962ae8))
* **prover:** Add prometheus port to witness generator config ([#2385](https://github.com/matter-labs/zksync-era/issues/2385)) ([d0e1add](https://github.com/matter-labs/zksync-era/commit/d0e1addfccf6b5d3b21facd6bb74455f098f0177))
* **prover:** Add prover_cli stats command ([#2362](https://github.com/matter-labs/zksync-era/issues/2362)) ([fe65319](https://github.com/matter-labs/zksync-era/commit/fe65319da0f26ca45e95f067c1e8b97cf7874c45))
* **snapshots_applier:** Add a method to check whether snapshot recovery is done ([#2338](https://github.com/matter-labs/zksync-era/issues/2338)) ([610a7cf](https://github.com/matter-labs/zksync-era/commit/610a7cf037c6c655564deffebbf5a3fe5533783b))
* Switch to using crates.io deps ([#2409](https://github.com/matter-labs/zksync-era/issues/2409)) ([27fabaf](https://github.com/matter-labs/zksync-era/commit/27fabafbec66bf4cb65c4fa9e3fab4c3c981d0f2))
* **tee:** add Prometheus metrics to the TEE Prover ([#2386](https://github.com/matter-labs/zksync-era/issues/2386)) ([6153e99](https://github.com/matter-labs/zksync-era/commit/6153e9956065bfb04b94cc909315a6f1b6fdd364))
* **tee:** TEE Prover Gateway ([#2333](https://github.com/matter-labs/zksync-era/issues/2333)) ([f8df34d](https://github.com/matter-labs/zksync-era/commit/f8df34d9bff5e165fe40d4f67afa582a84038303))
* Unify and port node storage initialization  ([#2363](https://github.com/matter-labs/zksync-era/issues/2363)) ([8ea9791](https://github.com/matter-labs/zksync-era/commit/8ea979171e56af20c779e08fb2c55be30f655149))
* Validium with DA ([#2010](https://github.com/matter-labs/zksync-era/issues/2010)) ([fe03d0e](https://github.com/matter-labs/zksync-era/commit/fe03d0e254a98fea60ecb7485a7de9e7fdecaee1))
* **vm-runner:** make vm runner report time taken ([#2369](https://github.com/matter-labs/zksync-era/issues/2369)) ([275a333](https://github.com/matter-labs/zksync-era/commit/275a3337840c6722c2cd16241c785ff507da4521))
* **zk toolbox:** External node support ([#2287](https://github.com/matter-labs/zksync-era/issues/2287)) ([6384cad](https://github.com/matter-labs/zksync-era/commit/6384cad26aead4d1bdbb606a97d623dacebf912c))
* **zk_toolbox:** Add prover init command ([#2298](https://github.com/matter-labs/zksync-era/issues/2298)) ([159af3c](https://github.com/matter-labs/zksync-era/commit/159af3c54cc9beb742b2ab43ce3b89b14c8368b7))


### Bug Fixes

* **api:** fix log timestamp format ([#2407](https://github.com/matter-labs/zksync-era/issues/2407)) ([e9d63db](https://github.com/matter-labs/zksync-era/commit/e9d63dbe357a07fb07c7d35389b99e7b1ae47402))
* BWIP race condition ([#2405](https://github.com/matter-labs/zksync-era/issues/2405)) ([8099ab0](https://github.com/matter-labs/zksync-era/commit/8099ab0b77da3168a4184611adecb98a7d32fbaa))
* **config:** Implement proper tests ([#2381](https://github.com/matter-labs/zksync-era/issues/2381)) ([2ec494b](https://github.com/matter-labs/zksync-era/commit/2ec494bf6917bbce8a6e4e0c61ad77bf006815ec))
* **db:** Fix / extend transaction isolation levels ([#2350](https://github.com/matter-labs/zksync-era/issues/2350)) ([404ceb9](https://github.com/matter-labs/zksync-era/commit/404ceb91e9a179c269baed4d218261aae48a8061))
* **en:** Fix panics when queuing sync actions during shutdown ([d5935c7](https://github.com/matter-labs/zksync-era/commit/d5935c77b1496f24b829fe8e7f1c019ec6848db0))
* **erc20-test:** only approving baseToken allowance when needed ([#2379](https://github.com/matter-labs/zksync-era/issues/2379)) ([087a3c4](https://github.com/matter-labs/zksync-era/commit/087a3c4d01992c2173eb35ada24c63f290ef6140))
* **eth-sender:** confirm eth-txs in order of their creation ([#2310](https://github.com/matter-labs/zksync-era/issues/2310)) ([31a1a04](https://github.com/matter-labs/zksync-era/commit/31a1a04183c213cf1270e1487e05d6f9548c0afd))
* **eth-sender:** fix query returning inflight txs ([#2404](https://github.com/matter-labs/zksync-era/issues/2404)) ([6a89ca0](https://github.com/matter-labs/zksync-era/commit/6a89ca077c02c1d1bba511409d4e4196642205a6))
* **eth-sender:** missing fix in second query calculating txs unsent txs ([#2406](https://github.com/matter-labs/zksync-era/issues/2406)) ([948b532](https://github.com/matter-labs/zksync-era/commit/948b532ff4c94a80689e7906791d03cef64e3804))
* **eth-sender:** revert commit changing which type of txs we resend first ([#2327](https://github.com/matter-labs/zksync-era/issues/2327)) ([ef75292](https://github.com/matter-labs/zksync-era/commit/ef752926691d768ea412d0fdc78f43a62f16cd15))
* Fix rustls setup for jsonrpsee clients ([#2417](https://github.com/matter-labs/zksync-era/issues/2417)) ([a040f09](https://github.com/matter-labs/zksync-era/commit/a040f099cd9863d47d49cbdb3360e53a82e0423e))
* **merkle-tree:** Change `LazyAsyncTreeReader::wait()` signature ([#2314](https://github.com/matter-labs/zksync-era/issues/2314)) ([408393c](https://github.com/matter-labs/zksync-era/commit/408393c7d8ceee0ae95cbc1f2b24a3375e345e97))
* **merkle-tree:** Fix chunk recovery reporting during tree recovery ([#2348](https://github.com/matter-labs/zksync-era/issues/2348)) ([70b3a8a](https://github.com/matter-labs/zksync-era/commit/70b3a8aea33820d5bf932b608c9e68ecc2915d4c))
* **merkle-tree:** Fix connection timeouts during tree pruning ([#2372](https://github.com/matter-labs/zksync-era/issues/2372)) ([d5935c7](https://github.com/matter-labs/zksync-era/commit/d5935c77b1496f24b829fe8e7f1c019ec6848db0))
* **object-store:** Consider some token source errors transient ([#2331](https://github.com/matter-labs/zksync-era/issues/2331)) ([85386d3](https://github.com/matter-labs/zksync-era/commit/85386d314a934b7eaa0bf2707f6d5af039e93340))
* **tee:** Introduce a 1 second delay in the batch poll ([#2398](https://github.com/matter-labs/zksync-era/issues/2398)) ([312defe](https://github.com/matter-labs/zksync-era/commit/312defed86fbbbc1dfee489be373af1417ee624a))
* **vm-runner:** change `processing_started_at` column type to `timestamp` ([#2397](https://github.com/matter-labs/zksync-era/issues/2397)) ([4221155](https://github.com/matter-labs/zksync-era/commit/4221155d7f7467a1a8d57c4cbb8f1d9de3bac9e3))


### Reverts

* "refactor: Rename consensus tasks and split storage (BFT-476)" ([#2364](https://github.com/matter-labs/zksync-era/issues/2364)) ([e67ec5d](https://github.com/matter-labs/zksync-era/commit/e67ec5de15d01a0edce741efd6f5fe126ce76290))

## [24.8.0](https://github.com/matter-labs/zksync-era/compare/core-v24.7.0...core-v24.8.0) (2024-06-24)


### ⚠ BREAKING CHANGES

* updated boojum and nightly rust compiler ([#2126](https://github.com/matter-labs/zksync-era/issues/2126))

### Features

* Add metrics for transaction execution result in state keeper ([#2021](https://github.com/matter-labs/zksync-era/issues/2021)) ([dde0fc4](https://github.com/matter-labs/zksync-era/commit/dde0fc4b469474525fd5e4fd1594c3710d6d91f5))
* **api:** Add new `l1_committed` block tag ([#2282](https://github.com/matter-labs/zksync-era/issues/2282)) ([d5e8e9b](https://github.com/matter-labs/zksync-era/commit/d5e8e9bc66ff38b828730b62d8a7b8794cb1758a))
* **api:** Rework zks_getProtocolVersion ([#2146](https://github.com/matter-labs/zksync-era/issues/2146)) ([800b8f4](https://github.com/matter-labs/zksync-era/commit/800b8f456282685e81d3423ba3e27d017db2f183))
* change `zkSync` occurences to `ZKsync` ([#2227](https://github.com/matter-labs/zksync-era/issues/2227)) ([0b4104d](https://github.com/matter-labs/zksync-era/commit/0b4104dbb996ec6333619ea05f3a99e6d4f3b8fa))
* **contract-verifier:** Adjust contract verifier for zksolc 1.5.0 ([#2255](https://github.com/matter-labs/zksync-era/issues/2255)) ([63efb2e](https://github.com/matter-labs/zksync-era/commit/63efb2e530d8b1445bdd58537d6f0cdb5593cd75))
* **docs:** Add documentation for subset of wiring layer implementations, used by Main node ([#2292](https://github.com/matter-labs/zksync-era/issues/2292)) ([06c287b](https://github.com/matter-labs/zksync-era/commit/06c287b630707843fd92cb88f899a8fd1dcc7147))
* **docs:** Pruning and Snapshots recovery basic docs ([#2265](https://github.com/matter-labs/zksync-era/issues/2265)) ([619a525](https://github.com/matter-labs/zksync-era/commit/619a525bc8f1098297259ddb296b4b5dee223944))
* **en:** Allow recovery from specific snapshot ([#2137](https://github.com/matter-labs/zksync-era/issues/2137)) ([ac61fed](https://github.com/matter-labs/zksync-era/commit/ac61fedb5756ed700e35f231a364b9c933423ab8))
* **eth-sender:** fix for missing eth_txs_history entries ([#2236](https://github.com/matter-labs/zksync-era/issues/2236)) ([f05b0ae](https://github.com/matter-labs/zksync-era/commit/f05b0aefbb04ce715431bf039b8760e95f87dc93))
* Expose fair_pubdata_price for blocks and batches ([#2244](https://github.com/matter-labs/zksync-era/issues/2244)) ([0d51cd6](https://github.com/matter-labs/zksync-era/commit/0d51cd6f3e65eef1bda981fe96f3026d8e12156d))
* **merkle-tree:** Rework tree rollback ([#2207](https://github.com/matter-labs/zksync-era/issues/2207)) ([c3b9c38](https://github.com/matter-labs/zksync-era/commit/c3b9c38ca07f01e6f7b2d7e631b2b811cacecf3a))
* **node-framework:** Add Main Node Client layer ([#2132](https://github.com/matter-labs/zksync-era/issues/2132)) ([927d842](https://github.com/matter-labs/zksync-era/commit/927d8427e05b6d1a3aa9a63ee8e0db4fb1b82094))
* **node:** Move some stuff around ([#2151](https://github.com/matter-labs/zksync-era/issues/2151)) ([bad5a6c](https://github.com/matter-labs/zksync-era/commit/bad5a6c0ec2e166235418a2796b6ccf6f8b3b05f))
* **node:** Port (most of) Node to the Node Framework ([#2196](https://github.com/matter-labs/zksync-era/issues/2196)) ([7842bc4](https://github.com/matter-labs/zksync-era/commit/7842bc4842c5c92437639105d8edac5f775ad0e6))
* **object-store:** Allow caching object store objects locally ([#2153](https://github.com/matter-labs/zksync-era/issues/2153)) ([6c6e65c](https://github.com/matter-labs/zksync-era/commit/6c6e65ce646bcb4ed9ba8b2dd6be676bb6e66324))
* **proof_data_handler:** add new endpoints to the TEE prover interface API ([#1993](https://github.com/matter-labs/zksync-era/issues/1993)) ([eca98cc](https://github.com/matter-labs/zksync-era/commit/eca98cceeb74a979040279caaf1d05d1fdf1b90c))
* **prover:** Add file based config for fri prover gateway ([#2150](https://github.com/matter-labs/zksync-era/issues/2150)) ([81ffc6a](https://github.com/matter-labs/zksync-era/commit/81ffc6a753fb72747c01ddc8a37211bf6a8a1a27))
* Remove initialize_components function ([#2284](https://github.com/matter-labs/zksync-era/issues/2284)) ([0a38891](https://github.com/matter-labs/zksync-era/commit/0a388911914bfcf58785e394db9d5ddce3afdef0))
* **state-keeper:** Add metric for l2 block seal reason ([#2229](https://github.com/matter-labs/zksync-era/issues/2229)) ([f967e6d](https://github.com/matter-labs/zksync-era/commit/f967e6d20bb7f9192af08e5040c58af97585862d))
* **state-keeper:** More state keeper metrics ([#2224](https://github.com/matter-labs/zksync-era/issues/2224)) ([1e48cd9](https://github.com/matter-labs/zksync-era/commit/1e48cd99a0e5ea8bedff91135938dbbb70141d43))
* **sync-layer:** adapt MiniMerkleTree to manage priority queue ([#2068](https://github.com/matter-labs/zksync-era/issues/2068)) ([3e72364](https://github.com/matter-labs/zksync-era/commit/3e7236494e346324fe1254038632ee005e0083e5))
* **tee_verifier_input_producer:** use `FactoryDepsDal::get_factory_deps() ([#2271](https://github.com/matter-labs/zksync-era/issues/2271)) ([2c0a00a](https://github.com/matter-labs/zksync-era/commit/2c0a00add179cc4ed521bbb9d616b8828f0ad3c1))
* **toolbox:** add zk_toolbox ci ([#1985](https://github.com/matter-labs/zksync-era/issues/1985)) ([4ab4922](https://github.com/matter-labs/zksync-era/commit/4ab492201a1654a254c0b14a382a2cb67e3cb9e5))
* updated boojum and nightly rust compiler ([#2126](https://github.com/matter-labs/zksync-era/issues/2126)) ([9e39f13](https://github.com/matter-labs/zksync-era/commit/9e39f13c29788e66645ea57f623555c4b36b8aff))
* upgraded encoding of transactions in consensus Payload. ([#2245](https://github.com/matter-labs/zksync-era/issues/2245)) ([cb6a6c8](https://github.com/matter-labs/zksync-era/commit/cb6a6c88de54806d0f4ae4af7ea873a911605780))
* Use info log level for crates named zksync_* by default ([#2296](https://github.com/matter-labs/zksync-era/issues/2296)) ([9303142](https://github.com/matter-labs/zksync-era/commit/9303142de5e6af3da69fa836a7e537287bdde4b0))
* verification of L1Batch witness (BFT-471) - attempt 2 ([#2232](https://github.com/matter-labs/zksync-era/issues/2232)) ([dbcf3c6](https://github.com/matter-labs/zksync-era/commit/dbcf3c6d02a6bfb9197bf4278f296632b0fd7d66))
* verification of L1Batch witness (BFT-471) ([#2019](https://github.com/matter-labs/zksync-era/issues/2019)) ([6cc5455](https://github.com/matter-labs/zksync-era/commit/6cc54555972804be4cd2ca118f0e425c490fbfca))
* **vm-runner:** add basic metrics ([#2203](https://github.com/matter-labs/zksync-era/issues/2203)) ([dd154f3](https://github.com/matter-labs/zksync-era/commit/dd154f388c23ff67068a1053fec878e80ba9bd17))
* **vm-runner:** add protective reads persistence flag for state keeper ([#2307](https://github.com/matter-labs/zksync-era/issues/2307)) ([36d2eb6](https://github.com/matter-labs/zksync-era/commit/36d2eb651a583293a5103dc990813e74e8532f52))
* **vm-runner:** shadow protective reads using VM runner ([#2017](https://github.com/matter-labs/zksync-era/issues/2017)) ([1402dd0](https://github.com/matter-labs/zksync-era/commit/1402dd054e3248de55bcc6899bb58a2cfe900473))


### Bug Fixes

* **api:** Fix getting pending block ([#2186](https://github.com/matter-labs/zksync-era/issues/2186)) ([93315ba](https://github.com/matter-labs/zksync-era/commit/93315ba95c54bd0730c964998bfc0c64080b3c04))
* **api:** Fix transaction methods for pruned transactions ([#2168](https://github.com/matter-labs/zksync-era/issues/2168)) ([00c4cca](https://github.com/matter-labs/zksync-era/commit/00c4cca1635e6cd17bbc74e7841f47ead7f8e445))
* **config:** Fix object store ([#2183](https://github.com/matter-labs/zksync-era/issues/2183)) ([551cdc2](https://github.com/matter-labs/zksync-era/commit/551cdc2da38dbd2ca1f07e9a49f9f2745f21556a))
* **config:** Split object stores ([#2187](https://github.com/matter-labs/zksync-era/issues/2187)) ([9bcdabc](https://github.com/matter-labs/zksync-era/commit/9bcdabcaa8462ae19da1688052a7a78fa4108298))
* **db:** Fix `insert_proof_generation_details()` ([#2291](https://github.com/matter-labs/zksync-era/issues/2291)) ([c2412cf](https://github.com/matter-labs/zksync-era/commit/c2412cf2421448c706a08e3c8fda3b0af6aac497))
* **db:** Optimize `get_l2_blocks_to_execute_for_l1_batch` ([#2199](https://github.com/matter-labs/zksync-era/issues/2199)) ([06ec5f3](https://github.com/matter-labs/zksync-era/commit/06ec5f3e6bb66025a3ec1e5b4d314c7ff1e116c7))
* **en:** Fix reorg detection in presence of tree data fetcher ([#2197](https://github.com/matter-labs/zksync-era/issues/2197)) ([20da566](https://github.com/matter-labs/zksync-era/commit/20da5668a42a11cc0ea07f9d1a5d5c39e32ce3b4))
* **en:** Fix transient error detection in consistency checker ([#2140](https://github.com/matter-labs/zksync-era/issues/2140)) ([38fdfe0](https://github.com/matter-labs/zksync-era/commit/38fdfe083f61f5aad11b5a0efb41215c674f3186))
* **en:** Remove L1 client health check ([#2136](https://github.com/matter-labs/zksync-era/issues/2136)) ([49198f6](https://github.com/matter-labs/zksync-era/commit/49198f695a93d24a5e2d37a24b2c5e1b6c70b9c5))
* **eth-sender:** Don't resend already sent transactions in the same block ([#2208](https://github.com/matter-labs/zksync-era/issues/2208)) ([3538e9c](https://github.com/matter-labs/zksync-era/commit/3538e9c346ef7bacf62fd76874d41548a4be46ea))
* **eth-sender:** etter error handling in eth-sender ([#2163](https://github.com/matter-labs/zksync-era/issues/2163)) ([0cad504](https://github.com/matter-labs/zksync-era/commit/0cad504b1c40399a24b604c3454ae4ab98550ad6))
* **node_framework:** Run gas adjuster task only if necessary ([#2266](https://github.com/matter-labs/zksync-era/issues/2266)) ([2dac846](https://github.com/matter-labs/zksync-era/commit/2dac8463376b5ca7cb3aeefab83b9220f3b2466a))
* **object-store:** Consider more GCS errors transient ([#2246](https://github.com/matter-labs/zksync-era/issues/2246)) ([2f6cd41](https://github.com/matter-labs/zksync-era/commit/2f6cd41642d9c2680f17e5c1adf22ad8e1b0288a))
* **prover_cli:** Remove outdated fix for circuit id in node wg ([#2248](https://github.com/matter-labs/zksync-era/issues/2248)) ([db8e71b](https://github.com/matter-labs/zksync-era/commit/db8e71b55393b3d0e419886b62712b61305ac030))
* **prover:** Disallow state changes from successful ([#2233](https://github.com/matter-labs/zksync-era/issues/2233)) ([2488a76](https://github.com/matter-labs/zksync-era/commit/2488a767a362ea3b40a348ae9822bed77d4b8de9))
* **pruning:** Check pruning in metadata calculator ([#2286](https://github.com/matter-labs/zksync-era/issues/2286)) ([7bd8f27](https://github.com/matter-labs/zksync-era/commit/7bd8f27e5171f37da3aa1d6c6abb06b9a291fbbf))
* Treat 502s and 503s as transient for GCS OS ([#2202](https://github.com/matter-labs/zksync-era/issues/2202)) ([0a12c52](https://github.com/matter-labs/zksync-era/commit/0a12c5224b0b6b6d937311e6d6d81c26b03b1d9d))
* **vm-runner:** add config value for the first processed batch ([#2158](https://github.com/matter-labs/zksync-era/issues/2158)) ([f666717](https://github.com/matter-labs/zksync-era/commit/f666717e01beb90ff878d1cdf060284b27faf680))
* **vm-runner:** make `last_ready_batch` account for `first_processed_batch` ([#2238](https://github.com/matter-labs/zksync-era/issues/2238)) ([3889794](https://github.com/matter-labs/zksync-era/commit/38897947439db539920d97f2318b2133ddc40284))
* **vm:** fix insertion to `decommitted_code_hashes` ([#2275](https://github.com/matter-labs/zksync-era/issues/2275)) ([15bb71e](https://github.com/matter-labs/zksync-era/commit/15bb71ec3ee66796e62cb7e61dec6e496e1f2774))
* **vm:** Update `decommitted_code_hashes` in `prepare_to_decommit` ([#2253](https://github.com/matter-labs/zksync-era/issues/2253)) ([6c49a50](https://github.com/matter-labs/zksync-era/commit/6c49a50eb4374a06143e5bac130d0e0e74347597))


### Performance Improvements

* **db:** Improve storage switching for state keeper cache ([#2234](https://github.com/matter-labs/zksync-era/issues/2234)) ([7c8e24c](https://github.com/matter-labs/zksync-era/commit/7c8e24ce7d6e6d47359d5ae4ab1db4ddbd3e9441))
* **db:** Try yet another storage log pruning approach ([#2268](https://github.com/matter-labs/zksync-era/issues/2268)) ([3ee34be](https://github.com/matter-labs/zksync-era/commit/3ee34be7e48fb4b7c5030a6422a0a9f8a8ebc35b))
* **en:** Parallelize persistence and chunk processing during tree recovery ([#2050](https://github.com/matter-labs/zksync-era/issues/2050)) ([b08a667](https://github.com/matter-labs/zksync-era/commit/b08a667c819f8b3d222c237fc4447be6b75d334e))
* **pruning:** Use more efficient query to delete past storage logs ([#2179](https://github.com/matter-labs/zksync-era/issues/2179)) ([4c18755](https://github.com/matter-labs/zksync-era/commit/4c18755876a42ee81840cadb365b3040194d0ae3))


### Reverts

* **pruning:** Revert pruning query ([#2220](https://github.com/matter-labs/zksync-era/issues/2220)) ([8427cdd](https://github.com/matter-labs/zksync-era/commit/8427cddcbd5ba13388e5b96fb988128f8dabe0f4))
* verification of L1Batch witness (BFT-471) ([#2230](https://github.com/matter-labs/zksync-era/issues/2230)) ([227e101](https://github.com/matter-labs/zksync-era/commit/227e10180396fbb54a2e99cab775f13bc93745f3))

## [24.7.0](https://github.com/matter-labs/zksync-era/compare/core-v24.6.0...core-v24.7.0) (2024-06-03)


### Features

* **node-framework:** Add reorg detector  ([#1551](https://github.com/matter-labs/zksync-era/issues/1551)) ([7c7d352](https://github.com/matter-labs/zksync-era/commit/7c7d352708aa64b55a9b33e273b1a16d3f1d168b))


### Bug Fixes

* **block-reverter:** Fix reverting snapshot files ([#2064](https://github.com/matter-labs/zksync-era/issues/2064)) ([17a7e78](https://github.com/matter-labs/zksync-era/commit/17a7e782d9e35eaf38acf920c2326d4037c7781e))
* **env:** Do not print stacktrace for locate workspace ([#2111](https://github.com/matter-labs/zksync-era/issues/2111)) ([5f2677f](https://github.com/matter-labs/zksync-era/commit/5f2677f2c966f4dd23538a02ecd7fffe306bec7f))
* **eth-watch:** make assert less strict ([#2129](https://github.com/matter-labs/zksync-era/issues/2129)) ([e9bab95](https://github.com/matter-labs/zksync-era/commit/e9bab95539af383c161b357a422d5c45f20f27aa))

## [24.6.0](https://github.com/matter-labs/zksync-era/compare/core-v24.5.1...core-v24.6.0) (2024-06-03)


### Features

* **en:** Fetch old L1 batch hashes from L1 ([#2000](https://github.com/matter-labs/zksync-era/issues/2000)) ([dc5a918](https://github.com/matter-labs/zksync-era/commit/dc5a9188a44a51810c9b7609a0887090043507f2))
* use semver for metrics, move constants to prover workspace ([#2098](https://github.com/matter-labs/zksync-era/issues/2098)) ([7a50a9f](https://github.com/matter-labs/zksync-era/commit/7a50a9f79e516ec150d1f30b9f1c781a5523375b))


### Bug Fixes

* **api:** correct default fee data in eth call ([#2072](https://github.com/matter-labs/zksync-era/issues/2072)) ([e71f6f9](https://github.com/matter-labs/zksync-era/commit/e71f6f96bda08f8330c643a31df4ef9e82c9afc2))

## [24.5.1](https://github.com/matter-labs/zksync-era/compare/core-v24.5.0...core-v24.5.1) (2024-05-31)


### Bug Fixes

* **house-keeper:** Fix queue size queries ([#2106](https://github.com/matter-labs/zksync-era/issues/2106)) ([183502a](https://github.com/matter-labs/zksync-era/commit/183502a17eb47a747f50b6a9d38ab78de984f80e))

## [24.5.0](https://github.com/matter-labs/zksync-era/compare/core-v24.4.0...core-v24.5.0) (2024-05-30)


### Features

* Add protocol_version label to WG jobs metric ([#2009](https://github.com/matter-labs/zksync-era/issues/2009)) ([e0a3393](https://github.com/matter-labs/zksync-era/commit/e0a33931f9bb9429eff362deaa1500fe914971c7))
* **config:** remove zksync home ([#2022](https://github.com/matter-labs/zksync-era/issues/2022)) ([d08fe81](https://github.com/matter-labs/zksync-era/commit/d08fe81f4ec6c3aaeb5ad98351e44a63e5b100be))
* **en:** Improve tree snapshot recovery ([#1938](https://github.com/matter-labs/zksync-era/issues/1938)) ([5bc8234](https://github.com/matter-labs/zksync-era/commit/5bc8234aae57c0d0f492b94860483a53d044b323))
* Make house keeper emit correct protocol version ([#2062](https://github.com/matter-labs/zksync-era/issues/2062)) ([a58a7e8](https://github.com/matter-labs/zksync-era/commit/a58a7e8ec8599eb957e5693308b789e7ace5c126))
* **node_framework:** Migrate main node to the framework ([#1997](https://github.com/matter-labs/zksync-era/issues/1997)) ([27a26cb](https://github.com/matter-labs/zksync-era/commit/27a26cbb955ee8dd59140386af90816a1a44ab99))
* **node_framework:** Synchronize pools layer with logic in initialize_components ([#2079](https://github.com/matter-labs/zksync-era/issues/2079)) ([3202461](https://github.com/matter-labs/zksync-era/commit/3202461788052f0bf4a55738b9b59a13b6a83ca6))
* Protocol semantic version ([#2059](https://github.com/matter-labs/zksync-era/issues/2059)) ([3984dcf](https://github.com/matter-labs/zksync-era/commit/3984dcfbdd890f0862c9c0f3e7757fb8b0c8184a))
* **prover:** Adnotate prover queue metrics with protocol version ([#1893](https://github.com/matter-labs/zksync-era/issues/1893)) ([d1e1004](https://github.com/matter-labs/zksync-era/commit/d1e1004416b7e9db47e242ff68f01b5520834e94))
* save writes needed for tree in state keeper ([#1965](https://github.com/matter-labs/zksync-era/issues/1965)) ([471af53](https://github.com/matter-labs/zksync-era/commit/471af539db6d965852360f8c0978744061a932eb))
* **test:** Add filebased config support for integration tests ([#2043](https://github.com/matter-labs/zksync-era/issues/2043)) ([be3ded9](https://github.com/matter-labs/zksync-era/commit/be3ded97ede1caea69b4881b783c7b40861d183d))
* **vm-runner:** implement VM runner main body ([#1955](https://github.com/matter-labs/zksync-era/issues/1955)) ([bf5b6c2](https://github.com/matter-labs/zksync-era/commit/bf5b6c2e5491b14920fd881388cbfdb6d7b4aa91))


### Bug Fixes

* **API:** polish web3 api  block-related types ([#1994](https://github.com/matter-labs/zksync-era/issues/1994)) ([6cd3c53](https://github.com/matter-labs/zksync-era/commit/6cd3c532190ee96a9ca56336d20837d249d6207e))
* **en:** chunk factory deps  ([#2077](https://github.com/matter-labs/zksync-era/issues/2077)) ([4b9e6fa](https://github.com/matter-labs/zksync-era/commit/4b9e6faead8df7119f4617f4d4ec2f4ac348c174))
* **en:** Fix recovery-related metrics ([#2014](https://github.com/matter-labs/zksync-era/issues/2014)) ([86355d6](https://github.com/matter-labs/zksync-era/commit/86355d647fca772a7c665a8534ab02e8a213cf7b))
* **eth-watch:** Do not track for stm, only for diamond proxy ([#2080](https://github.com/matter-labs/zksync-era/issues/2080)) ([87adac9](https://github.com/matter-labs/zksync-era/commit/87adac9c4f5470e82e46eeef892442adb6948713))
* fix metrics reporting wrong values ([#2065](https://github.com/matter-labs/zksync-era/issues/2065)) ([2ec010a](https://github.com/matter-labs/zksync-era/commit/2ec010aa15dc04f367fc7276ab01afcf211f57b4))
* **loadtest:** resolve unit conversion error in loadtest metrics ([#1987](https://github.com/matter-labs/zksync-era/issues/1987)) ([b5870a0](https://github.com/matter-labs/zksync-era/commit/b5870a0b9c470ed38dfe4c67036139a3a1d7dddc))
* **merkle-tree:** Fix incoherent Merkle tree view ([#2071](https://github.com/matter-labs/zksync-era/issues/2071)) ([2fc9a6c](https://github.com/matter-labs/zksync-era/commit/2fc9a6cdb659bd16694c568d16a5b76af063c730))
* **metadata-calculator:** protective reads sort ([#2087](https://github.com/matter-labs/zksync-era/issues/2087)) ([160c13c](https://github.com/matter-labs/zksync-era/commit/160c13c576faaeb490309c2f5a10e4de1d90f7cc))
* **node_framework:** Fix the connection pool size for the catchup task ([#2046](https://github.com/matter-labs/zksync-era/issues/2046)) ([c00a2eb](https://github.com/matter-labs/zksync-era/commit/c00a2eb21fe1670386364c7ced38f562471ed7f5))
* **node_framework:** Use custom pool for commitiment generator ([#2076](https://github.com/matter-labs/zksync-era/issues/2076)) ([994df8f](https://github.com/matter-labs/zksync-era/commit/994df8f85cd65d032fb5ce991df89fdc319c24e2))
* **protocol_version:** Add backward compatibility ([#2097](https://github.com/matter-labs/zksync-era/issues/2097)) ([391624b](https://github.com/matter-labs/zksync-era/commit/391624b01b5fb4bdf52b8826205e35839746732f))
* **pruning:** Fix DB pruner responsiveness during shutdown ([#2058](https://github.com/matter-labs/zksync-era/issues/2058)) ([0a07312](https://github.com/matter-labs/zksync-era/commit/0a07312089833cd5da33009edd13ad253b263677))
* **zk_toolbox:** Use both folders for loading contracts  ([#2030](https://github.com/matter-labs/zksync-era/issues/2030)) ([97c6d5c](https://github.com/matter-labs/zksync-era/commit/97c6d5c9c2d9dddf0b18391077c8828e5dc7042b))


### Performance Improvements

* **commitment-generator:** Run commitment generation for multiple batches in parallel ([#1984](https://github.com/matter-labs/zksync-era/issues/1984)) ([602bf67](https://github.com/matter-labs/zksync-era/commit/602bf6725e7590fc67d8b027e07e0767fec9408b))

## [24.4.0](https://github.com/matter-labs/zksync-era/compare/core-v24.3.0...core-v24.4.0) (2024-05-21)


### Features

* **prover:** add GPU feature for compressor ([#1838](https://github.com/matter-labs/zksync-era/issues/1838)) ([e9a2213](https://github.com/matter-labs/zksync-era/commit/e9a2213985928cd3804a3855ccfde6a7d99da238))
* **pruning:** remove manual vaccum; add migration configuring autovacuum ([#1983](https://github.com/matter-labs/zksync-era/issues/1983)) ([3d98072](https://github.com/matter-labs/zksync-era/commit/3d98072468b1f7dac653b4ff04bda66e2fc8185e))
* **tests:** Move all env calls to one place in ts-tests ([#1968](https://github.com/matter-labs/zksync-era/issues/1968)) ([3300047](https://github.com/matter-labs/zksync-era/commit/33000475b47831fc3791dac338aae4d0e7db25b0))


### Bug Fixes

* Disallow non null updates for transactions ([#1951](https://github.com/matter-labs/zksync-era/issues/1951)) ([a603ac8](https://github.com/matter-labs/zksync-era/commit/a603ac8eaab112738e1c2336b0f537273ad58d85))
* **en:** Minor node fixes ([#1978](https://github.com/matter-labs/zksync-era/issues/1978)) ([74144e8](https://github.com/matter-labs/zksync-era/commit/74144e8240f633a587f0cd68f4d136a7a68af7be))
* **en:** run `MainNodeFeeParamsFetcher` in API component ([#1988](https://github.com/matter-labs/zksync-era/issues/1988)) ([b62677e](https://github.com/matter-labs/zksync-era/commit/b62677ea5f8f6bb57d6ad02139a938ccf943e06a))
* **merkle-tree:** Fix tree API health check status ([#1973](https://github.com/matter-labs/zksync-era/issues/1973)) ([6235561](https://github.com/matter-labs/zksync-era/commit/623556112c40400244906e42c5f84a047dc6f26b))

## [24.3.0](https://github.com/matter-labs/zksync-era/compare/core-v24.2.0...core-v24.3.0) (2024-05-16)


### Features

* Added support for making EN a (non-leader) consensus validator (BFT-426) ([#1905](https://github.com/matter-labs/zksync-era/issues/1905)) ([9973629](https://github.com/matter-labs/zksync-era/commit/9973629e35cec9af9eac81452631a2526dd336a8))
* **configs:** Extract secrets to an additional config  ([#1956](https://github.com/matter-labs/zksync-era/issues/1956)) ([bab4d65](https://github.com/matter-labs/zksync-era/commit/bab4d6579828e484453c84df417550bbaf1013b6))
* **en:** Fetch L1 batch root hashes from main node ([#1923](https://github.com/matter-labs/zksync-era/issues/1923)) ([72a3571](https://github.com/matter-labs/zksync-era/commit/72a357147391b6f7e6e1ee44bb2c22462316732b))
* **eth-client:** Generalize RPC client ([#1898](https://github.com/matter-labs/zksync-era/issues/1898)) ([a4e099f](https://github.com/matter-labs/zksync-era/commit/a4e099fe961f329ff2d604d657862819732446b4))
* **Prover CLI:** `requeue` cmd ([#1719](https://github.com/matter-labs/zksync-era/issues/1719)) ([f722df7](https://github.com/matter-labs/zksync-era/commit/f722df7c0ae429f43d047ff79e24bca39f81230c))
* **Prover CLI:** `status batch --verbose` ([#1899](https://github.com/matter-labs/zksync-era/issues/1899)) ([cf80184](https://github.com/matter-labs/zksync-era/commit/cf80184941a1fc62c3a755b99571d370949d8566))
* **pruning:** Vacuum freeze started daily ([#1929](https://github.com/matter-labs/zksync-era/issues/1929)) ([5c85e9f](https://github.com/matter-labs/zksync-era/commit/5c85e9fad350751c85cf6f2d1a9eb79d0e4503df))
* Remove metrics crate ([#1902](https://github.com/matter-labs/zksync-era/issues/1902)) ([5f7bda7](https://github.com/matter-labs/zksync-era/commit/5f7bda78c3fef7f324f8cbeaed2d7d41b7169d16))
* **state-keeper:** Parallel l2 block sealing ([#1801](https://github.com/matter-labs/zksync-era/issues/1801)) ([9b06dd8](https://github.com/matter-labs/zksync-era/commit/9b06dd848e85e20f2e94d2a0e858c3f207da5f47))
* tee_verifier_input_producer ([#1860](https://github.com/matter-labs/zksync-era/issues/1860)) ([fea7f16](https://github.com/matter-labs/zksync-era/commit/fea7f165cfb96bf673353ef562fb5c06f3e49736))
* **vm-runner:** implement output handler for VM runner ([#1856](https://github.com/matter-labs/zksync-era/issues/1856)) ([1e4aeb5](https://github.com/matter-labs/zksync-era/commit/1e4aeb57d36b347f9b1c7f2112b0af0471a6dbc9))


### Bug Fixes

* **basic_types:** bincode deserialization for `web3::Bytes` ([#1928](https://github.com/matter-labs/zksync-era/issues/1928)) ([406ec8c](https://github.com/matter-labs/zksync-era/commit/406ec8cb61ff2b7870ea0a1572e825133304048a))
* **config:** Fix data-handler-config ([#1919](https://github.com/matter-labs/zksync-era/issues/1919)) ([b6bb041](https://github.com/matter-labs/zksync-era/commit/b6bb041693811813f05dee0587b678afdc1d97a1))
* **en:** Delete old txs by (init_addr, nonce) ([#1942](https://github.com/matter-labs/zksync-era/issues/1942)) ([fa5f4a7](https://github.com/matter-labs/zksync-era/commit/fa5f4a7e442d4343ed112b448a035c6a0b8f1504))
* **en:** Fix reorg detector logic for dealing with last L1 batch ([#1906](https://github.com/matter-labs/zksync-era/issues/1906)) ([3af5f5b](https://github.com/matter-labs/zksync-era/commit/3af5f5b3f663c8586cf15698eee168918333a966))
* parentHash in websocket blocks subscription is shown as 0x0 ([#1946](https://github.com/matter-labs/zksync-era/issues/1946)) ([fc2efad](https://github.com/matter-labs/zksync-era/commit/fc2efad56e1a194b8945abf3fff1abfcd0b7da54))
* **Prover CLI:** `status batch` bugs ([#1865](https://github.com/matter-labs/zksync-era/issues/1865)) ([09682f2](https://github.com/matter-labs/zksync-era/commit/09682f2951f5f62fa0942057e96f855d78bf67c8))
* **utils:** bincode ser-/deserialization for `BytesToHexSerde` ([#1947](https://github.com/matter-labs/zksync-era/issues/1947)) ([a75b917](https://github.com/matter-labs/zksync-era/commit/a75b9174b73b1293f0b7f696daa6b21183fd7d19))

## [24.2.0](https://github.com/matter-labs/zksync-era/compare/core-v24.1.0...core-v24.2.0) (2024-05-14)


### Features

* **api:** Add zeppelinos well-known slots ([#1892](https://github.com/matter-labs/zksync-era/issues/1892)) ([1c041cc](https://github.com/matter-labs/zksync-era/commit/1c041ccd4226f7f9c520814f3af8d00d6d8784c7))
* **en:** Brush up EN observability config ([#1897](https://github.com/matter-labs/zksync-era/issues/1897)) ([086f768](https://github.com/matter-labs/zksync-era/commit/086f7683307b2b9c6a43cb8cf2c1a8a8874277a7))
* **node_framework:** Add tree api server & client to the metadata calculator ([#1885](https://github.com/matter-labs/zksync-era/issues/1885)) ([6dda157](https://github.com/matter-labs/zksync-era/commit/6dda15773295c36d4a8ef56d4e84e4b944829922))


### Bug Fixes

* **core/prover:** Changes to support Validium ([#1910](https://github.com/matter-labs/zksync-era/issues/1910)) ([1cb0dc5](https://github.com/matter-labs/zksync-era/commit/1cb0dc5504d226c55217accca87f2fd75addc917))
* **eth-client:** Fix call error detection ([#1890](https://github.com/matter-labs/zksync-era/issues/1890)) ([c22ce63](https://github.com/matter-labs/zksync-era/commit/c22ce639e08f78b5fd12f2ba7f1e419e8849b1ca))
* **eth-client:** Make block params non-optional ([#1882](https://github.com/matter-labs/zksync-era/issues/1882)) ([3005862](https://github.com/matter-labs/zksync-era/commit/3005862be1ea7c40dea0ae9442b3299b15cf20ca))
* **pruning:** Don't require metadata to exist for first L1 batches to be pruned ([#1850](https://github.com/matter-labs/zksync-era/issues/1850)) ([75c8565](https://github.com/matter-labs/zksync-era/commit/75c85654c4ac4638f3f0705485c95e72727880d4))
* **pruning:** query optimization ([#1904](https://github.com/matter-labs/zksync-era/issues/1904)) ([9154390](https://github.com/matter-labs/zksync-era/commit/9154390ff1d0c2d66a4a811795afa66e6c3c742e))

## [24.1.0](https://github.com/matter-labs/zksync-era/compare/core-v24.0.0...core-v24.1.0) (2024-05-08)


### Features

* add `sendRawTransactionWithDetailedOutput` API ([#1806](https://github.com/matter-labs/zksync-era/issues/1806)) ([6a30a31](https://github.com/matter-labs/zksync-era/commit/6a30a3161972461cb707ec5549aed6e837933a27))
* add getGasPerPubdataByte endpoint ([#1778](https://github.com/matter-labs/zksync-era/issues/1778)) ([d62dd08](https://github.com/matter-labs/zksync-era/commit/d62dd0801faae5442dc72e93a2c746769351cee0))
* **config:** Wrap sensitive urls ([#1828](https://github.com/matter-labs/zksync-era/issues/1828)) ([c8ee740](https://github.com/matter-labs/zksync-era/commit/c8ee740a4cc7dc9196d4223397e0bfc9fd8198cf))
* **db:** Implement weak references to RocksDB ([e0d4daa](https://github.com/matter-labs/zksync-era/commit/e0d4daa990c2563d2bd9a048d7350a545f136f00))
* **en:** Add pruning health checks and rework pruning config ([#1790](https://github.com/matter-labs/zksync-era/issues/1790)) ([e0d4daa](https://github.com/matter-labs/zksync-era/commit/e0d4daa990c2563d2bd9a048d7350a545f136f00))
* Extract proof_data_handler into separate crate ([#1677](https://github.com/matter-labs/zksync-era/issues/1677)) ([f4facee](https://github.com/matter-labs/zksync-era/commit/f4faceef1cb2f155dad37a61be3823a96b96c2ab))
* Extract several crates from zksync_core ([#1859](https://github.com/matter-labs/zksync-era/issues/1859)) ([7dcf796](https://github.com/matter-labs/zksync-era/commit/7dcf79606e0f37b468c82b6bdcb374149bc30f34))
* **node:** Extract genesis into separate crate ([#1797](https://github.com/matter-labs/zksync-era/issues/1797)) ([a8c4599](https://github.com/matter-labs/zksync-era/commit/a8c459951cea4692a7d7c9bdf3dc8323d2ad86ba))
* **Prover CLI:** `status batch` command ([#1638](https://github.com/matter-labs/zksync-era/issues/1638)) ([3fd6d65](https://github.com/matter-labs/zksync-era/commit/3fd6d653cad1e783f7a52eead13b322d4c6639a9))
* prover components versioning ([#1660](https://github.com/matter-labs/zksync-era/issues/1660)) ([29a4ffc](https://github.com/matter-labs/zksync-era/commit/29a4ffc6b9420590f32a9e1d1585ebffb95eeb6c))
* Update provers current version ([#1872](https://github.com/matter-labs/zksync-era/issues/1872)) ([bb5f129](https://github.com/matter-labs/zksync-era/commit/bb5f129deb75f9dad029da55ec8e2c0defefbe6e))


### Bug Fixes

* **basic_types:** bincode deserialization for `L2ChainId` ([#1835](https://github.com/matter-labs/zksync-era/issues/1835)) ([fde85f4](https://github.com/matter-labs/zksync-era/commit/fde85f4e182b38322d766fdd697a70f13d10ffac))
* **contract-verifier:** YUL system-mode verification ([#1863](https://github.com/matter-labs/zksync-era/issues/1863)) ([5aa7d41](https://github.com/matter-labs/zksync-era/commit/5aa7d415d4b03efd0d9c52d36cf1a1818e23efcf))
* **loadtest:** Do not initiate transactions with 0 amount ([#1847](https://github.com/matter-labs/zksync-era/issues/1847)) ([1bbe108](https://github.com/matter-labs/zksync-era/commit/1bbe108ae4f864f9d257a09d38e4baf22ea0b2c0))


### Performance Improvements

* **db:** Fine-tune state keeper cache performance / RAM usage ([#1804](https://github.com/matter-labs/zksync-era/issues/1804)) ([82bf40e](https://github.com/matter-labs/zksync-era/commit/82bf40e414e11b40c4ba4bad20f5d421a62d2e2f))
* **state-keeper:** Improve `FilterWrittenSlots` l1 batch seal stage ([#1854](https://github.com/matter-labs/zksync-era/issues/1854)) ([4cf235f](https://github.com/matter-labs/zksync-era/commit/4cf235f807932ac53fec1403005dcd7ffc0fb539))

## [24.0.0](https://github.com/matter-labs/zksync-era/compare/core-v23.1.0...core-v24.0.0) (2024-04-30)


### ⚠ BREAKING CHANGES

* **prover:** Protocol Upgrade 1.5.0 ([#1699](https://github.com/matter-labs/zksync-era/issues/1699))
* shared bridge ([#298](https://github.com/matter-labs/zksync-era/issues/298))

### Features

* **api:** Allow granular max response size config ([#1642](https://github.com/matter-labs/zksync-era/issues/1642)) ([83c4034](https://github.com/matter-labs/zksync-era/commit/83c40341c092dda71c02fb0cc8c498c5f70c0fe4))
* **api:** Improve logging for API server ([#1792](https://github.com/matter-labs/zksync-era/issues/1792)) ([50fbda5](https://github.com/matter-labs/zksync-era/commit/50fbda57aaba2be8b196e1f24ec4f66ea3cad962))
* **api:** Track params for RPC methods ([#1673](https://github.com/matter-labs/zksync-era/issues/1673)) ([1a34c8b](https://github.com/matter-labs/zksync-era/commit/1a34c8beb635ea70803a19407c90a1c3e3150398))
* **configs:** move ecosystem contracts to contracts ([#1606](https://github.com/matter-labs/zksync-era/issues/1606)) ([9d52180](https://github.com/matter-labs/zksync-era/commit/9d521807ccb94225ead751eb9267220805ed01c6))
* EcPairing precompile as system contract ([#1761](https://github.com/matter-labs/zksync-era/issues/1761)) ([3071622](https://github.com/matter-labs/zksync-era/commit/3071622f494bff39fe24eb251c656bba9dcea32c))
* Include create2 factory in genesis ([#1775](https://github.com/matter-labs/zksync-era/issues/1775)) ([597280b](https://github.com/matter-labs/zksync-era/commit/597280b8b698964a2a89b7f114f7f54de31bb133))
* **prover:** Protocol Upgrade 1.5.0 ([#1699](https://github.com/matter-labs/zksync-era/issues/1699)) ([6a557f7](https://github.com/matter-labs/zksync-era/commit/6a557f7766b727f72195d65e338cc41740cdbdbd))
* **prover:** remove redundant config fields ([#1787](https://github.com/matter-labs/zksync-era/issues/1787)) ([a784ea6](https://github.com/matter-labs/zksync-era/commit/a784ea64c847f31010af0ee71b1e64e9961dc5e1))
* shared bridge ([#298](https://github.com/matter-labs/zksync-era/issues/298)) ([8c3478a](https://github.com/matter-labs/zksync-era/commit/8c3478ae27c78a60c272f68c15d2bd59c99c8391))
* **tree:** Improved tree pruning ([#1532](https://github.com/matter-labs/zksync-era/issues/1532)) ([bcb192c](https://github.com/matter-labs/zksync-era/commit/bcb192c7b7bbb110f6320bc8d5bc53e12300e026))
* **vm-runner:** implement VM runner storage layer ([#1651](https://github.com/matter-labs/zksync-era/issues/1651)) ([543f9e9](https://github.com/matter-labs/zksync-era/commit/543f9e9397915e893d7b747ceccd9b76f9d571aa))
* **vm:** Extend bootloader memory in the new version ([#1807](https://github.com/matter-labs/zksync-era/issues/1807)) ([f461b28](https://github.com/matter-labs/zksync-era/commit/f461b28d19027a449c5d5c6678e6fcefd5e450a3))


### Bug Fixes

* **api:** Fix extra DB connection acquisition during tx submission ([#1793](https://github.com/matter-labs/zksync-era/issues/1793)) ([9c6ed83](https://github.com/matter-labs/zksync-era/commit/9c6ed838ffcfe6bd8fe157c69fbfe8823826849b))
* **en:** correct en config vars ([#1809](https://github.com/matter-labs/zksync-era/issues/1809)) ([d340fbc](https://github.com/matter-labs/zksync-era/commit/d340fbc5719d5f60a07866358cedabe1ab505ff6))
* **en:** Remove duplicate reorg detector ([#1783](https://github.com/matter-labs/zksync-era/issues/1783)) ([3417941](https://github.com/matter-labs/zksync-era/commit/34179412aa9bb11b8b2809d4028fbc200cf4d712))
* **kl-factory:** base token ether tests ([#1746](https://github.com/matter-labs/zksync-era/issues/1746)) ([6cf14a0](https://github.com/matter-labs/zksync-era/commit/6cf14a0e0d9192bcc868cc02ffd9069568da10be))
* **prover:** Fix panics if prover's config is not ready ([#1822](https://github.com/matter-labs/zksync-era/issues/1822)) ([21d90d7](https://github.com/matter-labs/zksync-era/commit/21d90d766798fb05be95028df185e9036ec7dee9))
* **snapshots_creator:** Remove snapshots during reverts ([#1757](https://github.com/matter-labs/zksync-era/issues/1757)) ([8d587fd](https://github.com/matter-labs/zksync-era/commit/8d587fd1f1d9a59281d59cf825e2cb9430774639))
* **types:** Add LegacyMixedCall ([#1773](https://github.com/matter-labs/zksync-era/issues/1773)) ([2b236fe](https://github.com/matter-labs/zksync-era/commit/2b236fe8c3b3a918d1bf2229c11295adbbbff9b8))
* v23 api typo ([#1821](https://github.com/matter-labs/zksync-era/issues/1821)) ([a11fa86](https://github.com/matter-labs/zksync-era/commit/a11fa8613446f82910d63c2ad89ea29538d228ea))
* Weaker assert for protocol version for operations ([#1800](https://github.com/matter-labs/zksync-era/issues/1800)) ([11898c2](https://github.com/matter-labs/zksync-era/commit/11898c2b58a0e04ce8bd8d4a587442b06d6c4ef3))

## [23.1.0](https://github.com/matter-labs/zksync-era/compare/core-v23.0.0...core-v23.1.0) (2024-04-22)


### Features

* **en:** Add boxed L2 client and use it in DI ([#1627](https://github.com/matter-labs/zksync-era/issues/1627)) ([9948187](https://github.com/matter-labs/zksync-era/commit/9948187aa12bcd99ef512ec8cf322fefe2cd745b))
* Extract block_reverter into separate crate ([#1632](https://github.com/matter-labs/zksync-era/issues/1632)) ([8ab2488](https://github.com/matter-labs/zksync-era/commit/8ab2488768f138c49fa90e8f975077cddd56c043))
* Extract house keeper into separate crate ([#1685](https://github.com/matter-labs/zksync-era/issues/1685)) ([f6f49b7](https://github.com/matter-labs/zksync-era/commit/f6f49b71f87265532111fd36e2af9b934fcffe64))
* remove enum index migration ([#1734](https://github.com/matter-labs/zksync-era/issues/1734)) ([13c0f52](https://github.com/matter-labs/zksync-era/commit/13c0f5202a9bf3c7d4532edef0e447ec3cad2062))
* **state-keeper:** miniblock max payload size (BFT-417) ([#1284](https://github.com/matter-labs/zksync-era/issues/1284)) ([a3c8e81](https://github.com/matter-labs/zksync-era/commit/a3c8e8103085289363b872e65054a6cbbdfa01f3))


### Bug Fixes

* **en:** Fix miscellaneous snapshot recovery nits ([#1701](https://github.com/matter-labs/zksync-era/issues/1701)) ([13bfecc](https://github.com/matter-labs/zksync-era/commit/13bfecc760c8b7217802b1d6e2c5da9afe61af39))
* ensure two connections for both executor and async catchup ([#1755](https://github.com/matter-labs/zksync-era/issues/1755)) ([3b14a9f](https://github.com/matter-labs/zksync-era/commit/3b14a9f259efb87e416f623cfa1263f7847c1e80))
* made consensus store certificates asynchronously from statekeeper ([#1711](https://github.com/matter-labs/zksync-era/issues/1711)) ([d1032ab](https://github.com/matter-labs/zksync-era/commit/d1032ab2b4328352d602783606dbffe6c0c9f635))
* **merkle_tree:** don't panic in `BlockOutputWithProofs::verify_proofs` ([#1717](https://github.com/matter-labs/zksync-era/issues/1717)) ([a44fac9](https://github.com/matter-labs/zksync-era/commit/a44fac97e0b121c93fde2d3fe15f494128cb3b16))
* **types:** fix LegacyCall type ([#1739](https://github.com/matter-labs/zksync-era/issues/1739)) ([712919f](https://github.com/matter-labs/zksync-era/commit/712919f40e0bf287b10f1cf2dfaee0df36a91d1b))


### Performance Improvements

* **en:** Monitor recovery latency by stage ([#1725](https://github.com/matter-labs/zksync-era/issues/1725)) ([d7efdd5](https://github.com/matter-labs/zksync-era/commit/d7efdd5c20bf273a451612f7d52431da94bf7080))


### Reverts

* **env:** Remove `ZKSYNC_HOME` env var from server ([#1713](https://github.com/matter-labs/zksync-era/issues/1713)) ([aed23e1](https://github.com/matter-labs/zksync-era/commit/aed23e1b2c100d6c8fc9259a1e573e790bfad36b))

## [23.0.0](https://github.com/matter-labs/zksync-era/compare/core-v22.1.0...core-v23.0.0) (2024-04-16)


### ⚠ BREAKING CHANGES

* **vm:** 1 5 0 support ([#1508](https://github.com/matter-labs/zksync-era/issues/1508))

### Features

* **api:** Add `tokens_whitelisted_for_paymaster` ([#1545](https://github.com/matter-labs/zksync-era/issues/1545)) ([6da89cd](https://github.com/matter-labs/zksync-era/commit/6da89cd5222435aa9994fb5989af75ecbe69b6fd))
* **api:** Log info about estimated fee ([#1611](https://github.com/matter-labs/zksync-era/issues/1611)) ([daed58c](https://github.com/matter-labs/zksync-era/commit/daed58ced42546b7ff1e38f12b44a861a2f41eee))
* Archive old prover jobs ([#1516](https://github.com/matter-labs/zksync-era/issues/1516)) ([201476c](https://github.com/matter-labs/zksync-era/commit/201476c8c1869c30605eb2acd462ae1dfe026fd1))
* Archiving of prover in gpu_prover_queue ([#1537](https://github.com/matter-labs/zksync-era/issues/1537)) ([a970629](https://github.com/matter-labs/zksync-era/commit/a9706294fe740cbc9af37eef8d968584a3ec4859))
* **block-reverter:** only require private key for sending revert transactions ([#1579](https://github.com/matter-labs/zksync-era/issues/1579)) ([27de6b7](https://github.com/matter-labs/zksync-era/commit/27de6b79d065ec5a25b5205017158256b9b62d00))
* **config:** Initialize log config from files as well ([#1566](https://github.com/matter-labs/zksync-era/issues/1566)) ([9e7db59](https://github.com/matter-labs/zksync-era/commit/9e7db5900c74019cf3368db99ed711e0f570852b))
* **configs:** Implement new format of configs and implement protobuf for it    ([#1501](https://github.com/matter-labs/zksync-era/issues/1501)) ([086ba5b](https://github.com/matter-labs/zksync-era/commit/086ba5b40565db7c23697830af2b9910b8bd0e34))
* **db:** Wrap sqlx errors in DAL ([#1522](https://github.com/matter-labs/zksync-era/issues/1522)) ([6e9ed8c](https://github.com/matter-labs/zksync-era/commit/6e9ed8c0499830ba71a22b5e112d94aa7e91d517))
* EN Pruning  ([#1418](https://github.com/matter-labs/zksync-era/issues/1418)) ([cea6578](https://github.com/matter-labs/zksync-era/commit/cea6578ffb037a2ad8476b6d3fb03416c1e55593))
* **en:** add consistency checker condition in db pruner ([#1653](https://github.com/matter-labs/zksync-era/issues/1653)) ([5ed92b9](https://github.com/matter-labs/zksync-era/commit/5ed92b9810cdb0dc0ceea594a8828cfbbf067006))
* **en:** add manual vacuum step in db pruning ([#1652](https://github.com/matter-labs/zksync-era/issues/1652)) ([c818be3](https://github.com/matter-labs/zksync-era/commit/c818be362aef9244bb644a2c50a620ac1ef077b4))
* **en:** Rate-limit L2 client requests ([#1500](https://github.com/matter-labs/zksync-era/issues/1500)) ([3f55f1e](https://github.com/matter-labs/zksync-era/commit/3f55f1e50e053c52cd9989581abf5848440920c1))
* **en:** Rework storing and using protective reads ([#1515](https://github.com/matter-labs/zksync-era/issues/1515)) ([13c0c45](https://github.com/matter-labs/zksync-era/commit/13c0c454b887f4dfad68cab9f2a5f421c1df5f8c))
* **en:** support for snapshots recovery in version_sync_task.rs ([#1585](https://github.com/matter-labs/zksync-era/issues/1585)) ([f911276](https://github.com/matter-labs/zksync-era/commit/f9112769c5885d44a6fb2d04fb2bf5abd4c0a2a2))
* **eth-watch:** Brush up Ethereum watcher component ([#1596](https://github.com/matter-labs/zksync-era/issues/1596)) ([b0b8f89](https://github.com/matter-labs/zksync-era/commit/b0b8f8932b03d98780d504f9acf394547dac7724))
* Expose component configs as info metrics ([#1584](https://github.com/matter-labs/zksync-era/issues/1584)) ([7c8ae40](https://github.com/matter-labs/zksync-era/commit/7c8ae40357a6ceeeb097c019588a8be18326bed1))
* **external-node:** external node distributed operation mode ([#1457](https://github.com/matter-labs/zksync-era/issues/1457)) ([777ffca](https://github.com/matter-labs/zksync-era/commit/777ffca152045c6a49298f714a44c8cfcde8a1d5))
* Extract commitment generator into a separate crate ([#1636](https://github.com/matter-labs/zksync-era/issues/1636)) ([f763d1f](https://github.com/matter-labs/zksync-era/commit/f763d1f193bac0dcdc367c8566b31d8384fe0651))
* Extract eth_watch and shared metrics into separate crates ([#1572](https://github.com/matter-labs/zksync-era/issues/1572)) ([4013771](https://github.com/matter-labs/zksync-era/commit/4013771e4a7a2a14828aa99153edb74a1861fa94))
* Finalize fee address migration ([#1617](https://github.com/matter-labs/zksync-era/issues/1617)) ([713f56b](https://github.com/matter-labs/zksync-era/commit/713f56b14433a39e8cc431be3150a9abe574984f))
* fix availability checker ([#1574](https://github.com/matter-labs/zksync-era/issues/1574)) ([b2f21fb](https://github.com/matter-labs/zksync-era/commit/b2f21fb1d72e65a738db9f7bc9f162a410d36c9b))
* **genesis:** Add genesis config generator  ([#1671](https://github.com/matter-labs/zksync-era/issues/1671)) ([45164fa](https://github.com/matter-labs/zksync-era/commit/45164fa6cb174c04c8542246cdb79c4f393339af))
* **genesis:** mark system contracts bytecodes as known ([#1554](https://github.com/matter-labs/zksync-era/issues/1554)) ([5ffec51](https://github.com/matter-labs/zksync-era/commit/5ffec511736bdc74542280b391b253febbe517f1))
* Migrate gas limit to u64 ([#1538](https://github.com/matter-labs/zksync-era/issues/1538)) ([56dc049](https://github.com/matter-labs/zksync-era/commit/56dc049993d673635e55b545962bf4f6d4f32739))
* **node-framework:** Add consensus support ([#1546](https://github.com/matter-labs/zksync-era/issues/1546)) ([27fe475](https://github.com/matter-labs/zksync-era/commit/27fe475dcd22e33efdeaf0f8d24f32a595538e3f))
* **node-framework:** Add consistency checker ([#1527](https://github.com/matter-labs/zksync-era/issues/1527)) ([3c28c25](https://github.com/matter-labs/zksync-era/commit/3c28c2540f420602cf63748b5e6de3002bfe90fb))
* remove unused variables in prover configs ([#1564](https://github.com/matter-labs/zksync-era/issues/1564)) ([d32a019](https://github.com/matter-labs/zksync-era/commit/d32a01918b2e42b8187fb6740b510b9b8798cafe))
* Remove zksync-rs SDK ([#1559](https://github.com/matter-labs/zksync-era/issues/1559)) ([cc78e1d](https://github.com/matter-labs/zksync-era/commit/cc78e1d98cc51e86b3c1b70dbe3dc38deaa9f3c2))
* soft removal of `events_queue` table  ([#1504](https://github.com/matter-labs/zksync-era/issues/1504)) ([5899bc6](https://github.com/matter-labs/zksync-era/commit/5899bc6d8f1b7ef03c2bb5d677e5d60cd6ed0fe5))
* **sqlx:** Use offline mode by default ([#1539](https://github.com/matter-labs/zksync-era/issues/1539)) ([af01edd](https://github.com/matter-labs/zksync-era/commit/af01edd6cedd96c0ce73b24e0d74452ec6c38d43))
* Use config for max number of circuits ([#1573](https://github.com/matter-labs/zksync-era/issues/1573)) ([9fcb87e](https://github.com/matter-labs/zksync-era/commit/9fcb87e9b126e2ad5465c6e2326d87cdc2f1a5cb))
* Validium ([#1461](https://github.com/matter-labs/zksync-era/issues/1461)) ([132a169](https://github.com/matter-labs/zksync-era/commit/132a1691de00eb3ca8fa7bd456a4591d84b24a5d))
* **vm:** 1 5 0 support ([#1508](https://github.com/matter-labs/zksync-era/issues/1508)) ([a6ccd25](https://github.com/matter-labs/zksync-era/commit/a6ccd2533b65a7464f097e2082b690bd426d7694))


### Bug Fixes

* **api:** Change error code for Web3Error::NotImplemented ([#1521](https://github.com/matter-labs/zksync-era/issues/1521)) ([0a13602](https://github.com/matter-labs/zksync-era/commit/0a13602e35fbfbb1abbdf7cefc33b2894754b199))
* **cache:** use factory deps cache correctly ([#1547](https://github.com/matter-labs/zksync-era/issues/1547)) ([a923e11](https://github.com/matter-labs/zksync-era/commit/a923e11ecfecc3de9b0b2cb578939a1f877a1e8a))
* **CI:** Less flaky CI ([#1536](https://github.com/matter-labs/zksync-era/issues/1536)) ([2444b53](https://github.com/matter-labs/zksync-era/commit/2444b5375fcd305bf2f0b7c2bb300316b99e37e2))
* **configs:** Make genesis fields optional ([#1555](https://github.com/matter-labs/zksync-era/issues/1555)) ([2d0ef46](https://github.com/matter-labs/zksync-era/commit/2d0ef46035142eafcb2974c323eab9fc04a4b6a7))
* contract verifier config test ([#1583](https://github.com/matter-labs/zksync-era/issues/1583)) ([030d447](https://github.com/matter-labs/zksync-era/commit/030d447cff0069cc5f218f767d9af7de5dba3a0f))
* **contract-verifier-api:** permissive cors for contract verifier api server ([#1525](https://github.com/matter-labs/zksync-era/issues/1525)) ([423f4a7](https://github.com/matter-labs/zksync-era/commit/423f4a7a906ad336a2853ebd6eb837bf7c0c0572))
* **db:** Fix "values cache update task failed" panics ([#1561](https://github.com/matter-labs/zksync-era/issues/1561)) ([f7c5c14](https://github.com/matter-labs/zksync-era/commit/f7c5c142e46de19abee540deb8a067934be17ca9))
* **en:** do not log error when whitelisted_tokens_for_aa is not supported ([#1600](https://github.com/matter-labs/zksync-era/issues/1600)) ([06c87f5](https://github.com/matter-labs/zksync-era/commit/06c87f5f58c325f6ca3652c729d85e56ab4e8ebe))
* **en:** Fix DB pool for Postgres metrics on EN ([#1675](https://github.com/matter-labs/zksync-era/issues/1675)) ([c51ca91](https://github.com/matter-labs/zksync-era/commit/c51ca91f05e65bffd52c190115bdb39180880f2b))
* **en:** improved tree recovery logs ([#1619](https://github.com/matter-labs/zksync-era/issues/1619)) ([ef12df7](https://github.com/matter-labs/zksync-era/commit/ef12df73a891579a87903055acae02a25da03ff6))
* **en:** Reduce amount of data in snapshot header ([#1528](https://github.com/matter-labs/zksync-era/issues/1528)) ([afa1cf1](https://github.com/matter-labs/zksync-era/commit/afa1cf1fd0359b27d6689b03bf76d3db2b3101b1))
* **eth-client:** Use local FeeHistory type ([#1552](https://github.com/matter-labs/zksync-era/issues/1552)) ([5a512e8](https://github.com/matter-labs/zksync-era/commit/5a512e8f853808332f9324d89be961a12b7fdbd0))
* instruction count diff always N/A in VM perf comparison ([#1608](https://github.com/matter-labs/zksync-era/issues/1608)) ([c0f3104](https://github.com/matter-labs/zksync-era/commit/c0f3104b63b32f681cb11233d3d41efd09b888a7))
* **vm:** Fix storage oracle and estimation ([#1634](https://github.com/matter-labs/zksync-era/issues/1634)) ([932b14b](https://github.com/matter-labs/zksync-era/commit/932b14b6ddee35375fbc302523da2d4d37f1d46b))
* **vm:** Increase log demuxer cycles on far calls ([#1575](https://github.com/matter-labs/zksync-era/issues/1575)) ([90eb9d8](https://github.com/matter-labs/zksync-era/commit/90eb9d8b2f3a50544e4020964dfea90b19a9894b))


### Performance Improvements

* **db:** rework "finalized" block SQL query ([#1524](https://github.com/matter-labs/zksync-era/issues/1524)) ([2b27290](https://github.com/matter-labs/zksync-era/commit/2b27290139cb3fa87412613e3b987d9d9c882275))
* **merkle tree:** Manage indices / filters in RocksDB ([#1550](https://github.com/matter-labs/zksync-era/issues/1550)) ([6bbfa06](https://github.com/matter-labs/zksync-era/commit/6bbfa064371089a31d6751de05682edda1dbfe2e))

## [22.1.0](https://github.com/matter-labs/zksync-era/compare/core-v22.0.0...core-v22.1.0) (2024-03-28)


### Features

* Drop prover tables in core database ([#1436](https://github.com/matter-labs/zksync-era/issues/1436)) ([0d78122](https://github.com/matter-labs/zksync-era/commit/0d78122833e8f92b63fad7c7a9974b9693c1d792))
* **en:** consistency checker persistent cursor ([#1466](https://github.com/matter-labs/zksync-era/issues/1466)) ([03496e6](https://github.com/matter-labs/zksync-era/commit/03496e66a63499518affcb81ccda26c997a2a729))
* **en:** Make snapshot syncing future-proof ([#1441](https://github.com/matter-labs/zksync-era/issues/1441)) ([8c26a7a](https://github.com/matter-labs/zksync-era/commit/8c26a7a47c8023236c194cadcb4e7e92b74b9622))
* **genesis:** Using genesis config only during the genesis ([#1423](https://github.com/matter-labs/zksync-era/issues/1423)) ([4b634fd](https://github.com/matter-labs/zksync-era/commit/4b634fd78ff2a8ee30135d4b774bc2cb0a4729ed))
* **node_framework:** Add a task to handle sigint ([#1471](https://github.com/matter-labs/zksync-era/issues/1471)) ([2ba6527](https://github.com/matter-labs/zksync-era/commit/2ba6527bbbda9343f0d5b09be028c69592ed890e))
* **node-framework:** Add circuit breaker checker layer to framework ([#1452](https://github.com/matter-labs/zksync-era/issues/1452)) ([2c7a6bf](https://github.com/matter-labs/zksync-era/commit/2c7a6bfcebb2cfa96c85e2f9c65369c4bf51e9f4))
* **prover:** export prover traces through OTLP ([#1427](https://github.com/matter-labs/zksync-era/issues/1427)) ([16dce75](https://github.com/matter-labs/zksync-era/commit/16dce7588ae6435bade23f48f6f8475312935445))
* sigint initialization only after snapshots is applied ([#1356](https://github.com/matter-labs/zksync-era/issues/1356)) ([c7c7356](https://github.com/matter-labs/zksync-era/commit/c7c7356483c931a10c9070b1aa70070a737af3f4))
* Split witness generator timeout configs by round ([#1505](https://github.com/matter-labs/zksync-era/issues/1505)) ([8074d01](https://github.com/matter-labs/zksync-era/commit/8074d01320b906f017702b046818d0b4f0faec65))
* **state-keeper:** implement asynchronous RocksDB cache ([#1256](https://github.com/matter-labs/zksync-era/issues/1256)) ([da41f63](https://github.com/matter-labs/zksync-era/commit/da41f634849372917fa7bf4cf00524b868f46fd4))
* **state-keeper:** Refactor persistence in `StateKeeper` ([#1411](https://github.com/matter-labs/zksync-era/issues/1411)) ([e26091a](https://github.com/matter-labs/zksync-era/commit/e26091a18ad7893f78511bf6ce47e8e85b54167e))
* **state-keeper:** Remove `WitnessBlockState` generation from state keeper ([#1507](https://github.com/matter-labs/zksync-era/issues/1507)) ([8ae0355](https://github.com/matter-labs/zksync-era/commit/8ae0355c2a6692b5b18642012de8ad5402d6c294))
* Switch contract verification API to axum and get rid of actix-web usage ([#1467](https://github.com/matter-labs/zksync-era/issues/1467)) ([e7a9d61](https://github.com/matter-labs/zksync-era/commit/e7a9d61bd2ee76f075291af0501cfba148af61d8))


### Bug Fixes

* **api:** `filters_disabled` should only affect HTTP endpoints ([#1493](https://github.com/matter-labs/zksync-era/issues/1493)) ([8720568](https://github.com/matter-labs/zksync-era/commit/8720568ce8eae416365b4b69fbb1f9612915dfcc))
* **api:** Fix API server shutdown flow ([#1425](https://github.com/matter-labs/zksync-era/issues/1425)) ([780f6b0](https://github.com/matter-labs/zksync-era/commit/780f6b041991c6f26ead50f8227dd6f8ff949208))
* **prover:** Remove FriProtocolVersionId ([#1510](https://github.com/matter-labs/zksync-era/issues/1510)) ([6aa51b0](https://github.com/matter-labs/zksync-era/commit/6aa51b0c04b9da5ba5d1c5a208ccf253188d45ef))
* **prover:** Remove redundant LoadingMode ([#1496](https://github.com/matter-labs/zksync-era/issues/1496)) ([e7583f4](https://github.com/matter-labs/zksync-era/commit/e7583f43872db45eaea1b8525add12530b4f128e))

## [22.0.0](https://github.com/matter-labs/zksync-era/compare/core-v21.1.0...core-v22.0.0) (2024-03-21)


### ⚠ BREAKING CHANGES

* Use protocol version v22 as latest ([#1432](https://github.com/matter-labs/zksync-era/issues/1432))

### Features

* add docs for Dal ([#1273](https://github.com/matter-labs/zksync-era/issues/1273)) ([66ceb0b](https://github.com/matter-labs/zksync-era/commit/66ceb0b79e10d043f40247b6f9f4af5c6e9e5123))
* adds debug_traceBlockByNumber.callFlatTracer ([#1413](https://github.com/matter-labs/zksync-era/issues/1413)) ([d2a5e36](https://github.com/matter-labs/zksync-era/commit/d2a5e361bcd9c3a0a4ec6920458935805cf96471))
* **api:** introduce mempool cache ([#1460](https://github.com/matter-labs/zksync-era/issues/1460)) ([c5d6c4b](https://github.com/matter-labs/zksync-era/commit/c5d6c4b96034da05fec15beb44dd54c3a0a249e7))
* **commitment-generator:** `events_queue` shadow mode ([#1138](https://github.com/matter-labs/zksync-era/issues/1138)) ([9bb47fa](https://github.com/matter-labs/zksync-era/commit/9bb47faba73eecfddf706d1a5382caa733c7ed37))
* **contract-verifier:** Allow sc code reverification ([#1455](https://github.com/matter-labs/zksync-era/issues/1455)) ([5a37b42](https://github.com/matter-labs/zksync-era/commit/5a37b422917012a90d8b15c75a9a113d47cab18b))
* **database:** add an optional master replica max connections settings ([#1472](https://github.com/matter-labs/zksync-era/issues/1472)) ([e5c8127](https://github.com/matter-labs/zksync-era/commit/e5c8127032f4f6346602428b6b0b89f9cca1e8d7))
* **db:** Configurable maximum number of open RocksDB files ([#1401](https://github.com/matter-labs/zksync-era/issues/1401)) ([b00c052](https://github.com/matter-labs/zksync-era/commit/b00c0527dcbbf93c6c09a9bdbd06064097a6016d))
* **en:** Check recipient contract and function selector in consistency checker ([#1367](https://github.com/matter-labs/zksync-era/issues/1367)) ([ea5c684](https://github.com/matter-labs/zksync-era/commit/ea5c6845e2a3d497f9cc2fdddd1b89682256697d))
* Follow-up for DAL split ([#1464](https://github.com/matter-labs/zksync-era/issues/1464)) ([c072288](https://github.com/matter-labs/zksync-era/commit/c072288a19511cb353e2895818178337b9a2d8ae))
* **node_framework:** Ergonomic improvements ([#1453](https://github.com/matter-labs/zksync-era/issues/1453)) ([09b6887](https://github.com/matter-labs/zksync-era/commit/09b6887eef795d11d7d72d2b839d2f4269191db7))
* **node_framework:** Only store each wiring layer once ([#1468](https://github.com/matter-labs/zksync-era/issues/1468)) ([4a393dc](https://github.com/matter-labs/zksync-era/commit/4a393dc844ef97d2b078539ff01d7f1a1396986e))
* **node_framework:** Support for preconditions and oneshot tasks ([#1398](https://github.com/matter-labs/zksync-era/issues/1398)) ([65ea881](https://github.com/matter-labs/zksync-era/commit/65ea881a348b6b982810e81a85f52810798d513b))
* **node-framework:** Add eth sender layer ([#1390](https://github.com/matter-labs/zksync-era/issues/1390)) ([0affdf8](https://github.com/matter-labs/zksync-era/commit/0affdf8a446c993e4effd635526ac534abfd963c))
* **node-framework:** Add housekeeper layer ([#1409](https://github.com/matter-labs/zksync-era/issues/1409)) ([702e739](https://github.com/matter-labs/zksync-era/commit/702e7395948e93dad81d829271110b22df92dac5))
* Remove batch dry_run ([#1076](https://github.com/matter-labs/zksync-era/issues/1076)) ([b82d093](https://github.com/matter-labs/zksync-era/commit/b82d093a167a06490918a069bfb715bfc50ee342))
* Separate Prover and Server DAL ([#1334](https://github.com/matter-labs/zksync-era/issues/1334)) ([103a56b](https://github.com/matter-labs/zksync-era/commit/103a56b2a66d58ffb9a16d4fb64cbfd90c2d5d7b))
* **storage:** Make the storage caches task cancellation aware ([#1430](https://github.com/matter-labs/zksync-era/issues/1430)) ([ab532bb](https://github.com/matter-labs/zksync-era/commit/ab532bbe6bc85f20685e5d3ede0c38d28715d958))
* support running consensus from snapshot (BFT-418) ([#1429](https://github.com/matter-labs/zksync-era/issues/1429)) ([f9f4d38](https://github.com/matter-labs/zksync-era/commit/f9f4d38258916fe8fc9a317bf96bfd9e3ad26e46))
* **tx-sender:** Limit concurrent tx submissions ([#1473](https://github.com/matter-labs/zksync-era/issues/1473)) ([4bdf3ca](https://github.com/matter-labs/zksync-era/commit/4bdf3ca443ffa78e21101c0e5351e8a1941ee7ba))
* Use protocol version v22 as latest ([#1432](https://github.com/matter-labs/zksync-era/issues/1432)) ([1757412](https://github.com/matter-labs/zksync-era/commit/175741211ec21e71936ac1b6a23e70a18924240c))
* **vm:** Prestate tracer implementation ([#1306](https://github.com/matter-labs/zksync-era/issues/1306)) ([c36be65](https://github.com/matter-labs/zksync-era/commit/c36be654cd37ce23a85d28c4fe5f93e7c05d583e))


### Bug Fixes

* **core:** drop correct index of proof_generation_details table during database migration ([#1199](https://github.com/matter-labs/zksync-era/issues/1199)) ([76a6821](https://github.com/matter-labs/zksync-era/commit/76a68217ad0823c2afe5dd8be60a6b1d71db5dbc))
* **dal:** correct `tx_index_in_l1_batch` in `get_vm_events_for_l1_batch` ([#1463](https://github.com/matter-labs/zksync-era/issues/1463)) ([8bf37ac](https://github.com/matter-labs/zksync-era/commit/8bf37acc2e78a96c9de37846293f2adafd6e8682))
* **node_framework:** Fix main node example ([#1470](https://github.com/matter-labs/zksync-era/issues/1470)) ([ac4a744](https://github.com/matter-labs/zksync-era/commit/ac4a744756ddb4825c412de96ac1fb90bf64532d))
* **protocol:** Remove verifier address from protocol upgrade ([#1443](https://github.com/matter-labs/zksync-era/issues/1443)) ([90dee73](https://github.com/matter-labs/zksync-era/commit/90dee732295d8eaf48a5308c39238a0f5838cc35))
* **reth:** use reth instead of geth ([#1410](https://github.com/matter-labs/zksync-era/issues/1410)) ([bd98dc7](https://github.com/matter-labs/zksync-era/commit/bd98dc790ef0132fd1213b77e1b6269c55cf7a26))
* **verified sources fetcher:** Use correct `contact_name` for `SolSingleFile` ([#1433](https://github.com/matter-labs/zksync-era/issues/1433)) ([0764227](https://github.com/matter-labs/zksync-era/commit/0764227aa22165cba5e3d41756383cfe940e97b4))

## [21.1.0](https://github.com/matter-labs/zksync-era/compare/core-v21.0.0...core-v21.1.0) (2024-03-13)


### Features

* **api:** Monitor server RPC errors ([#1203](https://github.com/matter-labs/zksync-era/issues/1203)) ([60d1060](https://github.com/matter-labs/zksync-era/commit/60d106096504363f05c415161909c67b3f5d20c7))
* block revert support for consensus component ([#1213](https://github.com/matter-labs/zksync-era/issues/1213)) ([8a3a938](https://github.com/matter-labs/zksync-era/commit/8a3a938c1ce871e77a478b4cd06b23714a6d7c64))
* **db:** Add Postgres table size metrics ([#1351](https://github.com/matter-labs/zksync-era/issues/1351)) ([63f3ff8](https://github.com/matter-labs/zksync-era/commit/63f3ff8fc3dcc895cafa50eb44067a570151ef6d))
* **db:** Serialize events queue as bytes ([#1420](https://github.com/matter-labs/zksync-era/issues/1420)) ([955680b](https://github.com/matter-labs/zksync-era/commit/955680b1aad9c6b66f13027a41819e5e3edf407a))
* enabled loading yaml config for the main node ([#1344](https://github.com/matter-labs/zksync-era/issues/1344)) ([0adab9e](https://github.com/matter-labs/zksync-era/commit/0adab9ea84412c62d46893cc5b66368b2294d760))
* **en:** Enable Merkle tree client on EN ([#1386](https://github.com/matter-labs/zksync-era/issues/1386)) ([58576d1](https://github.com/matter-labs/zksync-era/commit/58576d106179177f808018514a1606941bf24b30))
* enhance unit test for batch tip ([#1253](https://github.com/matter-labs/zksync-era/issues/1253)) ([ca7d194](https://github.com/matter-labs/zksync-era/commit/ca7d19429cf5f0be8451c930423cb9733e55e7b1))
* Moving 1.4.x to use the circuit_api ([#1383](https://github.com/matter-labs/zksync-era/issues/1383)) ([8add2d6](https://github.com/matter-labs/zksync-era/commit/8add2d6d8d5837359f1d309c959d05f23e5393fe))
* **node_framework:** Add timeouts for remaining tasks to avoid hang outs ([#1354](https://github.com/matter-labs/zksync-era/issues/1354)) ([8108dbd](https://github.com/matter-labs/zksync-era/commit/8108dbdf8c2e05828121e45a664413800a6668a5))
* **node-framework:** Add commitment generator layer ([#1402](https://github.com/matter-labs/zksync-era/issues/1402)) ([daa029c](https://github.com/matter-labs/zksync-era/commit/daa029c1e094bcb6d75c7fe8914de5f00db8fcc1))
* replacing 1.3.3 test harness with circuit sequencer api ([#1382](https://github.com/matter-labs/zksync-era/issues/1382)) ([a628d56](https://github.com/matter-labs/zksync-era/commit/a628d56f449889ae5c5a0e48294e0be3e560728b))


### Bug Fixes

* **aggregator:** correct order of processing of prove transactions ([#1333](https://github.com/matter-labs/zksync-era/issues/1333)) ([7522d15](https://github.com/matter-labs/zksync-era/commit/7522d15be20861ce77e3c104fc1ed852b7f066be))
* **api:** Fix panics in API server if storage values cache is disabled ([#1370](https://github.com/matter-labs/zksync-era/issues/1370)) ([723232b](https://github.com/matter-labs/zksync-era/commit/723232b98b064bb536a34a722155dfc438f03243))
* **api:** SQL: use = instead of ANY where possible in events-related queries ([#1346](https://github.com/matter-labs/zksync-era/issues/1346)) ([160b4d4](https://github.com/matter-labs/zksync-era/commit/160b4d4a59851c90ae9f439ac3e960d073a0ea18))
* **consistency_checker:** Fix consistency checker for large pubdata ([#1331](https://github.com/matter-labs/zksync-era/issues/1331)) ([d162add](https://github.com/matter-labs/zksync-era/commit/d162addc74cd4f7a8223de88d8943e2732c60eb1))
* **en:** Fix pending transactions subscription ([#1342](https://github.com/matter-labs/zksync-era/issues/1342)) ([a040001](https://github.com/matter-labs/zksync-era/commit/a040001f8007d4f85e17f3dd1e59eb637f962df1))
* **eth-sender:** adjust the blob tx fees taking into account the current prices ([#1399](https://github.com/matter-labs/zksync-era/issues/1399)) ([fea67fb](https://github.com/matter-labs/zksync-era/commit/fea67fb129601c647908fd758feead4ac8c4255e))
* **gas-adjuster:** Do not unwrap in gas-adjuster ([#1400](https://github.com/matter-labs/zksync-era/issues/1400)) ([269812e](https://github.com/matter-labs/zksync-era/commit/269812ed71f88ba0c33280d93b8b978f35a3e5c8))
* **gas-adjuster:** Use `internal_pubdata_pricing_multiplier` for pubdata price calculation ([#1404](https://github.com/matter-labs/zksync-era/issues/1404)) ([a40c2d0](https://github.com/matter-labs/zksync-era/commit/a40c2d0b1874f0109d907b81025750acb21e04ba))


### Performance Improvements

* **db:** Add "contains" clause for get_logs ([#1384](https://github.com/matter-labs/zksync-era/issues/1384)) ([e62ae32](https://github.com/matter-labs/zksync-era/commit/e62ae326affa0b7665e783d351e560dfe6566251))
* **db:** Remove obsolete indexes ([#1376](https://github.com/matter-labs/zksync-era/issues/1376)) ([0afc377](https://github.com/matter-labs/zksync-era/commit/0afc377a058b55e4c4ecada88001718acc4b4d51))


### Reverts

* **db:** Remove "contains" clause for get_logs ([#1384](https://github.com/matter-labs/zksync-era/issues/1384)) ([#1407](https://github.com/matter-labs/zksync-era/issues/1407)) ([1da53f3](https://github.com/matter-labs/zksync-era/commit/1da53f3993ff3658a451ed67e04d838851e534cf))

## [21.0.0](https://github.com/matter-labs/zksync-era/compare/core-v20.8.0...core-v21.0.0) (2024-03-01)


### ⚠ BREAKING CHANGES

* **prover:** Add EIP4844 support for provers subsystem ([#1200](https://github.com/matter-labs/zksync-era/issues/1200))
* Set 21 as latest protocol version ([#1262](https://github.com/matter-labs/zksync-era/issues/1262))

### Features

* A way to disable slow query logging for some queries ([#1297](https://github.com/matter-labs/zksync-era/issues/1297)) ([13b82a0](https://github.com/matter-labs/zksync-era/commit/13b82a0db88d64b579e79cb824c71af8d14a9e64))
* **aggregator:** support two operator addresses in sender and aggregator ([#1201](https://github.com/matter-labs/zksync-era/issues/1201)) ([7be56e5](https://github.com/matter-labs/zksync-era/commit/7be56e576fbb4d404319a6c099dcf1d5cb0b476a))
* change EN main node fetcher concurrency factor ([#1317](https://github.com/matter-labs/zksync-era/issues/1317)) ([d4235b5](https://github.com/matter-labs/zksync-era/commit/d4235b541034b300a7043e83bb9fc44ab196dad5))
* **commitment-generator:** Commitment for 1.4.2 ([#1234](https://github.com/matter-labs/zksync-era/issues/1234)) ([9b773eb](https://github.com/matter-labs/zksync-era/commit/9b773ebb3c170dc3d503ec83fb33d8d463614b4f))
* **config:** add pubdata sending method to config ([#1261](https://github.com/matter-labs/zksync-era/issues/1261)) ([cebf55a](https://github.com/matter-labs/zksync-era/commit/cebf55acde566064d365840933515d79535af18c))
* **config:** Added blobs to commit batches and made da source part of config ([#1243](https://github.com/matter-labs/zksync-era/issues/1243)) ([e01d1b6](https://github.com/matter-labs/zksync-era/commit/e01d1b64a20eabf7c43f310978730ad6f3223fb2))
* **config:** update eth_sender to use blobs ([#1295](https://github.com/matter-labs/zksync-era/issues/1295)) ([e81f080](https://github.com/matter-labs/zksync-era/commit/e81f0800ad072e4a1433e1fdce6866bb7568f7da))
* **core:** Adds support for 4844 transaction signing and rlp encoding ([#1254](https://github.com/matter-labs/zksync-era/issues/1254)) ([10e3a3e](https://github.com/matter-labs/zksync-era/commit/10e3a3ec50c84b3533af111d532e0b528c0b4966))
* **dal:** `zksync_types::Transaction` to use protobuf for wire encoding (BFT-407) ([#1047](https://github.com/matter-labs/zksync-era/issues/1047)) ([ee94bee](https://github.com/matter-labs/zksync-era/commit/ee94beed9e610adee94ea5073f0c748df7447135))
* **eth_sender:** set blob gas fee when blobs DA is used ([#1285](https://github.com/matter-labs/zksync-era/issues/1285)) ([57c5526](https://github.com/matter-labs/zksync-era/commit/57c5526602b89ce0fff9bbf06d840f7858b2ce99))
* **gas-adjuster:** gas adjuster for EIP4844 ([#1255](https://github.com/matter-labs/zksync-era/issues/1255)) ([1da97ed](https://github.com/matter-labs/zksync-era/commit/1da97edea5bc323fc40d76c96d6ac48be77f0f7b))
* Metrics for block tip ([#1144](https://github.com/matter-labs/zksync-era/issues/1144)) ([85d4b12](https://github.com/matter-labs/zksync-era/commit/85d4b124f59ee6596aaa8e6db3bbbb86fa829c90))
* **node_framework:** Add Web3 API layers ([#1258](https://github.com/matter-labs/zksync-era/issues/1258)) ([105f4cc](https://github.com/matter-labs/zksync-era/commit/105f4cc2aebce958c3e9d2bfe4a4277fc26ebb4b))
* **node_framework:** Support Proof Data Handler in the framework ([#1233](https://github.com/matter-labs/zksync-era/issues/1233)) ([2191218](https://github.com/matter-labs/zksync-era/commit/21912182b72f17c40d259f2bc0de9ce8e8241420))
* **prover:** Add EIP4844 support for provers subsystem ([#1200](https://github.com/matter-labs/zksync-era/issues/1200)) ([6953e89](https://github.com/matter-labs/zksync-era/commit/6953e8989571d65230c66f3731c8831fd7abb274))
* Remove methods to sign an arbitraty message ([#1294](https://github.com/matter-labs/zksync-era/issues/1294)) ([8904123](https://github.com/matter-labs/zksync-era/commit/89041236282e4b85de5eec3a4d9471f0fd3db58d))
* Set 21 as latest protocol version ([#1262](https://github.com/matter-labs/zksync-era/issues/1262)) ([30579ef](https://github.com/matter-labs/zksync-era/commit/30579ef6cf5e8f96572d9b215fceec79556ce0ea))
* support loading verifier and protocol version from db ([#1293](https://github.com/matter-labs/zksync-era/issues/1293)) ([533f013](https://github.com/matter-labs/zksync-era/commit/533f0131342dc40eea1bb82334828d0007043d19))


### Bug Fixes

* Add EIP4844 to fri_prover_group_config ([#1309](https://github.com/matter-labs/zksync-era/issues/1309)) ([edf9397](https://github.com/matter-labs/zksync-era/commit/edf9397ea65ddcb41b68f9732e496f38cadf3e58))
* **api:** Use better gas per pubdata in API (pure server changes) ([#1311](https://github.com/matter-labs/zksync-era/issues/1311)) ([54f8d8c](https://github.com/matter-labs/zksync-era/commit/54f8d8caa702e0676b31cfc048d8d331fa961547))
* **docker:** change default password for postgres ([#1250](https://github.com/matter-labs/zksync-era/issues/1250)) ([f6bff74](https://github.com/matter-labs/zksync-era/commit/f6bff74845983518026ec1d91a47523a3ec0f48c))
* **en:** fail fast if we don't request correct number of txs from man node ([#1269](https://github.com/matter-labs/zksync-era/issues/1269)) ([1bcbf17](https://github.com/matter-labs/zksync-era/commit/1bcbf1751c6224619873373c7cf071cdebdf56ee))
* Fix scheduler enqueuer bug ([#1322](https://github.com/matter-labs/zksync-era/issues/1322)) ([95deb92](https://github.com/matter-labs/zksync-era/commit/95deb92a2da59417470084894588a97c98bc65b8))
* **snapshots_creator:** Fix snapshot generation query ([#1289](https://github.com/matter-labs/zksync-era/issues/1289)) ([e279456](https://github.com/matter-labs/zksync-era/commit/e2794564ce61e52b0f31079e9a0f67c27037002c))
* **vm:** fix match clause in `get_max_gas_per_pubdata_byte` ([#1292](https://github.com/matter-labs/zksync-era/issues/1292)) ([eaf5a50](https://github.com/matter-labs/zksync-era/commit/eaf5a506266b5292df9b277b6c020170ca1f59db))

## [20.8.0](https://github.com/matter-labs/zksync-era/compare/core-v20.7.0...core-v20.8.0) (2024-02-26)


### Features

* Add more buckets to call tracer ([#1137](https://github.com/matter-labs/zksync-era/issues/1137)) ([dacd8c9](https://github.com/matter-labs/zksync-era/commit/dacd8c99ee08175b885489ca12322d275e4c3d11))
* **api:** add a config flag for disabling filter api ([#1078](https://github.com/matter-labs/zksync-era/issues/1078)) ([b486d7e](https://github.com/matter-labs/zksync-era/commit/b486d7efead7afccedff9e21c98c30fb70eca66b))
* **api:** Create RPC method to return all tokens ([#1103](https://github.com/matter-labs/zksync-era/issues/1103)) ([b538d1a](https://github.com/matter-labs/zksync-era/commit/b538d1a411f58a4106afea8f9a71a3ea2ad9352d))
* **api:** Implement TxSink abstraction ([#1204](https://github.com/matter-labs/zksync-era/issues/1204)) ([11a34d4](https://github.com/matter-labs/zksync-era/commit/11a34d486258e2535ab5e11e38ba7e5e2885dc88))
* **en:** Add health checks for EN components ([#1088](https://github.com/matter-labs/zksync-era/issues/1088)) ([4ea1520](https://github.com/matter-labs/zksync-era/commit/4ea15202f9d703bbefe9e383138321b33d0844ee))
* **en:** Start health checks early into EN lifecycle ([#1146](https://github.com/matter-labs/zksync-era/issues/1146)) ([f983e80](https://github.com/matter-labs/zksync-era/commit/f983e803c7c0f5c41f8beaabc820676be84d7825))
* **en:** switch to tree light mode ([#1152](https://github.com/matter-labs/zksync-era/issues/1152)) ([ce6c120](https://github.com/matter-labs/zksync-era/commit/ce6c120c31b270739eecb102f6742237e6c9d004))
* **en:** Take into account nonce from tx proxy ([#995](https://github.com/matter-labs/zksync-era/issues/995)) ([22099cb](https://github.com/matter-labs/zksync-era/commit/22099cb2f90d917a2479b82c8a4165e68c8da878))
* **healthcheck:** Various healthcheck improvements ([#1166](https://github.com/matter-labs/zksync-era/issues/1166)) ([1e34148](https://github.com/matter-labs/zksync-era/commit/1e3414831ffebbad67a552dcaededfa93d6762df))
* Integration tests enhancement for L1 ([#1209](https://github.com/matter-labs/zksync-era/issues/1209)) ([a1c866c](https://github.com/matter-labs/zksync-era/commit/a1c866c9fc9c6e6ceed9a1cc0b39e0fade29ecfa))
* **node_framework:** Support Eth Watch in the framework ([#1145](https://github.com/matter-labs/zksync-era/issues/1145)) ([4f41b68](https://github.com/matter-labs/zksync-era/commit/4f41b68b038526423aeed98af96cd7b801273172))
* **shared bridge:** preparation for shared bridge migration (server) ([#1012](https://github.com/matter-labs/zksync-era/issues/1012)) ([2a766a7](https://github.com/matter-labs/zksync-era/commit/2a766a75db8f14a6eadf0fdecac415865724cb5f))
* **vlog:** Remove env getters from vlog ([#1077](https://github.com/matter-labs/zksync-era/issues/1077)) ([00d3429](https://github.com/matter-labs/zksync-era/commit/00d342932c6305055f719afaecf6a0eeeb3ca63a))
* **vm:** Add new VM folder ([#1208](https://github.com/matter-labs/zksync-era/issues/1208)) ([66cdefc](https://github.com/matter-labs/zksync-era/commit/66cdefcfd86d5ddd2718f04491f577d6a24161f6))
* **vm:** integrate new vm version ([#1215](https://github.com/matter-labs/zksync-era/issues/1215)) ([63d1f52](https://github.com/matter-labs/zksync-era/commit/63d1f529ca8f2125e6effa8bc3baef45bc7ef602))


### Bug Fixes

* **contract-verifier:** Add force_evmla flag ([#1179](https://github.com/matter-labs/zksync-era/issues/1179)) ([e75aa11](https://github.com/matter-labs/zksync-era/commit/e75aa11f2d3e6c9eb50543f896e3dfca8cca90d2))
* **contract-verifier:** allow other zksolc settings ([#1174](https://github.com/matter-labs/zksync-era/issues/1174)) ([72c60bd](https://github.com/matter-labs/zksync-era/commit/72c60bdaf242576a1a3e9b068e489f2c601d2d96))
* **state-keeper:** Add GasForBatchTip criterion ([#1096](https://github.com/matter-labs/zksync-era/issues/1096)) ([de4d729](https://github.com/matter-labs/zksync-era/commit/de4d729dbd1f2f1bf524be75a0f8ee9d9cd372f8))


### Performance Improvements

* **db:** Improve `get_logs_by_tx_hashes` query ([#1171](https://github.com/matter-labs/zksync-era/issues/1171)) ([0dda7cc](https://github.com/matter-labs/zksync-era/commit/0dda7cc6f9559503f3801e6d0b9d5ba9649815bd))

## [20.7.0](https://github.com/matter-labs/zksync-era/compare/core-v20.6.0...core-v20.7.0) (2024-02-16)


### Features

* Add input field to CallRequest ([#1069](https://github.com/matter-labs/zksync-era/issues/1069)) ([5087121](https://github.com/matter-labs/zksync-era/commit/5087121db54d49bc5268e3b77790d3315976b69a))
* **api:** Remove unused and obsolete token info ([#1071](https://github.com/matter-labs/zksync-era/issues/1071)) ([e920897](https://github.com/matter-labs/zksync-era/commit/e920897d41583b822eefc72fb5899cfcbeefd621))
* Better errors for JsonRPC calls ([#1002](https://github.com/matter-labs/zksync-era/issues/1002)) ([079f999](https://github.com/matter-labs/zksync-era/commit/079f99941767173f6a3d9824410ca35c3fbf07a5))
* **commitment:** Commitment component ([#1024](https://github.com/matter-labs/zksync-era/issues/1024)) ([60305ba](https://github.com/matter-labs/zksync-era/commit/60305ba2fdb211c4053ab40e0e6c55d4e6f337b0))
* **en:** Make snapshots applier resilient and process storage log chunks in parallel ([#1036](https://github.com/matter-labs/zksync-era/issues/1036)) ([805218c](https://github.com/matter-labs/zksync-era/commit/805218ccca18b2f3ab80881097e5cd98ee926204))
* **node_framework:** Resources and layers for ETH clients ([#1074](https://github.com/matter-labs/zksync-era/issues/1074)) ([776337a](https://github.com/matter-labs/zksync-era/commit/776337a8939831b94a5c86a6eb9b3cdc22a8b689))
* **node_framework:** Support StateKeeper in the framework ([#1043](https://github.com/matter-labs/zksync-era/issues/1043)) ([a80fff2](https://github.com/matter-labs/zksync-era/commit/a80fff2c5be6dba109e844e6c1774f76417def7d))


### Bug Fixes

* **api:** Return on duplicate earlier ([#1059](https://github.com/matter-labs/zksync-era/issues/1059)) ([cfa5701](https://github.com/matter-labs/zksync-era/commit/cfa57019cc0cb3ae784f37d4b1295de20eb1b2c3))
* **contract-verifier:** Use optimizer mode in solidity-single-file verification ([#1079](https://github.com/matter-labs/zksync-era/issues/1079)) ([fdab638](https://github.com/matter-labs/zksync-era/commit/fdab63869c04bb8150963c3d400c9372d35c2ca8))
* Token distribution ([#1051](https://github.com/matter-labs/zksync-era/issues/1051)) ([bd63b3a](https://github.com/matter-labs/zksync-era/commit/bd63b3aa90931b19e31cda17f70db04a1e7e068b))

## [20.6.0](https://github.com/matter-labs/zksync-era/compare/core-v20.5.2...core-v20.6.0) (2024-02-08)


### Features

* **api:** Start API server after first L1 batch ([#1026](https://github.com/matter-labs/zksync-era/issues/1026)) ([86e189c](https://github.com/matter-labs/zksync-era/commit/86e189ca2f4be2a3c431694a8938af073b81b298))
* **db:** Instrument DB connection lifecycle ([#1027](https://github.com/matter-labs/zksync-era/issues/1027)) ([636fcfd](https://github.com/matter-labs/zksync-era/commit/636fcfd308cdde92c6407b886d7b329938623e9d))
* **db:** Soft-remove `storage` table ([#982](https://github.com/matter-labs/zksync-era/issues/982)) ([601f893](https://github.com/matter-labs/zksync-era/commit/601f893b98613222422961b95473560445e34637))
* **en:** Make state keeper work with pruned data ([#900](https://github.com/matter-labs/zksync-era/issues/900)) ([f1913ae](https://github.com/matter-labs/zksync-era/commit/f1913ae824b124525bec7896c9271e1a4bdefa41))
* export fee model for the test node ([#1030](https://github.com/matter-labs/zksync-era/issues/1030)) ([d1e4774](https://github.com/matter-labs/zksync-era/commit/d1e47744c773fa38aa22aaaa3dbb9dbffe7e9854))
* Time-limit health checks and log them ([#993](https://github.com/matter-labs/zksync-era/issues/993)) ([f3c190d](https://github.com/matter-labs/zksync-era/commit/f3c190da34ae340f05ed2c42e15694f963ab2508))
* **types:** Added KZG info needed for 4844 blobs ([#894](https://github.com/matter-labs/zksync-era/issues/894)) ([758f487](https://github.com/matter-labs/zksync-era/commit/758f4877433ebdd86cc90dd592db9a7d87598586))


### Bug Fixes

* fix link ([#1007](https://github.com/matter-labs/zksync-era/issues/1007)) ([f1424ce](https://github.com/matter-labs/zksync-era/commit/f1424ced16b5609e736d9075ef1339b955154260))
* **metrics:** Use latest block for non pos ethereum as safe ([#1022](https://github.com/matter-labs/zksync-era/issues/1022)) ([49ec843](https://github.com/matter-labs/zksync-era/commit/49ec843a211ab02bfcc2b40d98794fa35f3d4c83))
* Revert "preparation for shared bridge migration (server)" ([#1010](https://github.com/matter-labs/zksync-era/issues/1010)) ([d4c984a](https://github.com/matter-labs/zksync-era/commit/d4c984a3e4b454bd8811053a70e108e67ef33506))

## [20.5.2](https://github.com/matter-labs/zksync-era/compare/core-v20.5.1...core-v20.5.2) (2024-02-04)


### Bug Fixes

* **db:** miniblocks index ([#1004](https://github.com/matter-labs/zksync-era/issues/1004)) ([b3fd22e](https://github.com/matter-labs/zksync-era/commit/b3fd22e3a730499f37102a1bc00ad1004f192668))

## [20.5.1](https://github.com/matter-labs/zksync-era/compare/core-v20.5.0...core-v20.5.1) (2024-02-02)


### Bug Fixes

* **db:** transaction index ([#998](https://github.com/matter-labs/zksync-era/issues/998)) ([2b03736](https://github.com/matter-labs/zksync-era/commit/2b037365543aa39a28601e63b30f9963e7d3e044))

## [20.5.0](https://github.com/matter-labs/zksync-era/compare/core-v20.4.0...core-v20.5.0) (2024-02-02)


### Features

* **merkle-tree:** Do not wait for tree initialization when starting node ([#992](https://github.com/matter-labs/zksync-era/issues/992)) ([fdbfcb1](https://github.com/matter-labs/zksync-era/commit/fdbfcb1622ee1eccd380e1930ec5401c52b73567))


### Bug Fixes

* added consensus column back ([#986](https://github.com/matter-labs/zksync-era/issues/986)) ([b9b48d4](https://github.com/matter-labs/zksync-era/commit/b9b48d45fa5d854b21c3a3b9ff57665a788a53c5))
* get_block_receipts test ([#989](https://github.com/matter-labs/zksync-era/issues/989)) ([c301359](https://github.com/matter-labs/zksync-era/commit/c30135902afa3c39d1ac0ce2ff3b70f5c1746373))
* **vm:** Save empty bootloader memory for batches with ancient vms ([#991](https://github.com/matter-labs/zksync-era/issues/991)) ([af7f64f](https://github.com/matter-labs/zksync-era/commit/af7f64f37faa5300ae258874d74cdcfe009dfaae))

## [20.4.0](https://github.com/matter-labs/zksync-era/compare/core-v20.3.0...core-v20.4.0) (2024-01-31)


### Features

* **en:** Revert "feat(en): Fix operator address assignment for ENs" ([#977](https://github.com/matter-labs/zksync-era/issues/977)) ([e051f7a](https://github.com/matter-labs/zksync-era/commit/e051f7a80bd1c1c5b76a6e74288fab9f820738b2))

## [20.3.0](https://github.com/matter-labs/zksync-era/compare/core-v20.2.0...core-v20.3.0) (2024-01-31)


### Features

* add eth_getBlockReceipts ([#887](https://github.com/matter-labs/zksync-era/issues/887)) ([5dcbcfd](https://github.com/matter-labs/zksync-era/commit/5dcbcfdeb683b02d17b77031b0a2200fa69ac778))
* **eth-sender:** metrics for finalized and safe L1 block numbers ([#972](https://github.com/matter-labs/zksync-era/issues/972)) ([32c1637](https://github.com/matter-labs/zksync-era/commit/32c163754d5e21b9996267728fe3f527ed8ec4da))
* Optimized block tip seal criterion ([#968](https://github.com/matter-labs/zksync-era/issues/968)) ([8049eb3](https://github.com/matter-labs/zksync-era/commit/8049eb340eadcb2e9844465d8ea15ae8c08e0ef5))
* Prover interface and L1 interface crates ([#959](https://github.com/matter-labs/zksync-era/issues/959)) ([4f7e107](https://github.com/matter-labs/zksync-era/commit/4f7e10783afdff67a24246f17f03b536f743352d))

## [20.2.0](https://github.com/matter-labs/zksync-era/compare/core-v20.1.0...core-v20.2.0) (2024-01-30)


### Features

* added unauthenticated version of gcs object store ([#916](https://github.com/matter-labs/zksync-era/issues/916)) ([638a813](https://github.com/matter-labs/zksync-era/commit/638a813e1115c36d3d7fbed28f24658769b2b93e))
* Adding EN snapshots applier ([#882](https://github.com/matter-labs/zksync-era/issues/882)) ([0d2ba09](https://github.com/matter-labs/zksync-era/commit/0d2ba09c5d4b607bd9da31fc4bf0ea8ca2b4d7b8))
* consensus component config for main node and external node ([#881](https://github.com/matter-labs/zksync-era/issues/881)) ([1aed8de](https://github.com/matter-labs/zksync-era/commit/1aed8de0f1651686bf9e9f8aa7dc9ba15625cc42))
* **en:** Make ENs detect reorgs earlier ([#964](https://github.com/matter-labs/zksync-era/issues/964)) ([b043cc8](https://github.com/matter-labs/zksync-era/commit/b043cc84cd9f5e9c6e80b810a019c713fb3076d3))
* **en:** Restore state keeper storage from snapshot ([#885](https://github.com/matter-labs/zksync-era/issues/885)) ([a9553b5](https://github.com/matter-labs/zksync-era/commit/a9553b537a857a6f6a755cd700da4c096c1f80f0))
* protobuf-generated json configs for the main node (BFT-371) ([#458](https://github.com/matter-labs/zksync-era/issues/458)) ([f938314](https://github.com/matter-labs/zksync-era/commit/f9383143b4f1f0c18af658980bae8ec93b6b588f))
* Remove zkevm_test_harness public reexport from zksync_types ([#929](https://github.com/matter-labs/zksync-era/issues/929)) ([dd1a35e](https://github.com/matter-labs/zksync-era/commit/dd1a35eec006b40db66da73e6fa3d8963efb7d60))
* **state-keeper:** track the time that individual transactions spend in mempool ([#941](https://github.com/matter-labs/zksync-era/issues/941)) ([fa45aa9](https://github.com/matter-labs/zksync-era/commit/fa45aa9bd87f284872c9831620b36f2f2339f75b))
* **vm:** detailed circuit statistic ([#845](https://github.com/matter-labs/zksync-era/issues/845)) ([a20af60](https://github.com/matter-labs/zksync-era/commit/a20af609d6eda25e5530c30b360847f6eadb68d9))
* **vm:** Support tracers for old vm  ([#926](https://github.com/matter-labs/zksync-era/issues/926)) ([9fc2d95](https://github.com/matter-labs/zksync-era/commit/9fc2d95ebaa3670d573a2ed022603132be234a0e))


### Bug Fixes

* **api:** Order transaction traces in `debug_traceBlock*` methods ([#924](https://github.com/matter-labs/zksync-era/issues/924)) ([5918ef9](https://github.com/matter-labs/zksync-era/commit/5918ef925dae97aee428961c2dc61dd91bf2f07e))
* **db:** Make `get_expected_l1_batch_timestamp()` more efficient ([#963](https://github.com/matter-labs/zksync-era/issues/963)) ([7334679](https://github.com/matter-labs/zksync-era/commit/73346792952c5538aafc42a2ee778f0069a98607))
* **db:** Make `snapshot_recovery` migration backward-compatible ([#961](https://github.com/matter-labs/zksync-era/issues/961)) ([e756762](https://github.com/matter-labs/zksync-era/commit/e756762b934f4f2262ee02404b9d18f2f4431842))
* **zksync_types:** Update SerializationTransactionError::OversizedData description ([#949](https://github.com/matter-labs/zksync-era/issues/949)) ([c95f3ee](https://github.com/matter-labs/zksync-era/commit/c95f3eeb03804ba2739b487288b20f6bf6997e47))

## [20.1.0](https://github.com/matter-labs/zksync-era/compare/core-v20.0.0...core-v20.1.0) (2024-01-23)


### Features

* **api:** Get historical fee input ([#919](https://github.com/matter-labs/zksync-era/issues/919)) ([8e1009f](https://github.com/matter-labs/zksync-era/commit/8e1009fc4626465524183ae10a8ff26739fbb647))
* ZK Stack framework MVP ([#880](https://github.com/matter-labs/zksync-era/issues/880)) ([3e5c528](https://github.com/matter-labs/zksync-era/commit/3e5c528767e907e116e29310460019e2bf9161d1))


### Bug Fixes

* **test:** Provide higher min allowance in estimation ([#923](https://github.com/matter-labs/zksync-era/issues/923)) ([d379612](https://github.com/matter-labs/zksync-era/commit/d37961296c102555e7128424fb1e9b998579b1de))

## [20.0.0](https://github.com/matter-labs/zksync-era/compare/core-v19.2.0...core-v20.0.0) (2024-01-19)


### ⚠ BREAKING CHANGES

* **vm:** fee model updates + 1.4.1 ([#791](https://github.com/matter-labs/zksync-era/issues/791))

### Features

* **api:** Consider State keeper fee model input in the API ([#901](https://github.com/matter-labs/zksync-era/issues/901)) ([3211687](https://github.com/matter-labs/zksync-era/commit/32116878ba0ac68a7b90afa1a7e0fe170bdcd902))
* **api:** Make Web3 API server work with pruned data ([#838](https://github.com/matter-labs/zksync-era/issues/838)) ([0b7cd0b](https://github.com/matter-labs/zksync-era/commit/0b7cd0b50ead2406915528becad2fac8b7e48f85))
* **vm:** fee model updates + 1.4.1 ([#791](https://github.com/matter-labs/zksync-era/issues/791)) ([3564aff](https://github.com/matter-labs/zksync-era/commit/3564affbb246c87d668ea2ec74809384bc9d621f))


### Bug Fixes

* addresses broken links in preparation for ci link check ([#869](https://github.com/matter-labs/zksync-era/issues/869)) ([a78d03c](https://github.com/matter-labs/zksync-era/commit/a78d03cc53d0097f6be892de65a2c35bd7f1baa3))
* Incorrect exposing of log indexes ([#896](https://github.com/matter-labs/zksync-era/issues/896)) ([12974fc](https://github.com/matter-labs/zksync-era/commit/12974fcba74c5704b89fa63d7f73222d4fa58228))

## [19.2.0](https://github.com/matter-labs/zksync-era/compare/core-v19.1.1...core-v19.2.0) (2024-01-17)


### Features

* adds `zk linkcheck` to zk tool and updates zk env for `zk linkcheck` ci usage ([#868](https://github.com/matter-labs/zksync-era/issues/868)) ([d64f584](https://github.com/matter-labs/zksync-era/commit/d64f584f6d505b19cd6424928e9dc68e370e17fd))
* **contract-verifier:** Support zkVM solc contract verification ([#854](https://github.com/matter-labs/zksync-era/issues/854)) ([1ed5a95](https://github.com/matter-labs/zksync-era/commit/1ed5a95462dbd73151acd8afbc4ab6158a2aecda))
* **en:** Make batch status updater work with pruned data ([#863](https://github.com/matter-labs/zksync-era/issues/863)) ([3a07890](https://github.com/matter-labs/zksync-era/commit/3a07890dacebf6179636c44d7cce1afd21ab49eb))
* rewritten gossip sync to be async from block processing ([#711](https://github.com/matter-labs/zksync-era/issues/711)) ([3af4644](https://github.com/matter-labs/zksync-era/commit/3af4644f428af0328cdea0fbae8a8f965489c6c4))

## [19.1.1](https://github.com/matter-labs/zksync-era/compare/core-v19.1.0...core-v19.1.1) (2024-01-12)


### Bug Fixes

* **vm:** `inspect_transaction_with_bytecode_compression` for old VMs ([#862](https://github.com/matter-labs/zksync-era/issues/862)) ([077c0c6](https://github.com/matter-labs/zksync-era/commit/077c0c689317fa33c9bf3623942b565e8471f418))

## [19.1.0](https://github.com/matter-labs/zksync-era/compare/core-v19.0.0...core-v19.1.0) (2024-01-10)


### Features

* address remaining spelling issues in dev comments and turns on dev_comments in cfg ([#827](https://github.com/matter-labs/zksync-era/issues/827)) ([1fd0afd](https://github.com/matter-labs/zksync-era/commit/1fd0afdcd9b6c344e1f5dac93fda5aa25c106b2f))
* **core:** removes multiple tokio runtimes and worker number setting. ([#826](https://github.com/matter-labs/zksync-era/issues/826)) ([b8b190f](https://github.com/matter-labs/zksync-era/commit/b8b190f886f1d13602a0b2cc8a2b8525e68b1033))
* fix spelling in dev comments in `core/lib/*` - continued ([#683](https://github.com/matter-labs/zksync-era/issues/683)) ([0421fe6](https://github.com/matter-labs/zksync-era/commit/0421fe6b3e9629fdad2fb88ad5710200825adc91))
* fix spelling in dev comments in `core/lib/*` - continued ([#684](https://github.com/matter-labs/zksync-era/issues/684)) ([b46c2e9](https://github.com/matter-labs/zksync-era/commit/b46c2e9cbbcd048647f998810c8d550f8ad0c1f4))
* fix spelling in dev comments in `core/lib/multivm` - continued  ([#682](https://github.com/matter-labs/zksync-era/issues/682)) ([3839d39](https://github.com/matter-labs/zksync-era/commit/3839d39eb6b6d111ec556948c88d1eb9c6ab5e4a))
* fix spelling in dev comments in `core/lib/zksync_core` - continued ([#685](https://github.com/matter-labs/zksync-era/issues/685)) ([70c3feb](https://github.com/matter-labs/zksync-era/commit/70c3febbf0445d2e0c22a942eaf643828aee045d))
* **state-keeper:** circuits seal criterion ([#729](https://github.com/matter-labs/zksync-era/issues/729)) ([c4a86bb](https://github.com/matter-labs/zksync-era/commit/c4a86bbbc5697b5391a517299bbd7a5e882a7314))
* **state-keeper:** Reject transactions that fail to publish bytecodes ([#832](https://github.com/matter-labs/zksync-era/issues/832)) ([0a010f0](https://github.com/matter-labs/zksync-era/commit/0a010f0a6f6682cedc49cb12ab9f9dfcdbccf68e))
* **vm:** Add batch input abstraction ([#817](https://github.com/matter-labs/zksync-era/issues/817)) ([997db87](https://github.com/matter-labs/zksync-era/commit/997db872455351a484c3161d0a733a4bc59dd684))


### Bug Fixes

* oldest unpicked batch ([#692](https://github.com/matter-labs/zksync-era/issues/692)) ([a6c869d](https://github.com/matter-labs/zksync-era/commit/a6c869d88c64a986405bbdfb15cab88e910d1e03))
* **state-keeper:** Updates manager keeps track of fictive block metrics ([#843](https://github.com/matter-labs/zksync-era/issues/843)) ([88fd724](https://github.com/matter-labs/zksync-era/commit/88fd7247c377efce703cd1caeffa4ecd61ed0d7f))
* **vm:** fix circuit tracer ([#837](https://github.com/matter-labs/zksync-era/issues/837)) ([83fc7be](https://github.com/matter-labs/zksync-era/commit/83fc7be3cb9f4d3082b8b9fa8b8f568330bf744f))

## [19.0.0](https://github.com/matter-labs/zksync-era/compare/core-v18.13.0...core-v19.0.0) (2024-01-05)


### ⚠ BREAKING CHANGES

* **vm:** Release v19 - remove allowlist ([#747](https://github.com/matter-labs/zksync-era/issues/747))

### Features

* **en:** Make consistency checker work with pruned data ([#742](https://github.com/matter-labs/zksync-era/issues/742)) ([ae6e18e](https://github.com/matter-labs/zksync-era/commit/ae6e18e5412cadefbc03307a476d6b96c41f04e1))
* **eth_sender:** Remove generic bounds on L1TxParamsProvider in EthSender ([#799](https://github.com/matter-labs/zksync-era/issues/799)) ([29a4f52](https://github.com/matter-labs/zksync-era/commit/29a4f5299c95e0b338010a6baf83f196ece3a530))
* **merkle tree:** Finalize metadata calculator snapshot recovery logic ([#798](https://github.com/matter-labs/zksync-era/issues/798)) ([c83db35](https://github.com/matter-labs/zksync-era/commit/c83db35f0929a412bc4d89fbee1448d32c54a83f))
* **prover:** Remove circuit-synthesizer ([#801](https://github.com/matter-labs/zksync-era/issues/801)) ([1426b1b](https://github.com/matter-labs/zksync-era/commit/1426b1ba3c8b700e0531087b781ced0756c12e3c))
* **prover:** Remove old prover ([#810](https://github.com/matter-labs/zksync-era/issues/810)) ([8be1925](https://github.com/matter-labs/zksync-era/commit/8be1925b18dcbf268eb03b8ea5f07adfd5330876))
* **snapshot creator:** Make snapshot creator fault-tolerant ([#691](https://github.com/matter-labs/zksync-era/issues/691)) ([286c7d1](https://github.com/matter-labs/zksync-era/commit/286c7d15a623604e01effa7119de3362f0fb4eb9))
* **vm:** Add boojum integration folder ([#805](https://github.com/matter-labs/zksync-era/issues/805)) ([4071e90](https://github.com/matter-labs/zksync-era/commit/4071e90578e0fc8c027a4d2a30d09d96db942b4f))
* **vm:** Make utils version-dependent ([#809](https://github.com/matter-labs/zksync-era/issues/809)) ([e5fbcb5](https://github.com/matter-labs/zksync-era/commit/e5fbcb5dfc2a7d2582f40a481c861fb2f4dd5fb0))
* **vm:** Release v19 - remove allowlist ([#747](https://github.com/matter-labs/zksync-era/issues/747)) ([0e2bc56](https://github.com/matter-labs/zksync-era/commit/0e2bc561b9642b854718adcc86087a3e9762cf5d))
* **vm:** Separate boojum integration vm ([#806](https://github.com/matter-labs/zksync-era/issues/806)) ([61712a6](https://github.com/matter-labs/zksync-era/commit/61712a636f69be70d75719c04f364d679ef624e0))


### Bug Fixes

* **db:** Fix parsing statement timeout from env ([#818](https://github.com/matter-labs/zksync-era/issues/818)) ([3f663ec](https://github.com/matter-labs/zksync-era/commit/3f663eca2f38f4373339ad024e6578099c693af6))
* **prover:** Remove old prover subsystems tables ([#812](https://github.com/matter-labs/zksync-era/issues/812)) ([9d0aefc](https://github.com/matter-labs/zksync-era/commit/9d0aefc1ef4992e19d7b15ec1ce34697e61a3464))
* **prover:** Remove prover-utils from core ([#819](https://github.com/matter-labs/zksync-era/issues/819)) ([2ceb911](https://github.com/matter-labs/zksync-era/commit/2ceb9114659f4c4583c87b1bbc8ee230eb1c44db))

## [18.13.0](https://github.com/matter-labs/zksync-era/compare/core-v18.12.0...core-v18.13.0) (2024-01-02)


### Features

* **contract-verifier:** add zksolc v1.3.19 ([#797](https://github.com/matter-labs/zksync-era/issues/797)) ([2635570](https://github.com/matter-labs/zksync-era/commit/26355705c8c084344464458f3275c311c392c47f))
* Remove generic bounds on L1GasPriceProvider ([#792](https://github.com/matter-labs/zksync-era/issues/792)) ([edf071d](https://github.com/matter-labs/zksync-era/commit/edf071d39d4dd8e297fd2fb2244574d5e0537b38))
* Remove TPS limiter from TX Sender ([#793](https://github.com/matter-labs/zksync-era/issues/793)) ([d0e9296](https://github.com/matter-labs/zksync-era/commit/d0e929652eb431f6b1bc20f83d7c21d2a978293a))

## [18.12.0](https://github.com/matter-labs/zksync-era/compare/core-v18.11.0...core-v18.12.0) (2023-12-25)


### Features

* **get-tokens:** filter tokens by `well_known` ([#767](https://github.com/matter-labs/zksync-era/issues/767)) ([9c99e13](https://github.com/matter-labs/zksync-era/commit/9c99e13ca0a4de678a4ce5bf7e2d5880d79c0e66))

## [18.11.0](https://github.com/matter-labs/zksync-era/compare/core-v18.10.3...core-v18.11.0) (2023-12-25)


### Features

* Revert "feat: Remove zks_getConfirmedTokens method" ([#765](https://github.com/matter-labs/zksync-era/issues/765)) ([6e7ed12](https://github.com/matter-labs/zksync-era/commit/6e7ed124e816f5ba1d2ba3e8efaf281cd2c055dd))

## [18.10.3](https://github.com/matter-labs/zksync-era/compare/core-v18.10.2...core-v18.10.3) (2023-12-25)


### Bug Fixes

* **core:** do not unwrap unexisting calldata in commitment and regenerate it ([#762](https://github.com/matter-labs/zksync-era/issues/762)) ([ec104ef](https://github.com/matter-labs/zksync-era/commit/ec104ef01136d1a455f40163c2ced92dbc5917e2))

## [18.10.2](https://github.com/matter-labs/zksync-era/compare/core-v18.10.1...core-v18.10.2) (2023-12-25)


### Bug Fixes

* **vm:** Get pubdata bytes from vm ([#756](https://github.com/matter-labs/zksync-era/issues/756)) ([6c6f1ab](https://github.com/matter-labs/zksync-era/commit/6c6f1ab078485669002e50197b35ab1b6a38cdb9))

## [18.10.1](https://github.com/matter-labs/zksync-era/compare/core-v18.10.0...core-v18.10.1) (2023-12-25)


### Bug Fixes

* **sequencer:** don't stall blockchain on failed L1 tx ([#759](https://github.com/matter-labs/zksync-era/issues/759)) ([50cd7c4](https://github.com/matter-labs/zksync-era/commit/50cd7c41f71757a3f2ffb36a6c1e1fa6b4372703))

## [18.10.0](https://github.com/matter-labs/zksync-era/compare/core-v18.9.0...core-v18.10.0) (2023-12-25)


### Features

* **api:** Add metrics for `jsonrpsee` subscriptions ([#733](https://github.com/matter-labs/zksync-era/issues/733)) ([39fd71c](https://github.com/matter-labs/zksync-era/commit/39fd71cc2a0ffda45933fc99c4dac6d9beb92ad0))
* **api:** remove jsonrpc backend ([#693](https://github.com/matter-labs/zksync-era/issues/693)) ([b3f0417](https://github.com/matter-labs/zksync-era/commit/b3f0417fd4512f98d7e579eb5b3b03c7f4b92e18))
* applied status snapshots dal ([#679](https://github.com/matter-labs/zksync-era/issues/679)) ([2e9f23b](https://github.com/matter-labs/zksync-era/commit/2e9f23b46c31a9538d4a55bed75c5df3ed8e8f63))
* **en:** Make reorg detector work with pruned data ([#712](https://github.com/matter-labs/zksync-era/issues/712)) ([c4185d5](https://github.com/matter-labs/zksync-era/commit/c4185d5b6526cc9ec42e6941d76453cb693988bd))
* Remove data fetchers ([#694](https://github.com/matter-labs/zksync-era/issues/694)) ([f48d677](https://github.com/matter-labs/zksync-era/commit/f48d6773e1e30fede44075f8862c68e7a8173cbb))
* Remove zks_getConfirmedTokens method ([#719](https://github.com/matter-labs/zksync-era/issues/719)) ([9298b1b](https://github.com/matter-labs/zksync-era/commit/9298b1b916ad5f81160c66c061370f804d129d97))


### Bug Fixes

* added waiting for prometheus to finish ([#745](https://github.com/matter-labs/zksync-era/issues/745)) ([eed330d](https://github.com/matter-labs/zksync-era/commit/eed330dd2e47114d9d0ea29c074259a0bc016f78))
* **EN:** temporary produce a warning on pubdata mismatch with L1 ([#758](https://github.com/matter-labs/zksync-era/issues/758)) ([0a7a4da](https://github.com/matter-labs/zksync-era/commit/0a7a4da52926d1db8dfe72aef78390cba3754627))
* **prover:** Add logging for prover + WVGs ([#723](https://github.com/matter-labs/zksync-era/issues/723)) ([d7ce14c](https://github.com/matter-labs/zksync-era/commit/d7ce14c5d0434326a1ebf406d77c20676ae526ae))
* remove leftovers after [#693](https://github.com/matter-labs/zksync-era/issues/693) ([#720](https://github.com/matter-labs/zksync-era/issues/720)) ([e93aa35](https://github.com/matter-labs/zksync-era/commit/e93aa358c43e60d5640224e5422a40d91cd4b9a0))

## [18.9.0](https://github.com/matter-labs/zksync-era/compare/core-v18.8.0...core-v18.9.0) (2023-12-19)


### Features

* Add ecadd and ecmul to the list of precompiles upon genesis ([#669](https://github.com/matter-labs/zksync-era/issues/669)) ([0be35b8](https://github.com/matter-labs/zksync-era/commit/0be35b82fc63e88b6d709b644e437194f7559483))
* **api:** Do not return receipt if tx was not included to the batch ([#706](https://github.com/matter-labs/zksync-era/issues/706)) ([625d632](https://github.com/matter-labs/zksync-era/commit/625d632934ac63ad7479de50d65f83e6f144c7dd))
* proto serialization/deserialization of snapshots creator objects ([#667](https://github.com/matter-labs/zksync-era/issues/667)) ([9f096a4](https://github.com/matter-labs/zksync-era/commit/9f096a4dd362fbd74a35fa1e9af4f111f69f4317))
* zk fmt sqlx-queries ([#533](https://github.com/matter-labs/zksync-era/issues/533)) ([6982343](https://github.com/matter-labs/zksync-era/commit/69823439675411b3239ef0a24c6bfe4d3610161b))


### Bug Fixes

* **en:** Downgrade miniblock hash equality assertion to warning ([#695](https://github.com/matter-labs/zksync-era/issues/695)) ([2ef3ec8](https://github.com/matter-labs/zksync-era/commit/2ef3ec804573ba4bbf8f44f19a3b5616b297c796))


### Performance Improvements

* remove unnecessary to_vec ([#702](https://github.com/matter-labs/zksync-era/issues/702)) ([c55a658](https://github.com/matter-labs/zksync-era/commit/c55a6582eae3af7f92cdeceb4e50b81701665f96))

## [18.8.0](https://github.com/matter-labs/zksync-era/compare/core-v18.7.0...core-v18.8.0) (2023-12-13)


### Features

* **api:** Sunset API translator ([#675](https://github.com/matter-labs/zksync-era/issues/675)) ([846fd33](https://github.com/matter-labs/zksync-era/commit/846fd33a74734520ae1bb57d8bc8abca71e16f25))
* **core:** Merge bounded and unbounded gas adjuster ([#678](https://github.com/matter-labs/zksync-era/issues/678)) ([f3c3bf5](https://github.com/matter-labs/zksync-era/commit/f3c3bf53b3136b2fe8c17638c83fda3328fd6033))
* **dal:** Make ConnectionPoolBuilder owned ([#676](https://github.com/matter-labs/zksync-era/issues/676)) ([1153c42](https://github.com/matter-labs/zksync-era/commit/1153c42f9d0e7cfe78da64d4508974e74afea4ee))
* Implemented 1 validator consensus for the main node ([#554](https://github.com/matter-labs/zksync-era/issues/554)) ([9c59838](https://github.com/matter-labs/zksync-era/commit/9c5983858d9dd84de360e6a082369a06bb58e924))
* **merkle tree:** Snapshot recovery in metadata calculator ([#607](https://github.com/matter-labs/zksync-era/issues/607)) ([f49418b](https://github.com/matter-labs/zksync-era/commit/f49418b24cdfa905e571568cb3393296c951e903))


### Bug Fixes

* dropping installed filters ([#670](https://github.com/matter-labs/zksync-era/issues/670)) ([985c737](https://github.com/matter-labs/zksync-era/commit/985c7375f6fa192b45473d8ba0b7dacb9314a482))

## [18.7.0](https://github.com/matter-labs/zksync-era/compare/core-v18.6.1...core-v18.7.0) (2023-12-12)


### Features

* **contract-verifier:** Add zksolc v1.3.18 ([#654](https://github.com/matter-labs/zksync-era/issues/654)) ([77f91fe](https://github.com/matter-labs/zksync-era/commit/77f91fe253a0876e56de4aee47071fe249386fc7))
* **en:** Check block hash correspondence ([#572](https://github.com/matter-labs/zksync-era/issues/572)) ([28f5642](https://github.com/matter-labs/zksync-era/commit/28f5642c35800997879bc549fca9e960c4516d21))
* **en:** Remove `SyncBlock.root_hash` ([#633](https://github.com/matter-labs/zksync-era/issues/633)) ([d4cc6e5](https://github.com/matter-labs/zksync-era/commit/d4cc6e564642b4c49ef4a546cd1c86821327683c))
* Snapshot Creator ([#498](https://github.com/matter-labs/zksync-era/issues/498)) ([270edee](https://github.com/matter-labs/zksync-era/commit/270edee34402ecbd1761bc1fca559ef2205f71e8))


### Bug Fixes

* Cursor not moving correctly after poll in `get_filter_changes` ([#546](https://github.com/matter-labs/zksync-era/issues/546)) ([ec5907b](https://github.com/matter-labs/zksync-era/commit/ec5907b70ff7d868a05b685a1641d96dc4fa9d69))
* fix docs error ([#635](https://github.com/matter-labs/zksync-era/issues/635)) ([883c128](https://github.com/matter-labs/zksync-era/commit/883c1282f7771fb16a41d45391b74243021271e3))
* follow up metrics fixes ([#648](https://github.com/matter-labs/zksync-era/issues/648)) ([a317c7a](https://github.com/matter-labs/zksync-era/commit/a317c7ab68219cb376d08c8d1ec210c63b3c269f))
* Follow up metrics fixes vol.2 ([#656](https://github.com/matter-labs/zksync-era/issues/656)) ([5c1aea2](https://github.com/matter-labs/zksync-era/commit/5c1aea2a94d7eded26c3a4ae4973ff983c15e7fa))
* **job-processor:** `max_attepts_reached` metric ([#626](https://github.com/matter-labs/zksync-era/issues/626)) ([dd9b308](https://github.com/matter-labs/zksync-era/commit/dd9b308be9b0a6e37aad75f6f54b98e30a2ae14e))
* update google cloud dependencies that do not depend on rsa ([#622](https://github.com/matter-labs/zksync-era/issues/622)) ([8a8cad6](https://github.com/matter-labs/zksync-era/commit/8a8cad6ce62f2d34bb34adcd956f6920c08f94b8))

## [18.6.1](https://github.com/matter-labs/zksync-era/compare/core-v18.6.0...core-v18.6.1) (2023-12-06)


### Performance Improvements

* **external-node:** Use async miniblock sealing in external IO ([#611](https://github.com/matter-labs/zksync-era/issues/611)) ([5cf7210](https://github.com/matter-labs/zksync-era/commit/5cf7210dc77bb615944352f23ed39fad324b914f))

## [18.6.0](https://github.com/matter-labs/zksync-era/compare/core-v18.5.0...core-v18.6.0) (2023-12-05)


### Features

* **contract-verifier:** Support verification for zksolc v1.3.17 ([#606](https://github.com/matter-labs/zksync-era/issues/606)) ([b65fedd](https://github.com/matter-labs/zksync-era/commit/b65fedd6894497a4c9fbf38d558ccfaca535d1d2))


### Bug Fixes

* Fix database connections in house keeper ([#610](https://github.com/matter-labs/zksync-era/issues/610)) ([aeaaecb](https://github.com/matter-labs/zksync-era/commit/aeaaecb54b6bd3f173727531418dc242357b2aee))

## [18.5.0](https://github.com/matter-labs/zksync-era/compare/core-v18.4.0...core-v18.5.0) (2023-12-05)


### Features

* Add metric to CallTracer for calculating maximum depth of the calls ([#535](https://github.com/matter-labs/zksync-era/issues/535)) ([19c84ce](https://github.com/matter-labs/zksync-era/commit/19c84ce624d53735133fa3b12c7f980e8c14260d))
* Add various metrics to the Prover subsystems ([#541](https://github.com/matter-labs/zksync-era/issues/541)) ([58a4e6c](https://github.com/matter-labs/zksync-era/commit/58a4e6c4c22bd7f002ede1c6def0dc260706185e))


### Bug Fixes

* Sync protocol version between consensus and server blocks ([#568](https://github.com/matter-labs/zksync-era/issues/568)) ([56776f9](https://github.com/matter-labs/zksync-era/commit/56776f929f547b1a91c5b70f89e87ef7dc25c65a))

## [18.4.0](https://github.com/matter-labs/zksync-era/compare/core-v18.3.1...core-v18.4.0) (2023-12-01)


### Features

* adds spellchecker workflow, and corrects misspelled words ([#559](https://github.com/matter-labs/zksync-era/issues/559)) ([beac0a8](https://github.com/matter-labs/zksync-era/commit/beac0a85bb1535b05c395057171f197cd976bf82))
* **en:** Support arbitrary genesis block for external nodes ([#537](https://github.com/matter-labs/zksync-era/issues/537)) ([15d7eaf](https://github.com/matter-labs/zksync-era/commit/15d7eaf872e222338810243865cec9dff7f6e799))
* **merkle tree:** Remove enumeration index assignment from Merkle tree ([#551](https://github.com/matter-labs/zksync-era/issues/551)) ([e2c1b20](https://github.com/matter-labs/zksync-era/commit/e2c1b20e361e6ee2f5ac69cefe75d9c5575eb2f7))
* Restore commitment test in Boojum integration ([#539](https://github.com/matter-labs/zksync-era/issues/539)) ([06f510d](https://github.com/matter-labs/zksync-era/commit/06f510d00f855ddafaebb504f7ea799700221072))


### Bug Fixes

* Change no pending batches 404 error into a success response ([#279](https://github.com/matter-labs/zksync-era/issues/279)) ([e8fd805](https://github.com/matter-labs/zksync-era/commit/e8fd805c8be7980de7676bca87cfc2d445aab9e1))
* **vm:** Expose additional types and traits ([#563](https://github.com/matter-labs/zksync-era/issues/563)) ([bd268ac](https://github.com/matter-labs/zksync-era/commit/bd268ac02bc3530c1d3247cb9496c3e13c2e52d9))
* **witness_generator:** Disable BWIP dependency ([#573](https://github.com/matter-labs/zksync-era/issues/573)) ([e05d955](https://github.com/matter-labs/zksync-era/commit/e05d955036c76a29f9b6e900872c69e20278e045))

## [18.3.1](https://github.com/matter-labs/zksync-era/compare/core-v18.3.0...core-v18.3.1) (2023-11-28)


### Bug Fixes

* **external-node:** Check txs at insert time instead of read time ([#555](https://github.com/matter-labs/zksync-era/issues/555)) ([9ea02a1](https://github.com/matter-labs/zksync-era/commit/9ea02a1b2e7c861882f10c8cbe1997f6bb96d9cf))
* Update comments post-hotfix ([#556](https://github.com/matter-labs/zksync-era/issues/556)) ([339e450](https://github.com/matter-labs/zksync-era/commit/339e45035e85eba7d60b533221be92ce78643705))

## [18.3.0](https://github.com/matter-labs/zksync-era/compare/core-v18.2.0...core-v18.3.0) (2023-11-28)


### Features

* **contract-verifier:** Add zkvyper 1.3.13 ([#552](https://github.com/matter-labs/zksync-era/issues/552)) ([198deda](https://github.com/matter-labs/zksync-era/commit/198deda685a5bf44dc41911fe7b7797a219aa29c))


### Bug Fixes

* **house_keeper:** Emit the correct circuit_id for aggregation round 2 ([#547](https://github.com/matter-labs/zksync-era/issues/547)) ([9cada1a](https://github.com/matter-labs/zksync-era/commit/9cada1a17469e876cbf82d923e3d59f21576ec94))

## [18.2.0](https://github.com/matter-labs/zksync-era/compare/core-v18.1.0...core-v18.2.0) (2023-11-27)


### Features

* **en:** Implement gossip fetcher ([#371](https://github.com/matter-labs/zksync-era/issues/371)) ([a49b61d](https://github.com/matter-labs/zksync-era/commit/a49b61d7769f9dd7b4cbc4905f8f8a23abfb541c))
* **state-keeper:** reapply computational gas limit ([#544](https://github.com/matter-labs/zksync-era/issues/544)) ([698dbc3](https://github.com/matter-labs/zksync-era/commit/698dbc355af3220bbcb608ced2559362e96204de))
* **state-keeper:** Remove computational gas limit from boojum protocol version ([#536](https://github.com/matter-labs/zksync-era/issues/536)) ([e59a7c6](https://github.com/matter-labs/zksync-era/commit/e59a7c6552a9c99e56f0d37103386acac6a9c1b5))


### Bug Fixes

* **core:** differentiate l2 to l1 logs tree size for pre and post boojum batches ([#538](https://github.com/matter-labs/zksync-era/issues/538)) ([1e9e556](https://github.com/matter-labs/zksync-era/commit/1e9e55651a95b509b5dfd644b8f9f3c718e41804))
* **proof_data_handler:** Feature flag state_diff_hash check ([#545](https://github.com/matter-labs/zksync-era/issues/545)) ([0cab378](https://github.com/matter-labs/zksync-era/commit/0cab378886d4408a28bb1b71afb3e46c0b82a7c6))
* **prover:** use a more performant query to get next job for FRI prover ([#527](https://github.com/matter-labs/zksync-era/issues/527)) ([2cddf3c](https://github.com/matter-labs/zksync-era/commit/2cddf3c0fa786394161060445aa8a085173e3f71))

## [18.1.0](https://github.com/matter-labs/zksync-era/compare/core-v18.0.3...core-v18.1.0) (2023-11-20)


### Features

* added consensus types and consensus column to miniblocks table ([#490](https://github.com/matter-labs/zksync-era/issues/490)) ([f9ae0ad](https://github.com/matter-labs/zksync-era/commit/f9ae0ad56b17fffa4b400ec2376517a2b630b862))
* Adds `prover_group_id` label into `fri_prover_prover_job` metric ([#503](https://github.com/matter-labs/zksync-era/issues/503)) ([851e800](https://github.com/matter-labs/zksync-era/commit/851e800721a627742d6781d6162009d61f83c1af))
* **core:** adds a get proof endpoint in zks namespace to http endpoint on main node ([#504](https://github.com/matter-labs/zksync-era/issues/504)) ([0ac4a4d](https://github.com/matter-labs/zksync-era/commit/0ac4a4ddb87d7728a99a29df9adeded5822e1def))
* **merkle tree:** Allow random-order tree recovery ([#485](https://github.com/matter-labs/zksync-era/issues/485)) ([146e4cf](https://github.com/matter-labs/zksync-era/commit/146e4cf2f8d890ff0a8d33229e224442e14be437))


### Bug Fixes

* **core:** add tree url to jsonrpsee server on main node ([#512](https://github.com/matter-labs/zksync-era/issues/512)) ([7c137b7](https://github.com/matter-labs/zksync-era/commit/7c137b72b16a8671a27c24b378d33320877d6557))
* Fixes panic on unknown FRI prover group id ([#522](https://github.com/matter-labs/zksync-era/issues/522)) ([1c315f3](https://github.com/matter-labs/zksync-era/commit/1c315f3245be770ad729c19977861b79d9e438a5))

## [18.0.3](https://github.com/matter-labs/zksync-era/compare/core-v18.0.2...core-v18.0.3) (2023-11-16)


### Bug Fixes

* **proof-data-handler:** Check commitments only for post-boojum (again) ([#502](https://github.com/matter-labs/zksync-era/issues/502)) ([ff636ca](https://github.com/matter-labs/zksync-era/commit/ff636ca9250d0276098e4b5b4a5f7a44a0717d06))

## [18.0.2](https://github.com/matter-labs/zksync-era/compare/core-v18.0.1...core-v18.0.2) (2023-11-16)


### Bug Fixes

* **api:** `debug_trace*` no longer throws error if the `TracerConfig` object is incomplete ([#468](https://github.com/matter-labs/zksync-era/issues/468)) ([cb873bd](https://github.com/matter-labs/zksync-era/commit/cb873bd0da6b421160ce96b8d578f1351861f376))
* **proof-data-handler:** Check commitments only for post-boojum ([#500](https://github.com/matter-labs/zksync-era/issues/500)) ([c3a7651](https://github.com/matter-labs/zksync-era/commit/c3a7651987f6efaeca55ccf328e5aaaa5cc66bde))

## [18.0.1](https://github.com/matter-labs/zksync-era/compare/core-v18.0.0...core-v18.0.1) (2023-11-15)


### Bug Fixes

* **metadata-calculator:** Do not require events_queue for old batches ([#492](https://github.com/matter-labs/zksync-era/issues/492)) ([0c454fc](https://github.com/matter-labs/zksync-era/commit/0c454fc6cdd1fb32074389643bd40c899983283f))

## [18.0.0](https://github.com/matter-labs/zksync-era/compare/core-v17.1.0...core-v18.0.0) (2023-11-14)


### ⚠ BREAKING CHANGES

* boojum integration ([#112](https://github.com/matter-labs/zksync-era/issues/112))

### Features

* **basic_witness_input_producer:** Witness inputs queued after BWIP run ([#345](https://github.com/matter-labs/zksync-era/issues/345)) ([9c2be91](https://github.com/matter-labs/zksync-era/commit/9c2be91ec1b9bd30c44e210d943539630a857825))
* boojum integration ([#112](https://github.com/matter-labs/zksync-era/issues/112)) ([e76d346](https://github.com/matter-labs/zksync-era/commit/e76d346d02ded771dea380aa8240da32119d7198))
* **core:** adds a get proof endpoint in zks namespace ([#455](https://github.com/matter-labs/zksync-era/issues/455)) ([f4313a4](https://github.com/matter-labs/zksync-era/commit/f4313a4e5a67f616d2dfa8f364c47cb73cef1ec7))
* **core:** Split config definitions and deserialization ([#414](https://github.com/matter-labs/zksync-era/issues/414)) ([c7c6b32](https://github.com/matter-labs/zksync-era/commit/c7c6b321a63dbcc7f1af045aa7416e697beab08f))
* **dal:** Do not load config from env in DAL crate ([#444](https://github.com/matter-labs/zksync-era/issues/444)) ([3fe1bb2](https://github.com/matter-labs/zksync-era/commit/3fe1bb21f8d33557353f447811ca86c60f1fe51a))
* **house_keeper:** Remove GCS Blob Cleaner ([#321](https://github.com/matter-labs/zksync-era/issues/321)) ([9548914](https://github.com/matter-labs/zksync-era/commit/9548914bd1be7b6ada52061d961353a763412150))
* **job-processor:** report attempts metrics ([#448](https://github.com/matter-labs/zksync-era/issues/448)) ([ab31f03](https://github.com/matter-labs/zksync-era/commit/ab31f031dfcaa7ddf296786ddccb78e8edd2d3c5))
* **vm:** Use the one interface for all vms ([#277](https://github.com/matter-labs/zksync-era/issues/277)) ([91bb99b](https://github.com/matter-labs/zksync-era/commit/91bb99b232120e29f9ee55208e3325ab37550f0c))


### Bug Fixes

* **boojnet:** various boojnet fixes ([#462](https://github.com/matter-labs/zksync-era/issues/462)) ([f13648c](https://github.com/matter-labs/zksync-era/commit/f13648cf10c0a225dc7b4f64cb3b8195c1a52814))
* change vks upgrade logic ([#491](https://github.com/matter-labs/zksync-era/issues/491)) ([cb394f3](https://github.com/matter-labs/zksync-era/commit/cb394f3c3ce93d345f24e5b9ee34e22ebca3abb0))
* **eth-sender:** Correct ABI for get_verification_key ([#445](https://github.com/matter-labs/zksync-era/issues/445)) ([8af0d85](https://github.com/matter-labs/zksync-era/commit/8af0d85b94cc74f691eb21b59556c0cd6084db01))
* **metadata-calculator:** Save commitment for pre-boojum ([#481](https://github.com/matter-labs/zksync-era/issues/481)) ([664ce33](https://github.com/matter-labs/zksync-era/commit/664ce33622af220a24360f7f11a52a14141c3fdc))
* Versioned L1 batch metadata ([#450](https://github.com/matter-labs/zksync-era/issues/450)) ([8a40dc3](https://github.com/matter-labs/zksync-era/commit/8a40dc38669867c89dfe54bf71c1f461a9db1fc7))
* **vm:** storage_refunds for `vm_refunds_enhancement` ([#449](https://github.com/matter-labs/zksync-era/issues/449)) ([1e1e59f](https://github.com/matter-labs/zksync-era/commit/1e1e59fbbb4e7b0667f080fcd922a5302d819f22))

## [17.1.0](https://github.com/matter-labs/zksync-era/compare/core-v16.2.0...core-v17.1.0) (2023-11-03)


### ⚠ BREAKING CHANGES

* Update to protocol version 17 ([#384](https://github.com/matter-labs/zksync-era/issues/384))

### Features

* **en:** Cache blocks in `fetch_l2_block` ([#403](https://github.com/matter-labs/zksync-era/issues/403)) ([b94c845](https://github.com/matter-labs/zksync-era/commit/b94c8450a4b4905a7db8967bf42d37493cf31e0b))
* Port boojum eth-sender changes ([#293](https://github.com/matter-labs/zksync-era/issues/293)) ([8027326](https://github.com/matter-labs/zksync-era/commit/80273264a9512bc1e6f1d1f4372107f9167260b1))
* **state-keeper:** Disable some seal criteria for boojum ([#390](https://github.com/matter-labs/zksync-era/issues/390)) ([2343532](https://github.com/matter-labs/zksync-era/commit/2343532cd48bcc07ec939a25c205d521955dd05a))
* Update to protocol version 17 ([#384](https://github.com/matter-labs/zksync-era/issues/384)) ([ba271a5](https://github.com/matter-labs/zksync-era/commit/ba271a5f34d64d04c0135b8811685b80f26a8c32))
* **vm:** Make calculation for pubdata a bit more percise ([#392](https://github.com/matter-labs/zksync-era/issues/392)) ([6d0e61c](https://github.com/matter-labs/zksync-era/commit/6d0e61cba86d61b68f3657852283dd99d2b6530f))


### Bug Fixes

* bump zksolc from yanked version to 1.3.16 ([#348](https://github.com/matter-labs/zksync-era/issues/348)) ([c32b88f](https://github.com/matter-labs/zksync-era/commit/c32b88fe8fe7e8892c857b8fc36037ecd0892fa1))
* **db-index:** Add missing index from FRI prover jobs ([#334](https://github.com/matter-labs/zksync-era/issues/334)) ([730447f](https://github.com/matter-labs/zksync-era/commit/730447f90efb2478097f06c2ed5d965ac65b7874))
* **db-query:** use join instead of nested query for FRI prover extracting ([#364](https://github.com/matter-labs/zksync-era/issues/364)) ([f9cc831](https://github.com/matter-labs/zksync-era/commit/f9cc831ddc96467395a48a8ba2e6238c0fcb7341))
* **db-query:** use nested query for requeuing FRI prover jobs ([#399](https://github.com/matter-labs/zksync-era/issues/399)) ([3890542](https://github.com/matter-labs/zksync-era/commit/3890542c8e736a313306391259b9b356d00e2ef9))
* incorrect directory of intrinsic.rs generated.  ([#332](https://github.com/matter-labs/zksync-era/issues/332)) ([#336](https://github.com/matter-labs/zksync-era/issues/336)) ([eefaad0](https://github.com/matter-labs/zksync-era/commit/eefaad0e7e7766a269ee9022b3758be4ee32f1a1))

## [16.2.0](https://github.com/matter-labs/zksync-era/compare/core-v16.1.0...core-v16.2.0) (2023-10-26)


### Features

* **basic_witness_producer_input:** Add Basic Witness Producer Input component ([#156](https://github.com/matter-labs/zksync-era/issues/156)) ([3cd24c9](https://github.com/matter-labs/zksync-era/commit/3cd24c92b1f3011a5c43a61238e7fecf1a01ae3d))
* **core:** adding pubdata to statekeeper and merkle tree ([#259](https://github.com/matter-labs/zksync-era/issues/259)) ([1659c84](https://github.com/matter-labs/zksync-era/commit/1659c840687e4c71e8d5f7be3f2e66785d5fd0dc))


### Bug Fixes

* **db:** Fix root cause of RocksDB misbehavior ([#301](https://github.com/matter-labs/zksync-era/issues/301)) ([d6c30ab](https://github.com/matter-labs/zksync-era/commit/d6c30abbb842b1db04db276838144737525e3f86))
* **en:** gracefully shutdown en waiting for reorg detector ([#270](https://github.com/matter-labs/zksync-era/issues/270)) ([f048485](https://github.com/matter-labs/zksync-era/commit/f048485a0cdb7631c61b241d64d6a428f33178be))

## [16.1.0](https://github.com/matter-labs/zksync-era/compare/core-v16.0.2...core-v16.1.0) (2023-10-24)


### Features

* Add new commitments ([#219](https://github.com/matter-labs/zksync-era/issues/219)) ([a19256e](https://github.com/matter-labs/zksync-era/commit/a19256e2b369f059ab1a469e14de6654768d37aa))
* arm64 zk-environment rust Docker images and other ([#296](https://github.com/matter-labs/zksync-era/issues/296)) ([33174aa](https://github.com/matter-labs/zksync-era/commit/33174aa5955596f4fc8a283b1c150b8f957cce40))
* **config:** Extract everything not related to the env config from zksync_config crate ([#245](https://github.com/matter-labs/zksync-era/issues/245)) ([42c64e9](https://github.com/matter-labs/zksync-era/commit/42c64e91e13b6b37619f1459f927fa046ef01097))
* **eth-watch:** process governor upgrades ([#247](https://github.com/matter-labs/zksync-era/issues/247)) ([d250294](https://github.com/matter-labs/zksync-era/commit/d2502941081fb53387881631c2150803e9f559cc))
* **merkle tree:** Expose Merkle tree API ([#209](https://github.com/matter-labs/zksync-era/issues/209)) ([4010c7e](https://github.com/matter-labs/zksync-era/commit/4010c7ea63e6eb0f0999457fb2e8e8ad92ad988f))
* **merkle tree:** Snapshot recovery for Merkle tree ([#163](https://github.com/matter-labs/zksync-era/issues/163)) ([9e20703](https://github.com/matter-labs/zksync-era/commit/9e2070380e6720d84563a14a2246fc18fdb1f8f9))
* **multivm:** Remove lifetime from multivm ([#218](https://github.com/matter-labs/zksync-era/issues/218)) ([7eda27c](https://github.com/matter-labs/zksync-era/commit/7eda27ca0156225e965f29bc92748083d36ccf05))
* Remove fee_ticker and token_trading_volume fetcher modules ([#262](https://github.com/matter-labs/zksync-era/issues/262)) ([44f7179](https://github.com/matter-labs/zksync-era/commit/44f71794a66a32f565048636ea5b05aea190653a))
* **reorg_detector:** compare miniblock hashes for reorg detection ([#236](https://github.com/matter-labs/zksync-era/issues/236)) ([2c930b2](https://github.com/matter-labs/zksync-era/commit/2c930b2f53562cb63874c48c0112537d6efb1958))
* Rewrite server binary to use `vise` metrics ([#120](https://github.com/matter-labs/zksync-era/issues/120)) ([26ee1fb](https://github.com/matter-labs/zksync-era/commit/26ee1fbb16cbd7c4fad334cbc6804e7d779029b6))
* **types:** introduce state diff record type and compression ([#194](https://github.com/matter-labs/zksync-era/issues/194)) ([ccf753c](https://github.com/matter-labs/zksync-era/commit/ccf753c5e1befb58eb6bb3d9ce2b392d5de60bdd))
* **vm:** Improve tracer trait ([#121](https://github.com/matter-labs/zksync-era/issues/121)) ([ff60138](https://github.com/matter-labs/zksync-era/commit/ff601386686cdae0ab4f227203a006816e7a49a5))
* **vm:** Move all vm versions to the one crate ([#249](https://github.com/matter-labs/zksync-era/issues/249)) ([e3fb489](https://github.com/matter-labs/zksync-era/commit/e3fb4894d08aa98a84e64eaa95b51001055cf911))


### Bug Fixes

* **crypto:** update snark-vk to be used in server and update args for proof wrapping ([#240](https://github.com/matter-labs/zksync-era/issues/240)) ([4a5c54c](https://github.com/matter-labs/zksync-era/commit/4a5c54c48bbc100c29fa719c4b1dc3535743003d))
* **db:** Fix write stalls in RocksDB ([#250](https://github.com/matter-labs/zksync-era/issues/250)) ([650124c](https://github.com/matter-labs/zksync-era/commit/650124cfffc97b11e6bdce8fa7c5449fc9234991))
* **db:** Fix write stalls in RocksDB (again) ([#265](https://github.com/matter-labs/zksync-era/issues/265)) ([7b23ab0](https://github.com/matter-labs/zksync-era/commit/7b23ab0ba14cb6600ecf7e596a9e9536ffa5fda2))
* **db:** Fix write stalls in RocksDB (for real this time) ([#292](https://github.com/matter-labs/zksync-era/issues/292)) ([0f15919](https://github.com/matter-labs/zksync-era/commit/0f15919ccd229a141679f358088b1526e66b2d18))
* Fix `TxStage` string representation ([#255](https://github.com/matter-labs/zksync-era/issues/255)) ([246b5a0](https://github.com/matter-labs/zksync-era/commit/246b5a07435e2d48810126fbd259c12885b54127))
* fix typos ([#226](https://github.com/matter-labs/zksync-era/issues/226)) ([feb8a6c](https://github.com/matter-labs/zksync-era/commit/feb8a6c7053cc5e0202088f6a1f7644316e1ad05))
* **witness-generator:** Witness generator oracle with cached storage refunds ([#274](https://github.com/matter-labs/zksync-era/issues/274)) ([8928a41](https://github.com/matter-labs/zksync-era/commit/8928a4169faa3fd74d6816453aa192e1d0c3e4fe))

## [16.0.2](https://github.com/matter-labs/zksync-era/compare/core-v16.0.1...core-v16.0.2) (2023-10-12)

### Bug Fixes

* **API:** return correct v value for Legacy tx ([#154](https://github.com/matter-labs/zksync-era/issues/154)) ([ed502ea](https://github.com/matter-labs/zksync-era/commit/ed502ea9aff50627ae9620f1570579893cbf2722))
* **API:** U256 for chainId in api::Transaction struct ([#211](https://github.com/matter-labs/zksync-era/issues/211)) ([ca98a1c](https://github.com/matter-labs/zksync-era/commit/ca98a1c70c8482397215221b4e00bdb2edeccd84))
* **prover:** Fix statistic query ([#193](https://github.com/matter-labs/zksync-era/issues/193)) ([5499093](https://github.com/matter-labs/zksync-era/commit/54990933051632e505c76bd98b83462617cb725a))
* **state-keeper:** Add L2ToL1LogsCriterion ([#195](https://github.com/matter-labs/zksync-era/issues/195)) ([64459b2](https://github.com/matter-labs/zksync-era/commit/64459b2383a344b558ae648743c3f7d91c1b24c0))

## [16.0.0](https://github.com/matter-labs/zksync-era/compare/core-v15.1.1...core-v16.0.0) (2023-10-11)

### ⚠ BREAKING CHANGES

* **vm:** Update Refund model ([#181](https://github.com/matter-labs/zksync-era/issues/181))

### Features

* change chainId to u64 ([#167](https://github.com/matter-labs/zksync-era/issues/167)) ([f14bf68](https://github.com/matter-labs/zksync-era/commit/f14bf6851059a7add6677c89b3192e1b23cbf3c5))
* **merkle tree:** Provide Merkle proofs for tree entries and entry ranges ([#119](https://github.com/matter-labs/zksync-era/issues/119)) ([1e30d0b](https://github.com/matter-labs/zksync-era/commit/1e30d0ba8d243f41ad1e86e77d24848d64bd11e6))
* **storage:** save enum indices in RocksDB ([#162](https://github.com/matter-labs/zksync-era/issues/162)) ([bab099d](https://github.com/matter-labs/zksync-era/commit/bab099d83d9640c965bc02b32d90cce86a3f53cb))
* **vm:** Update Refund model ([#181](https://github.com/matter-labs/zksync-era/issues/181)) ([92b6f59](https://github.com/matter-labs/zksync-era/commit/92b6f5999b66666f01b89b5ff188d220139751a2))

### Bug Fixes

* **db:** drop constraint prover_jobs_fri_l1_batch_number_fkey ([#173](https://github.com/matter-labs/zksync-era/issues/173)) ([fa71650](https://github.com/matter-labs/zksync-era/commit/fa7165002884e7137b623feec3721cbbe3332a40))
* **vm:** Make execution status and stop reason public ([#169](https://github.com/matter-labs/zksync-era/issues/169)) ([f98c4fa](https://github.com/matter-labs/zksync-era/commit/f98c4fab0f10d190ceb2ae9bfa77929bf793a6ea))

## [15.1.1](https://github.com/matter-labs/zksync-era/compare/core-v15.1.0...core-v15.1.1) (2023-10-05)

### Bug Fixes

* use gauge instead histogram for replication lag metric ([#159](https://github.com/matter-labs/zksync-era/issues/159)) ([0d952d4](https://github.com/matter-labs/zksync-era/commit/0d952d43a021c2fbf18920da3e7d770a6309d990))

## [15.1.0](https://github.com/matter-labs/zksync-era/compare/core-v15.0.2...core-v15.1.0) (2023-10-03)

### Features

* Implement dynamic L2-to-L1 log tree depth ([#126](https://github.com/matter-labs/zksync-era/issues/126)) ([7dfbc5e](https://github.com/matter-labs/zksync-era/commit/7dfbc5eddab94cd24f96912e0d43ba36e1cf363f))
* **vm:** Introduce new way of returning from the tracer [#2569](https://github.com/matter-labs/zksync-2-dev/issues/2569) ([#116](https://github.com/matter-labs/zksync-era/issues/116)) ([cf44a49](https://github.com/matter-labs/zksync-era/commit/cf44a491a324199b4cf457d28658da44b6dafc61))
* **vm:** Restore system-constants-generator ([#115](https://github.com/matter-labs/zksync-era/issues/115)) ([5e61bdc](https://github.com/matter-labs/zksync-era/commit/5e61bdc75b2baa03004d4d3e801170c094766964))

## [15.0.1](https://github.com/matter-labs/zksync-2-dev/compare/core-v15.0.0...core-v15.0.1) (2023-09-27)

### Bug Fixes

* **vm:** Fix divergency of hashes for l2 block ([#2662](https://github.com/matter-labs/zksync-2-dev/issues/2662)) ([fb2e2ff](https://github.com/matter-labs/zksync-2-dev/commit/fb2e2ff5a79b7bbc8a1467f174fd6d26c3d11d36))

## [15.0.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v9.0.0...core-v15.0.0) (2023-09-26)

### Features

* Add replication lag checker to circuit breaker component ([#2620](https://github.com/matter-labs/zksync-2-dev/issues/2620)) ([a2b3395](https://github.com/matter-labs/zksync-2-dev/commit/a2b33950d884cbca1b1f7cc7b5a43ac3e38112dd))
* Rewrite libraries to use `vise` metrics ([#2616](https://github.com/matter-labs/zksync-2-dev/issues/2616)) ([d8cdbe9](https://github.com/matter-labs/zksync-2-dev/commit/d8cdbe9ad8ce40f55bbd8c788f3ca055a33989e6))

### Bug Fixes

* **crypto:** update compressor to pass universal setup file ([#2610](https://github.com/matter-labs/zksync-2-dev/issues/2610)) ([39ea81c](https://github.com/matter-labs/zksync-2-dev/commit/39ea81c360026d58222826d8d97bf909ff2c6326))
* **prover-fri:** move saving to GCS behind flag ([#2627](https://github.com/matter-labs/zksync-2-dev/issues/2627)) ([ed49420](https://github.com/matter-labs/zksync-2-dev/commit/ed49420fb782856541c632e64466ff4da8e05e81))
* **tracer:** Fixed a bug in calltracer that resulted in empty traces ([#2636](https://github.com/matter-labs/zksync-2-dev/issues/2636)) ([7983edc](https://github.com/matter-labs/zksync-2-dev/commit/7983edc9f65b1accf960c71d1d99033f3ba0e111))
* **vm:** next block assertion for the very first miniblock after upgrade ([#2655](https://github.com/matter-labs/zksync-2-dev/issues/2655)) ([7c00107](https://github.com/matter-labs/zksync-2-dev/commit/7c00107c0eb87092c1a5bab66b004c61d6917ac9))

## [9.0.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v8.7.0...core-v9.0.0) (2023-09-21)

### ⚠ BREAKING CHANGES

* update verification keys, protocol version 15 ([#2602](https://github.com/matter-labs/zksync-2-dev/issues/2602))

### Features

* **contracts:** Allow reading contracts code from other directories ([#2613](https://github.com/matter-labs/zksync-2-dev/issues/2613)) ([1481eb8](https://github.com/matter-labs/zksync-2-dev/commit/1481eb84cbac891586a41d6d9739ae343e3c1bb8))
* make data returned from the VM to have arbitrary length ([#2479](https://github.com/matter-labs/zksync-2-dev/issues/2479)) ([9251690](https://github.com/matter-labs/zksync-2-dev/commit/92516901cb2db61987554ddf0f8e080a15e7e72e))
* **prover-fri:** added picked-by column in prover fri related tables ([#2600](https://github.com/matter-labs/zksync-2-dev/issues/2600)) ([9e604ab](https://github.com/matter-labs/zksync-2-dev/commit/9e604abf3bae11b6f583f2abd39c07a85dc20f0a))
* update verification keys, protocol version 15 ([#2602](https://github.com/matter-labs/zksync-2-dev/issues/2602)) ([2fff59b](https://github.com/matter-labs/zksync-2-dev/commit/2fff59bab00849996864b68e932739135337ebd7))
* **vlog:** Rework the observability configuration subsystem ([#2608](https://github.com/matter-labs/zksync-2-dev/issues/2608)) ([377f0c5](https://github.com/matter-labs/zksync-2-dev/commit/377f0c5f734c979bc990b429dff0971466872e71))
* **vm:** MultiVM tracer support ([#2601](https://github.com/matter-labs/zksync-2-dev/issues/2601)) ([4a7467b](https://github.com/matter-labs/zksync-2-dev/commit/4a7467b1b1556bfd795792dbe280bcf28c93a58f))

## [8.7.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v8.6.0...core-v8.7.0) (2023-09-19)

### Features

* Rework metrics approach ([#2387](https://github.com/matter-labs/zksync-2-dev/issues/2387)) ([4855546](https://github.com/matter-labs/zksync-2-dev/commit/48555465d32f8524f6cf488859e8ae8259ecf5da))

### Bug Fixes

* **db:** Vacuum `storage_logs` table along with removing duplicate rows ([#2583](https://github.com/matter-labs/zksync-2-dev/issues/2583)) ([84f472d](https://github.com/matter-labs/zksync-2-dev/commit/84f472deb14a92bd2b90f8160c9316d21b646ff4))
* **prover_fri:** drop not null constraint from proof_compression_jobs_fri fri_proof_blob_url column ([#2590](https://github.com/matter-labs/zksync-2-dev/issues/2590)) ([5e41fee](https://github.com/matter-labs/zksync-2-dev/commit/5e41fee69c869c53d999c3ee53e3e7dd6735e603))
* **state-keeper:** deduplication logic ([#2597](https://github.com/matter-labs/zksync-2-dev/issues/2597)) ([7122a2b](https://github.com/matter-labs/zksync-2-dev/commit/7122a2b0cc33a96a4c117186437db8e290388356))
* **storage:** Ignore non-committed factory deps in RocksDB ([#2585](https://github.com/matter-labs/zksync-2-dev/issues/2585)) ([b3da824](https://github.com/matter-labs/zksync-2-dev/commit/b3da82483639728bd899fb9388b9b2868cb28159))
* **vm:** Handle near call gas correctly ([#2587](https://github.com/matter-labs/zksync-2-dev/issues/2587)) ([c925259](https://github.com/matter-labs/zksync-2-dev/commit/c9252597ce330d0c982365bb703c373191d03506))

## [8.6.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v8.5.0...core-v8.6.0) (2023-09-15)

### Features

* **prover-fri:** insert missing protocol version in FRI witness-gen table ([#2577](https://github.com/matter-labs/zksync-2-dev/issues/2577)) ([b9af6a5](https://github.com/matter-labs/zksync-2-dev/commit/b9af6a5784b0e6538bd542830593d16f3caf5fe5))
* **prover-server-api:** Add SkippedProofGeneration in SubmitProofRequest ([#2575](https://github.com/matter-labs/zksync-2-dev/issues/2575)) ([9c2653e](https://github.com/matter-labs/zksync-2-dev/commit/9c2653e5bc0e56b2906e9d25be3cb2887ad7d35d))

## [8.5.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v8.4.0...core-v8.5.0) (2023-09-15)

### Features

* **API:** enable request translator for ws api ([#2568](https://github.com/matter-labs/zksync-2-dev/issues/2568)) ([ccb6cad](https://github.com/matter-labs/zksync-2-dev/commit/ccb6cad57c9ba0ca58114701c256dbc44a457459))
* Use tracing directly instead of vlog macros ([#2566](https://github.com/matter-labs/zksync-2-dev/issues/2566)) ([53d53af](https://github.com/matter-labs/zksync-2-dev/commit/53d53afc9157214fb911aa0934a97f8b5103e1ec))
* **witness-generator:** Get wit inputs from MerklePath instead of SK ([#2559](https://github.com/matter-labs/zksync-2-dev/issues/2559)) ([da1c2fa](https://github.com/matter-labs/zksync-2-dev/commit/da1c2fa2b043bc4e31075a0454dc0e09937c93ad))

### Bug Fixes

* Do not automatically emit sentry events on vlog::error ([#2560](https://github.com/matter-labs/zksync-2-dev/issues/2560)) ([aebcd86](https://github.com/matter-labs/zksync-2-dev/commit/aebcd8634a0984aaf357b03d925932807848b6b8))
* filter_near_call performance ([#2523](https://github.com/matter-labs/zksync-2-dev/issues/2523)) ([eccb06b](https://github.com/matter-labs/zksync-2-dev/commit/eccb06b649621b6866476c6c5a95545e3359d79b))

## [8.4.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v8.3.1...core-v8.4.0) (2023-09-14)

### Features

* **API:** new translator for virtual blocks for zks_getLogs endpoint ([#2505](https://github.com/matter-labs/zksync-2-dev/issues/2505)) ([35b0553](https://github.com/matter-labs/zksync-2-dev/commit/35b05537dc8fecf11be477bd156da332d75b1320))
* **contract-verifier:** Add zkvyper v1.3.11 ([#2554](https://github.com/matter-labs/zksync-2-dev/issues/2554)) ([711c5db](https://github.com/matter-labs/zksync-2-dev/commit/711c5db4bd48e9b4b166256e8c9554ef0e54fad8))
* **contract-verifier:** Support verification for zksolc v1.3.16  ([#2546](https://github.com/matter-labs/zksync-2-dev/issues/2546)) ([adea3ef](https://github.com/matter-labs/zksync-2-dev/commit/adea3efd39099ef9599e24d47de6c7cffe6b0287))
* Decrease crate versions back to 0.1.0 ([#2528](https://github.com/matter-labs/zksync-2-dev/issues/2528)) ([adb7614](https://github.com/matter-labs/zksync-2-dev/commit/adb76142882dde197cd64b1aaaffb01906427054))
* **prover-fri:** Restrict prover to pick jobs for which they have vk's ([#2541](https://github.com/matter-labs/zksync-2-dev/issues/2541)) ([cedba03](https://github.com/matter-labs/zksync-2-dev/commit/cedba03ea66fc0da479e60d5ca30d8f67e32358a))
* **vm:** Make execute interface more obvious ([#2536](https://github.com/matter-labs/zksync-2-dev/issues/2536)) ([4cb18cb](https://github.com/matter-labs/zksync-2-dev/commit/4cb18cb06e87628ad122fc9857c789d1411a7f77))

### Bug Fixes

* **multi_vm:** Fix executing eth_call for old vm version ([#2558](https://github.com/matter-labs/zksync-2-dev/issues/2558)) ([0f3b990](https://github.com/matter-labs/zksync-2-dev/commit/0f3b990735caab8c905a9b51256608f4f7614ff1))
* **vm:** Add trait for convinient usage of tracers ([#2550](https://github.com/matter-labs/zksync-2-dev/issues/2550)) ([bc2ed11](https://github.com/matter-labs/zksync-2-dev/commit/bc2ed1188cf545cfae1266302f1d5c2ef1feab0f))

### Performance Improvements

* **state-keeper:** only persist unique storage logs per miniblock ([#1793](https://github.com/matter-labs/zksync-2-dev/issues/1793)) ([d0ef78b](https://github.com/matter-labs/zksync-2-dev/commit/d0ef78b294c4e29692170c9b244414c7a5b9aa6c))

## [8.3.1](https://github.com/matter-labs/zksync-2-dev/compare/core-v8.3.0...core-v8.3.1) (2023-09-12)

### Bug Fixes

* **house-keeper:** remove extra ! from status column ([#2539](https://github.com/matter-labs/zksync-2-dev/issues/2539)) ([583dadb](https://github.com/matter-labs/zksync-2-dev/commit/583dadb91885e664b79b299fc2cd84d5077cc2cd))

## [8.3.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v8.2.1...core-v8.3.0) (2023-09-11)

### Features

* **api:** Report some metrics more often ([#2519](https://github.com/matter-labs/zksync-2-dev/issues/2519)) ([eede188](https://github.com/matter-labs/zksync-2-dev/commit/eede188f6160fa383496c7c8ae8409c68bc54114))
* **housekeeper:** add proof compressor retry and queued jobs reporting ([#2526](https://github.com/matter-labs/zksync-2-dev/issues/2526)) ([4321545](https://github.com/matter-labs/zksync-2-dev/commit/432154527dc85a17fc83c9e866b772a8d6f47673))
* **metrics:** add more metrics to dry run ([#2529](https://github.com/matter-labs/zksync-2-dev/issues/2529)) ([0abdbb8](https://github.com/matter-labs/zksync-2-dev/commit/0abdbb8bd3229d2907f1f82493b2cf8e7a6a3254))
* **vm:** New vm intregration ([#2198](https://github.com/matter-labs/zksync-2-dev/issues/2198)) ([f5e7e7a](https://github.com/matter-labs/zksync-2-dev/commit/f5e7e7a6fa81ab46289016f57a6123ffec83bcf6))

### Bug Fixes

* **vm:** Add bootloader tip execution mode ([#2535](https://github.com/matter-labs/zksync-2-dev/issues/2535)) ([2d64a3a](https://github.com/matter-labs/zksync-2-dev/commit/2d64a3a0947d131a4f9baf57afd1e26bccbc7b81))

### Performance Improvements

* **db:** Support previous blocks in VM values cache ([#2474](https://github.com/matter-labs/zksync-2-dev/issues/2474)) ([5eb32c5](https://github.com/matter-labs/zksync-2-dev/commit/5eb32c588b4ae1c85ef8fc95f70e03921eb19625))

## [8.2.1](https://github.com/matter-labs/zksync-2-dev/compare/core-v8.2.0...core-v8.2.1) (2023-09-07)

### Bug Fixes

* **api:** miniblock_hash loading ([#2513](https://github.com/matter-labs/zksync-2-dev/issues/2513)) ([c553dae](https://github.com/matter-labs/zksync-2-dev/commit/c553daeca49a943a323cefa2017808e6c06728e9))

## [8.2.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v8.1.1...core-v8.2.0) (2023-09-06)

### Features

* **prover-fri-compressor:** Create a dedicated component for FRI proof conversion ([#2501](https://github.com/matter-labs/zksync-2-dev/issues/2501)) ([cd43aa7](https://github.com/matter-labs/zksync-2-dev/commit/cd43aa73095bf97b54c9fbcc9934128cc29506c2))
* **prover-fri:** use separate object store config for FRI prover components ([#2494](https://github.com/matter-labs/zksync-2-dev/issues/2494)) ([7f2537f](https://github.com/matter-labs/zksync-2-dev/commit/7f2537fc987c55b6efec925506478d665d20c0c4))
* **witness-generator:** Add Witness Storage, later used in Wit Gen ([#2509](https://github.com/matter-labs/zksync-2-dev/issues/2509)) ([c78ddf3](https://github.com/matter-labs/zksync-2-dev/commit/c78ddf33e7de929fd369472e8892a2a83f2b0ac2))
* **witness-generator:** Basic Wit Gen uses multiple Storage backends ([#2510](https://github.com/matter-labs/zksync-2-dev/issues/2510)) ([1dc1f1c](https://github.com/matter-labs/zksync-2-dev/commit/1dc1f1c4e65f0f49c63a654c64596dc085911791))

### Bug Fixes

* **api:** Use multivm bootloader in debug ([#2504](https://github.com/matter-labs/zksync-2-dev/issues/2504)) ([ae2a357](https://github.com/matter-labs/zksync-2-dev/commit/ae2a357f38a57498ef527f2ccbce1d32b9b3f7b5))
* **witnes-gen:** Fix getting bootloader memory ([#2507](https://github.com/matter-labs/zksync-2-dev/issues/2507)) ([bb8f894](https://github.com/matter-labs/zksync-2-dev/commit/bb8f89472432e9b11c538881f27dda8afdf46a4f))
* **witness-generator:** Add Data Source config for Basic Witness Gens ([#2502](https://github.com/matter-labs/zksync-2-dev/issues/2502)) ([9126597](https://github.com/matter-labs/zksync-2-dev/commit/91265973d0eabb34e056277cd2aa730c05a9c06f))

## [8.1.1](https://github.com/matter-labs/zksync-2-dev/compare/core-v8.1.0...core-v8.1.1) (2023-09-06)

### Performance Improvements

* **db:** Add `miniblocks_pending_batch` DB index ([#2496](https://github.com/matter-labs/zksync-2-dev/issues/2496)) ([dc20057](https://github.com/matter-labs/zksync-2-dev/commit/dc200570f62bb52de5fa798a353f08fae0a3fc71))

## [8.1.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v8.0.0...core-v8.1.0) (2023-09-05)

### Features

* **genesis:** make it possible to create genesis block with given protocol version ([#2471](https://github.com/matter-labs/zksync-2-dev/issues/2471)) ([430de1f](https://github.com/matter-labs/zksync-2-dev/commit/430de1f4ed59e9bc1eeb029dacdf88684c34a1ad))

### Bug Fixes

* **api:** Zeroes in eth_feeHistory ([#2490](https://github.com/matter-labs/zksync-2-dev/issues/2490)) ([67cd433](https://github.com/matter-labs/zksync-2-dev/commit/67cd433f57e01fdf94da7461c6b76a6948815212))
* **db:** Fix `get_expected_l1_batch_timestamp` query ([#2492](https://github.com/matter-labs/zksync-2-dev/issues/2492)) ([660ae98](https://github.com/matter-labs/zksync-2-dev/commit/660ae98d34b48f8c97c50c8c7988049e50d90297))

## [8.0.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v7.2.0...core-v8.0.0) (2023-09-05)

### ⚠ BREAKING CHANGES

* Fix a bug for the ending of the upgrade ([#2478](https://github.com/matter-labs/zksync-2-dev/issues/2478))

### Features

* **prover-fri:** Add protocol version for FRI prover related tables ([#2458](https://github.com/matter-labs/zksync-2-dev/issues/2458)) ([784a52b](https://github.com/matter-labs/zksync-2-dev/commit/784a52bc2d2fa784fe82cc10df1d39895255ade5))

### Bug Fixes

* **api:** Use MultiVM in API ([#2476](https://github.com/matter-labs/zksync-2-dev/issues/2476)) ([683582d](https://github.com/matter-labs/zksync-2-dev/commit/683582dab2fb26d09a5e183ac9e4d0d9e61286e4))
* Fix a bug for the ending of the upgrade ([#2478](https://github.com/matter-labs/zksync-2-dev/issues/2478)) ([5fbad97](https://github.com/matter-labs/zksync-2-dev/commit/5fbad971af10240feaa8da3062dcf7c98aca3f02))

## [7.2.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v7.1.0...core-v7.2.0) (2023-09-01)

### Features

* **api:** Implement rate-limiting for WebSocket jsonrpc backend ([#2468](https://github.com/matter-labs/zksync-2-dev/issues/2468)) ([db86c11](https://github.com/matter-labs/zksync-2-dev/commit/db86c11caf1c63de6fa6be5031636d95125fa6c9))
* **api:** make gas per pubdata field  to be not optional in SDK and API for transaction details endpoint ([#2431](https://github.com/matter-labs/zksync-2-dev/issues/2431)) ([4c3636a](https://github.com/matter-labs/zksync-2-dev/commit/4c3636a33af345046d5a78ee8b65b1a4d1066e98))

### Bug Fixes

* debug and fix local node setup ([#2408](https://github.com/matter-labs/zksync-2-dev/issues/2408)) ([4f3a9e6](https://github.com/matter-labs/zksync-2-dev/commit/4f3a9e695c868a181c7ecd1dbbd647b1a2a74a4f))

## [7.1.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v7.0.0...core-v7.1.0) (2023-09-01)

### Features

* **api:** Support batch request size limiting in jsonrpc ([#2461](https://github.com/matter-labs/zksync-2-dev/issues/2461)) ([287d360](https://github.com/matter-labs/zksync-2-dev/commit/287d360d03914adf3e15c115470709abadf4585c))
* **prover-gateway:** integrate snark wrapper to transform FRI proof to old ([#2413](https://github.com/matter-labs/zksync-2-dev/issues/2413)) ([60bb26b](https://github.com/matter-labs/zksync-2-dev/commit/60bb26bdc31f13f2b9253b245e848951e8e6e501))
* **witness_generator:** Add flagged upload path on state keeper ([#2448](https://github.com/matter-labs/zksync-2-dev/issues/2448)) ([10b78cb](https://github.com/matter-labs/zksync-2-dev/commit/10b78cb6b31e2bfe6c84d6cb76f3228003e44ae7))

### Bug Fixes

* **en:** Set correct hashes for old blocks ([#2463](https://github.com/matter-labs/zksync-2-dev/issues/2463)) ([aa5d0b1](https://github.com/matter-labs/zksync-2-dev/commit/aa5d0b126b8e68a8f4e8da874611165acf145a73))
* **en:** Set correct version for upgrade  ([#2464](https://github.com/matter-labs/zksync-2-dev/issues/2464)) ([44e5f32](https://github.com/matter-labs/zksync-2-dev/commit/44e5f32b910917f1661fbd0139f2ba35cbc9eca0))
* **state-keeper:** Calculate miniblock hash based on protocol version ([#2462](https://github.com/matter-labs/zksync-2-dev/issues/2462)) ([01bee1d](https://github.com/matter-labs/zksync-2-dev/commit/01bee1dcd1c398374253bb8b40ab9385d9fd8547))

## [7.0.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v6.2.0...core-v7.0.0) (2023-08-30)

### ⚠ BREAKING CHANGES

* **vm:** replace L1 batch number and timestamp with  miniblock number and timestamp ([#1975](https://github.com/matter-labs/zksync-2-dev/issues/1975))

### Features

* **prover-server-split:** insert batches to be proven on proof_generation_details table ([#2417](https://github.com/matter-labs/zksync-2-dev/issues/2417)) ([504c37f](https://github.com/matter-labs/zksync-2-dev/commit/504c37fc3aeab951b335b574508289fef33e1700))
* **vm:** replace L1 batch number and timestamp with  miniblock number and timestamp ([#1975](https://github.com/matter-labs/zksync-2-dev/issues/1975)) ([6814c7e](https://github.com/matter-labs/zksync-2-dev/commit/6814c7eafa0c6f29607e81acfffc70ac1fa5fa96))
* **witness_generator:** Add upload_witness_inputs_to_gcs flag ([#2444](https://github.com/matter-labs/zksync-2-dev/issues/2444)) ([9f0b87e](https://github.com/matter-labs/zksync-2-dev/commit/9f0b87ef1b33defa71ef98ff2cd5fb66f6537837))

### Bug Fixes

* **api:** fix eth_call for old blocks ([#2440](https://github.com/matter-labs/zksync-2-dev/issues/2440)) ([19bba44](https://github.com/matter-labs/zksync-2-dev/commit/19bba4413f8f4197e2178e409106eecf12089d08))
* **en:** Allow executed batch reversion ([#2442](https://github.com/matter-labs/zksync-2-dev/issues/2442)) ([a47b72d](https://github.com/matter-labs/zksync-2-dev/commit/a47b72db82527409e224467bfb07ca642426385f))
* **en:** Insert protocol version for pending batch ([#2450](https://github.com/matter-labs/zksync-2-dev/issues/2450)) ([dd0792e](https://github.com/matter-labs/zksync-2-dev/commit/dd0792ea200f255c9f9f3e55924cb7caf0452b89))
* **en:** Set protocol version for pending blocks in EN ([#2443](https://github.com/matter-labs/zksync-2-dev/issues/2443)) ([7464395](https://github.com/matter-labs/zksync-2-dev/commit/746439527be10ad513607094953ba7523316b843))

## [6.2.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v6.1.0...core-v6.2.0) (2023-08-28)

### Features

* **hyperchain:** hyperchain script ([#2410](https://github.com/matter-labs/zksync-2-dev/issues/2410)) ([52b63d3](https://github.com/matter-labs/zksync-2-dev/commit/52b63d348f634a4434d21aa2b1955e55859556d6))

### Bug Fixes

* **api:** Revert `ProtocolVersionId` serialization ([#2425](https://github.com/matter-labs/zksync-2-dev/issues/2425)) ([e2eee91](https://github.com/matter-labs/zksync-2-dev/commit/e2eee9121961fae234e8228c35ef4265b1328cf1))
* **db:** Fix prover_jobs indices ([#2416](https://github.com/matter-labs/zksync-2-dev/issues/2416)) ([4104e7e](https://github.com/matter-labs/zksync-2-dev/commit/4104e7e1e3bd5dfc3c46827e45527dc9a40b7757))

## [6.1.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v6.0.0...core-v6.1.0) (2023-08-25)

### Features

* Add logging for health check changes ([#2401](https://github.com/matter-labs/zksync-2-dev/issues/2401)) ([3be83e5](https://github.com/matter-labs/zksync-2-dev/commit/3be83e5481f12745579a1d7e6c42d2fa27a0a566))
* **api:** Measure difference from last miniblock for JSON-RPC APIs ([#2370](https://github.com/matter-labs/zksync-2-dev/issues/2370)) ([c706927](https://github.com/matter-labs/zksync-2-dev/commit/c706927d233935c20ac074968a82b10449eb4dff))
* **eth-sender:** add support for loading new proofs from GCS ([#2392](https://github.com/matter-labs/zksync-2-dev/issues/2392)) ([54f6f53](https://github.com/matter-labs/zksync-2-dev/commit/54f6f53953ddd20c19a8d6de092700de2835ad33))
* glue VM version, protocol version and EN ([#2411](https://github.com/matter-labs/zksync-2-dev/issues/2411)) ([c3768fc](https://github.com/matter-labs/zksync-2-dev/commit/c3768fc028afbd4b0ed8d005430a0d3a1ede72c1))
* **prover-server-split:** consume API for fetching proof gen data and submitting proofs ([#2365](https://github.com/matter-labs/zksync-2-dev/issues/2365)) ([6e99471](https://github.com/matter-labs/zksync-2-dev/commit/6e994717086941fd2538fced7c32b4bb5eeb4eac))

### Bug Fixes

* **contract-verifier:** No panic when 2 parallel cvs insert same key ([#2396](https://github.com/matter-labs/zksync-2-dev/issues/2396)) ([f0d9081](https://github.com/matter-labs/zksync-2-dev/commit/f0d90815cc2a27b27f84c0aa434a16fff59f356f))
* **en:** do not save protocol version for miniblocks/batches in EN ([#2403](https://github.com/matter-labs/zksync-2-dev/issues/2403)) ([75bd867](https://github.com/matter-labs/zksync-2-dev/commit/75bd867079830d4519c1e20c4a53af749ecc325d))
* **en:** Save system tx to en ([#2402](https://github.com/matter-labs/zksync-2-dev/issues/2402)) ([0bb50a5](https://github.com/matter-labs/zksync-2-dev/commit/0bb50a5b31d5e0960ed3dec84b21170d6ccfddad))
* save protocol versions in prover DB ([#2384](https://github.com/matter-labs/zksync-2-dev/issues/2384)) ([0fc2195](https://github.com/matter-labs/zksync-2-dev/commit/0fc21952e630f56df582b79e071998564132f67f))

## [6.0.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v5.28.1...core-v6.0.0) (2023-08-18)

### ⚠ BREAKING CHANGES

* new upgrade system ([#1784](https://github.com/matter-labs/zksync-2-dev/issues/1784))

### Features

* Add shadow_storage to enable shadow state read ([#2366](https://github.com/matter-labs/zksync-2-dev/issues/2366)) ([3269d82](https://github.com/matter-labs/zksync-2-dev/commit/3269d82de20b205feec4e4056dad51cd28e14f8f))
* **db:** Instrument key queries in DAL ([#2318](https://github.com/matter-labs/zksync-2-dev/issues/2318)) ([eb08ed6](https://github.com/matter-labs/zksync-2-dev/commit/eb08ed69db8655dd4e0d485597568c6e4e01e5bf))
* new upgrade system ([#1784](https://github.com/matter-labs/zksync-2-dev/issues/1784)) ([469a4c3](https://github.com/matter-labs/zksync-2-dev/commit/469a4c332a4f02b5a642b2951fd00228c9317f59))
* **prover-fri:** Add socket listener to receive witness vector over network ([#2367](https://github.com/matter-labs/zksync-2-dev/issues/2367)) ([19c9d89](https://github.com/matter-labs/zksync-2-dev/commit/19c9d89613a5d3ab5e55d9224a512a8952aa3689))
* **prover-server-split:** expose API from server for requesting proof gen data and submitting proof ([#2292](https://github.com/matter-labs/zksync-2-dev/issues/2292)) ([401d0ab](https://github.com/matter-labs/zksync-2-dev/commit/401d0ab51bfce89203fd82b5f8d1a6865f6d19b0))
* **witness-vector-generator:** Perform circuit synthesis on external machine ([#2351](https://github.com/matter-labs/zksync-2-dev/issues/2351)) ([6839f3f](https://github.com/matter-labs/zksync-2-dev/commit/6839f3fbfe472fb2fd492a9648bb97d1654bbb3b))

### Bug Fixes

* Return old paths for contract-verification API ([#2356](https://github.com/matter-labs/zksync-2-dev/issues/2356)) ([605a3ac](https://github.com/matter-labs/zksync-2-dev/commit/605a3ac0951241a892ee8f0832b69336299da6c6))

### Performance Improvements

* **db:** Optimize loading L1 batch header ([#2343](https://github.com/matter-labs/zksync-2-dev/issues/2343)) ([1274469](https://github.com/matter-labs/zksync-2-dev/commit/1274469b0618582d2027bc7b8dbda779486a553d))

## [5.28.1](https://github.com/matter-labs/zksync-2-dev/compare/core-v5.28.0...core-v5.28.1) (2023-08-10)

### Bug Fixes

* **api:** fix typo when setting `max_response_body_size` ([#2341](https://github.com/matter-labs/zksync-2-dev/issues/2341)) ([540da7f](https://github.com/matter-labs/zksync-2-dev/commit/540da7f16e745e0288a8877891c1f80d6d62bc00))

## [5.28.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v5.27.0...core-v5.28.0) (2023-08-10)

### Features

* **api:** add `max_response_body_size` to config ([#2294](https://github.com/matter-labs/zksync-2-dev/issues/2294)) ([a29a71a](https://github.com/matter-labs/zksync-2-dev/commit/a29a71a81f04672c8dbae7e9aac760b70dbca8f0))
* **db:** Configure statement timeout for Postgres ([#2317](https://github.com/matter-labs/zksync-2-dev/issues/2317)) ([afdbb6b](https://github.com/matter-labs/zksync-2-dev/commit/afdbb6b94d9e43b9659ff5d3428f2d9a7827b29f))
* **en:** Add support for debug namespace in EN ([#2295](https://github.com/matter-labs/zksync-2-dev/issues/2295)) ([ebcc6e9](https://github.com/matter-labs/zksync-2-dev/commit/ebcc6e9ac387b85e44795f6d35edb4b0a6175de2))
* **house-keeper:** refactor periodic job to be reusable by adding in lib ([#2333](https://github.com/matter-labs/zksync-2-dev/issues/2333)) ([ad72a16](https://github.com/matter-labs/zksync-2-dev/commit/ad72a1691b661b2b4eeaefd29375a8987b485715))
* **hyperchain:** hyperchain wizard ([#2259](https://github.com/matter-labs/zksync-2-dev/issues/2259)) ([34c5b54](https://github.com/matter-labs/zksync-2-dev/commit/34c5b542d6436930a6068c4d08562804205154a9))
* **prover-fri:** Add concurrent circuit synthesis for FRI GPU prover ([#2326](https://github.com/matter-labs/zksync-2-dev/issues/2326)) ([aef3491](https://github.com/matter-labs/zksync-2-dev/commit/aef3491cd6af01840dd4fe5b7e530028916ffa8f))
* **state-keeper:** enforce different timestamps for miniblocks ([#2280](https://github.com/matter-labs/zksync-2-dev/issues/2280)) ([f87944e](https://github.com/matter-labs/zksync-2-dev/commit/f87944e72526112454934a61c71475b5a6fde22e))

### Bug Fixes

* **db:** Fix storage caches initialization ([#2339](https://github.com/matter-labs/zksync-2-dev/issues/2339)) ([ec8c822](https://github.com/matter-labs/zksync-2-dev/commit/ec8c8229ecd9f2a0f96f15a03929ede5453b6b09))
* **prover:** Kill prover process for edge-case in crypto thread code ([#2334](https://github.com/matter-labs/zksync-2-dev/issues/2334)) ([f2b5e1a](https://github.com/matter-labs/zksync-2-dev/commit/f2b5e1a2fcbe3053e372f15992e592bc0c32a88f))
* **state-keeper:** Order by number in `SELECT timestamp ...` query ([#2331](https://github.com/matter-labs/zksync-2-dev/issues/2331)) ([513e36e](https://github.com/matter-labs/zksync-2-dev/commit/513e36ec6aace545004b964861d080b308e7a98b))

### Performance Improvements

* **merkle tree:** Allow configuring multi-get chunk size ([#2332](https://github.com/matter-labs/zksync-2-dev/issues/2332)) ([0633911](https://github.com/matter-labs/zksync-2-dev/commit/06339117a36060bb31b6afb3933e12625c943e0b))
* **merkle tree:** Parallelize loading data and updating tree ([#2327](https://github.com/matter-labs/zksync-2-dev/issues/2327)) ([1edd6ee](https://github.com/matter-labs/zksync-2-dev/commit/1edd6eee62b112d3f3d1b790df01acd04be1eeef))
* **merkle tree:** Use batched multi-get for RocksDB ([#2304](https://github.com/matter-labs/zksync-2-dev/issues/2304)) ([df22946](https://github.com/matter-labs/zksync-2-dev/commit/df22946743ac56dbe86c5875a1e35345bfcd1f09))

## [5.27.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v5.26.0...core-v5.27.0) (2023-08-04)

### Features

* **merkle tree:** Switch sync mode dynamically ([#2274](https://github.com/matter-labs/zksync-2-dev/issues/2274)) ([e2e2d98](https://github.com/matter-labs/zksync-2-dev/commit/e2e2d98e849d6d1b73c8f6c0dd32d9a5aed0ab42))

### Bug Fixes

* **migrations:** Add If Exists Clause to Migration ([#2285](https://github.com/matter-labs/zksync-2-dev/issues/2285)) ([1273f42](https://github.com/matter-labs/zksync-2-dev/commit/1273f4284f6fa02b1623a90145bf191bad5ca93f))
* **prover:** Panics in `send_report` will make provers crash ([#2273](https://github.com/matter-labs/zksync-2-dev/issues/2273)) ([85974d3](https://github.com/matter-labs/zksync-2-dev/commit/85974d3f9482307e0dbad0ec179e80886dafa42e))

### Performance Improvements

* **db:** Cache latest state entries for VM ([#2258](https://github.com/matter-labs/zksync-2-dev/issues/2258)) ([f05f757](https://github.com/matter-labs/zksync-2-dev/commit/f05f757a942e1e67a24022f3b5fd054ae53b35dc))
* **merkle tree:** Optimize loading data for tree some more ([#2281](https://github.com/matter-labs/zksync-2-dev/issues/2281)) ([58757e3](https://github.com/matter-labs/zksync-2-dev/commit/58757e359420fb85da2db9396661c6e2d65d7a1f))

### Reverts

* **migrations:** Add If Exists Clause to Migration ([#2285](https://github.com/matter-labs/zksync-2-dev/issues/2285)) ([#2301](https://github.com/matter-labs/zksync-2-dev/issues/2301)) ([517b2e0](https://github.com/matter-labs/zksync-2-dev/commit/517b2e0a9ce0a4cdaa09ff05cdef4aae761d1bcb))

## [5.26.0](https://github.com/matter-labs/zksync-2-dev/compare/core-v5.25.0...core-v5.26.0) (2023-08-01)

### Features

* **api:** Rewrite healthcheck server using `axum` ([#2241](https://github.com/matter-labs/zksync-2-dev/issues/2241)) ([5854c7f](https://github.com/matter-labs/zksync-2-dev/commit/5854c7ff71a25b291a3aa3bfe5455d0b5799f227))
* **api:** Support setting maximum batch request size ([#2252](https://github.com/matter-labs/zksync-2-dev/issues/2252)) ([2cf24fd](https://github.com/matter-labs/zksync-2-dev/commit/2cf24fd0230ad83dc3839ca017fc5571603aab69))
* **eth-sender:** Use Multicall for getting base system contracts hashes ([#2196](https://github.com/matter-labs/zksync-2-dev/issues/2196)) ([8d3e1b6](https://github.com/matter-labs/zksync-2-dev/commit/8d3e1b6308f6a0ec2142b81b5c319390344ea8df))
* **prover-fri:** Added vk commitment generator in CI ([#2265](https://github.com/matter-labs/zksync-2-dev/issues/2265)) ([8ad75e0](https://github.com/matter-labs/zksync-2-dev/commit/8ad75e04b0a49dee34c6fa7e3b81a21392afa186))
* **state-keeper:** save initial writes indices in state keeper ([#2127](https://github.com/matter-labs/zksync-2-dev/issues/2127)) ([3a8790c](https://github.com/matter-labs/zksync-2-dev/commit/3a8790c005f8ae8217a461fcfb11d913eb48692b))
* Update RockDB bindings ([#2208](https://github.com/matter-labs/zksync-2-dev/issues/2208)) ([211f548](https://github.com/matter-labs/zksync-2-dev/commit/211f548fa9945b7ed5328026e526cd72c09f6a94))

### Bug Fixes

* **api:** Fix bytes deserialization by bumping web3 crate version  ([#2240](https://github.com/matter-labs/zksync-2-dev/issues/2240)) ([59ef24a](https://github.com/matter-labs/zksync-2-dev/commit/59ef24afa6ceddf506a9ac7c4b1e9fc292311095))
* **api:** underflow in `fee_history_impl` ([#2242](https://github.com/matter-labs/zksync-2-dev/issues/2242)) ([87c97cb](https://github.com/matter-labs/zksync-2-dev/commit/87c97cbdf40bfad8bbd1e01143dab27cc2c546f2))
* **db:** `transactions` table deadlock ([#2267](https://github.com/matter-labs/zksync-2-dev/issues/2267)) ([1082267](https://github.com/matter-labs/zksync-2-dev/commit/1082267f5bbe097ccf27ea01d2c77bd43da4268e))
* **merkle tree:** Brush up tree-related configuration ([#2266](https://github.com/matter-labs/zksync-2-dev/issues/2266)) ([18071c2](https://github.com/matter-labs/zksync-2-dev/commit/18071c240584fed009714f6a7d2b9560a6f6df67))
* **merkle tree:** Make tree creation async in metadata calculator ([#2270](https://github.com/matter-labs/zksync-2-dev/issues/2270)) ([23b2fac](https://github.com/matter-labs/zksync-2-dev/commit/23b2fac8058d08448d1dc669d18d0c77b17167ae))
* Use replica for slot_index_consistency_checker.rs ([#2256](https://github.com/matter-labs/zksync-2-dev/issues/2256)) ([15b3f5d](https://github.com/matter-labs/zksync-2-dev/commit/15b3f5de09acaa6d6608e51e1d6327a12cc53bbd))

### Performance Improvements

* **db:** Cache initial writes info for VM ([#2221](https://github.com/matter-labs/zksync-2-dev/issues/2221)) ([22735ae](https://github.com/matter-labs/zksync-2-dev/commit/22735ae6c58a8c002a9ddd539649918547d48d1a))
* Various optimizations ([#2251](https://github.com/matter-labs/zksync-2-dev/issues/2251)) ([817a982](https://github.com/matter-labs/zksync-2-dev/commit/817a9827d7004c055e5966e0f8ad1a4d51502721))

## [5.25.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.24.0...v5.25.0) (2023-07-25)

### Features

* **api:** Add metrics for requests with block height ([#2206](https://github.com/matter-labs/zksync-2-dev/issues/2206)) ([7be59cb](https://github.com/matter-labs/zksync-2-dev/commit/7be59cb8ffa375bad1b97146d966860149b9d767))

## [5.24.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.23.0...v5.24.0) (2023-07-24)

### Features

* **api:** Bump jsonrpsee version ([#2219](https://github.com/matter-labs/zksync-2-dev/issues/2219)) ([c5ed6bc](https://github.com/matter-labs/zksync-2-dev/commit/c5ed6bccfcdd94330bb40eef04ea77f66c13a735))
* **external node:** MultiVM ([#1833](https://github.com/matter-labs/zksync-2-dev/issues/1833)) ([0065e8e](https://github.com/matter-labs/zksync-2-dev/commit/0065e8e3f6846486be5d8a79f3b080a269ee632f))

### Bug Fixes

* **merkle tree:** Fix storage logs loading ([#2216](https://github.com/matter-labs/zksync-2-dev/issues/2216)) ([d393302](https://github.com/matter-labs/zksync-2-dev/commit/d393302795af69571fa3f30f25dbb0c3aa0b5a6b))

## [5.23.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.22.0...v5.23.0) (2023-07-24)

### Features

* Use jemalloc as a global allocator ([#2213](https://github.com/matter-labs/zksync-2-dev/issues/2213)) ([4a230b6](https://github.com/matter-labs/zksync-2-dev/commit/4a230b6054bf1f0da55a086163750901c244ef52))

### Performance Improvements

* **merkle tree:** Optimize loading data for tree in metadata calculator ([#2197](https://github.com/matter-labs/zksync-2-dev/issues/2197)) ([f7736bc](https://github.com/matter-labs/zksync-2-dev/commit/f7736bc16bae3e7553eea24d33d4436627942635))

## [5.22.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.21.0...v5.22.0) (2023-07-21)

### Features

* **fri-prover:** Generate setup-data for GPU FRI prover ([#2200](https://github.com/matter-labs/zksync-2-dev/issues/2200)) ([3213c2b](https://github.com/matter-labs/zksync-2-dev/commit/3213c2bdeb0f2929d53aaa713dcbe9b2e76fd022))

### Bug Fixes

* **house-keeper:** use proper display method in metric names ([#2209](https://github.com/matter-labs/zksync-2-dev/issues/2209)) ([894a033](https://github.com/matter-labs/zksync-2-dev/commit/894a03390a212cb6b205bc87e925abc8a203bab2))

## [5.21.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.20.1...v5.21.0) (2023-07-20)

### Features

* **api:** added `eth_feeHistory` endpoint ([#2201](https://github.com/matter-labs/zksync-2-dev/issues/2201)) ([7a16252](https://github.com/matter-labs/zksync-2-dev/commit/7a16252e1ba7cb3eb42cee6a14ea320ddcf3e0a3))
* **explorer-api:** add `/contract_verification/info/{address}` endpoint ([#2195](https://github.com/matter-labs/zksync-2-dev/issues/2195)) ([ade8019](https://github.com/matter-labs/zksync-2-dev/commit/ade80195c7cc0bc288959d556f62b286aa9db9b3))

### Bug Fixes

* **api:** Fix graceful shutdown for Tokio ([#2167](https://github.com/matter-labs/zksync-2-dev/issues/2167)) ([4542f51](https://github.com/matter-labs/zksync-2-dev/commit/4542f511c78cffee85a004cf24729a81de801e31))
* **contract-verifier:** fix some vyper verification scenarios ([#2203](https://github.com/matter-labs/zksync-2-dev/issues/2203)) ([5a749b0](https://github.com/matter-labs/zksync-2-dev/commit/5a749b03380db3aac88c808cce202ae0c7343863))
* **db:** Add index for getting  pending l1 batch txs  ([#2192](https://github.com/matter-labs/zksync-2-dev/issues/2192)) ([0ba7870](https://github.com/matter-labs/zksync-2-dev/commit/0ba78709e99282a57480b034ae16988f840b9073))
* **merkle tree:** Remove new tree throttling ([#2189](https://github.com/matter-labs/zksync-2-dev/issues/2189)) ([e18c450](https://github.com/matter-labs/zksync-2-dev/commit/e18c45094dd5d187ccf3e5a9e434e287dd5f2dc9))
* **ws-api:** handle closed pubsub connections when assigning id ([#2193](https://github.com/matter-labs/zksync-2-dev/issues/2193)) ([f8c448a](https://github.com/matter-labs/zksync-2-dev/commit/f8c448adac3ef31bc02e055cdd94207cc3d6c1c8))

### Performance Improvements

* better page datastructure ([#1812](https://github.com/matter-labs/zksync-2-dev/issues/1812)) ([80dcb34](https://github.com/matter-labs/zksync-2-dev/commit/80dcb3402ed65dbedf5153273564650942e099a6))
* **merkle tree:** Measure and optimize RAM usage by tree ([#2202](https://github.com/matter-labs/zksync-2-dev/issues/2202)) ([c86fe43](https://github.com/matter-labs/zksync-2-dev/commit/c86fe43e0007fcf47d5594fd4fe15ea15a74c92c))
* reduce memory use of memory pages by using chunks of 64 values ([#2204](https://github.com/matter-labs/zksync-2-dev/issues/2204)) ([4262c6d](https://github.com/matter-labs/zksync-2-dev/commit/4262c6d8ffaa4a45f3a619e92189ac8d575fe04f))

## [5.20.1](https://github.com/matter-labs/zksync-2-dev/compare/v5.20.0...v5.20.1) (2023-07-17)

### Bug Fixes

* **crypto:** update zkevm_circuits to fix sha256 circuits for FRI prover ([#2186](https://github.com/matter-labs/zksync-2-dev/issues/2186)) ([daf460e](https://github.com/matter-labs/zksync-2-dev/commit/daf460e0a5798c363b65c3de80fff53c743b20e8))
* **merkle tree:** Handle tree having more versions than L1 batches in Postgres ([#2179](https://github.com/matter-labs/zksync-2-dev/issues/2179)) ([7b3d8ad](https://github.com/matter-labs/zksync-2-dev/commit/7b3d8ad545a8c2f0993d5087ae387935e7dff381))
* remove annoying Cargo.lock ([#2181](https://github.com/matter-labs/zksync-2-dev/issues/2181)) ([04602a4](https://github.com/matter-labs/zksync-2-dev/commit/04602a446899f672789d83a24db85ea910d00c2f))

## [5.20.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.19.1...v5.20.0) (2023-07-14)

### Features

* **fri-prover-config:** use flattened env variable instead of single composite ([#2183](https://github.com/matter-labs/zksync-2-dev/issues/2183)) ([5b67f1f](https://github.com/matter-labs/zksync-2-dev/commit/5b67f1fa9297bd49f5c2bf4a4bfaa71850b1feaf))
* **merkle tree:** Retire the old tree implementation ([#2130](https://github.com/matter-labs/zksync-2-dev/issues/2130)) ([30738a7](https://github.com/matter-labs/zksync-2-dev/commit/30738a7488f4dfb726a6a64f546437b03dd721ed))
* **prover-fri:** Add impl for running specialized prover ([#2166](https://github.com/matter-labs/zksync-2-dev/issues/2166)) ([0892ffe](https://github.com/matter-labs/zksync-2-dev/commit/0892ffeb34fcb987987e1f36b1daecb1a5ec07f5))
* **witness-gen-fri:** force process configured block when sampling enabled ([#2177](https://github.com/matter-labs/zksync-2-dev/issues/2177)) ([12e0395](https://github.com/matter-labs/zksync-2-dev/commit/12e0395aabfe3f0f8a1968816ac46b5c4585746d))

## [5.19.1](https://github.com/matter-labs/zksync-2-dev/compare/v5.19.0...v5.19.1) (2023-07-13)

### Bug Fixes

* **crypto:** update circuits, VK to fix sha256 ([#2172](https://github.com/matter-labs/zksync-2-dev/issues/2172)) ([3e56d26](https://github.com/matter-labs/zksync-2-dev/commit/3e56d26c6007b0cabeb7b5af712232df99d8dc12))
* **healthcheck:** Don't panic if healthcheck stop channel is dropped ([#2174](https://github.com/matter-labs/zksync-2-dev/issues/2174)) ([51588ba](https://github.com/matter-labs/zksync-2-dev/commit/51588bacd60975eb697d2b9cb27e922666331cd4))

## [5.19.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.18.1...v5.19.0) (2023-07-13)

### Features

* **api:** Expose metrics on SQL connections and number of requests in flight ([#2169](https://github.com/matter-labs/zksync-2-dev/issues/2169)) ([7cda24b](https://github.com/matter-labs/zksync-2-dev/commit/7cda24b858dbf79b84be09cacbd5a56a1663f592))

## [5.18.1](https://github.com/matter-labs/zksync-2-dev/compare/v5.18.0...v5.18.1) (2023-07-12)

### Bug Fixes

* **house-keeper:** rename server to prover_fri while emitting queued jobs metrics ([#2162](https://github.com/matter-labs/zksync-2-dev/issues/2162)) ([599eb7c](https://github.com/matter-labs/zksync-2-dev/commit/599eb7c187d7a6833d6bc7f7c539f7bfb2b9dc38))

## [5.18.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.17.0...v5.18.0) (2023-07-11)

### Features

* **house-keeeper:** emit FRI prover jobs stats ([#2152](https://github.com/matter-labs/zksync-2-dev/issues/2152)) ([1fa413b](https://github.com/matter-labs/zksync-2-dev/commit/1fa413b656f967437008996084c2429b78e08c97))
* **witness-gen-fri:** Save aux_output_witness in public GCS bucket ([#2160](https://github.com/matter-labs/zksync-2-dev/issues/2160)) ([848e8de](https://github.com/matter-labs/zksync-2-dev/commit/848e8ded0ca1806f6404f3bfeaabdf19b5a3c840))

## [5.17.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.16.1...v5.17.0) (2023-07-11)

### Features

* **api:** Allow to disable VM limiter ([#2158](https://github.com/matter-labs/zksync-2-dev/issues/2158)) ([2c950c0](https://github.com/matter-labs/zksync-2-dev/commit/2c950c0729b945aced1769a4f88e46de4ca9c68d))
* **db:** cache smart contract code queries ([#1988](https://github.com/matter-labs/zksync-2-dev/issues/1988)) ([fb331f5](https://github.com/matter-labs/zksync-2-dev/commit/fb331f529527a721c35a952444f90a110b1d2c79))

### Bug Fixes

* Rewrite duration metrics for Aggregation stage latency ([#2124](https://github.com/matter-labs/zksync-2-dev/issues/2124)) ([7e50d31](https://github.com/matter-labs/zksync-2-dev/commit/7e50d31217d86c15268232789c795757517e967f))

## [5.16.1](https://github.com/matter-labs/zksync-2-dev/compare/v5.16.0...v5.16.1) (2023-07-10)

### Bug Fixes

* **api:** Emit less logs ([#2144](https://github.com/matter-labs/zksync-2-dev/issues/2144)) ([51d7748](https://github.com/matter-labs/zksync-2-dev/commit/51d7748439f964c013e1b0124b52b03b871989c0))

## [5.16.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.15.0...v5.16.0) (2023-07-10)

### Features

* **api:** Different config values for HTTP/WS server threads amount ([#2141](https://github.com/matter-labs/zksync-2-dev/issues/2141)) ([fc245f7](https://github.com/matter-labs/zksync-2-dev/commit/fc245f701a37d8b8e254183727005063c3275fb4))

### Performance Improvements

* **api:** Remove blocking code from API ([#2131](https://github.com/matter-labs/zksync-2-dev/issues/2131)) ([ca83489](https://github.com/matter-labs/zksync-2-dev/commit/ca83489d83f7ad0adbfdd50db21a52edd7c7fbc2))

## [5.15.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.14.2...v5.15.0) (2023-07-10)

### Features

* **witness-gen-fri:** save BlockAuxilaryOutputWitness in GCS in case its need for debugging ([#2137](https://github.com/matter-labs/zksync-2-dev/issues/2137)) ([fdc6127](https://github.com/matter-labs/zksync-2-dev/commit/fdc612735e2a54ce84645828de8473fa1cfd0895))

## [5.14.2](https://github.com/matter-labs/zksync-2-dev/compare/v5.14.1...v5.14.2) (2023-07-09)

### Bug Fixes

* **house-keeper:** make prover db pool size configurable ([#2138](https://github.com/matter-labs/zksync-2-dev/issues/2138)) ([12d101c](https://github.com/matter-labs/zksync-2-dev/commit/12d101cc469504b0ce58b2d583d8f8373f5773ff))

## [5.14.1](https://github.com/matter-labs/zksync-2-dev/compare/v5.14.0...v5.14.1) (2023-07-07)

### Bug Fixes

* **crypto:** update harness to use log_tracing to supress println's from boojum ([#2134](https://github.com/matter-labs/zksync-2-dev/issues/2134)) ([b0655ba](https://github.com/matter-labs/zksync-2-dev/commit/b0655ba4e8bba5264c59cff83008af7390ed963f))

## [5.14.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.13.1...v5.14.0) (2023-07-07)

### Features

* **prover-fri:** add metrics for FRI prover and witness-gen ([#2128](https://github.com/matter-labs/zksync-2-dev/issues/2128)) ([5cea755](https://github.com/matter-labs/zksync-2-dev/commit/5cea755285e75f40cff1412a100508aa34c68922))

### Bug Fixes

* **sdk:** Fix getting receipt for transactions rejected in statekeeper ([#2071](https://github.com/matter-labs/zksync-2-dev/issues/2071)) ([c97e494](https://github.com/matter-labs/zksync-2-dev/commit/c97e494c1ef7f58fe8632a3ebf943d775b1703cb))

## [5.13.1](https://github.com/matter-labs/zksync-2-dev/compare/v5.13.0...v5.13.1) (2023-07-06)

### Bug Fixes

* **fri-witness-generator:** update harness and use different vk for node at diff depth ([#2116](https://github.com/matter-labs/zksync-2-dev/issues/2116)) ([82fd38c](https://github.com/matter-labs/zksync-2-dev/commit/82fd38c3e6bd62f9ac4785d732dc01099b73d972))
* **healthcheck:** Do not kill health check ([#2115](https://github.com/matter-labs/zksync-2-dev/issues/2115)) ([aec1792](https://github.com/matter-labs/zksync-2-dev/commit/aec1792e84e3c91eeef619d0dfa3f66c2323828b))
* **object_store:** switch to using published version for gcs ([#2118](https://github.com/matter-labs/zksync-2-dev/issues/2118)) ([c779569](https://github.com/matter-labs/zksync-2-dev/commit/c779569af18911f1a2f2ef3d2c8c628e37d4038d))

### Performance Improvements

* **state-keeper:** Make `BatchExecutor` async-aware ([#2109](https://github.com/matter-labs/zksync-2-dev/issues/2109)) ([cc992b8](https://github.com/matter-labs/zksync-2-dev/commit/cc992b80adbcf02e6a68228a9531a777d00bca47))

## [5.13.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.12.1...v5.13.0) (2023-07-05)

### Features

* Add metrics for tracking eth_tx's stage transition duration PLA-146 ([#2084](https://github.com/matter-labs/zksync-2-dev/issues/2084)) ([4c29be3](https://github.com/matter-labs/zksync-2-dev/commit/4c29be30618ded958c961d7473632d1f8f5efa26))
* **api:** Fix api health check ([#2108](https://github.com/matter-labs/zksync-2-dev/issues/2108)) ([406d6ba](https://github.com/matter-labs/zksync-2-dev/commit/406d6ba4c6c588304d74baacf9b3d66deb82e60a))
* **api:** Use dedicated tokio runtime for VM in API ([#2111](https://github.com/matter-labs/zksync-2-dev/issues/2111)) ([e088b8b](https://github.com/matter-labs/zksync-2-dev/commit/e088b8b6f6de1da63fe000325bb4a7faddbdf862))
* **house-keeper:** emit seperate metrics for FRI witness-gen jobs in  house-keeper ([#2112](https://github.com/matter-labs/zksync-2-dev/issues/2112)) ([fd616de](https://github.com/matter-labs/zksync-2-dev/commit/fd616defbb6380a876faeda33a0901dd9e4b9f57))
* **prover-fri:** save scheduler proofs in public bucket as well ([#2101](https://github.com/matter-labs/zksync-2-dev/issues/2101)) ([8979649](https://github.com/matter-labs/zksync-2-dev/commit/897964911e7ba610722d82ae0182e60973736794))
* **state-keeper:** Log miniblock sealing ([#2105](https://github.com/matter-labs/zksync-2-dev/issues/2105)) ([fd6e8b4](https://github.com/matter-labs/zksync-2-dev/commit/fd6e8b4b6a03ba0071645233c7a2ad2e7d3e9f5c))

### Bug Fixes

* **house-keeper:** enable GCS blob cleaner ([#2103](https://github.com/matter-labs/zksync-2-dev/issues/2103)) ([bd79319](https://github.com/matter-labs/zksync-2-dev/commit/bd79319cb24d00e76407027aa9f83b395f685cb0))
* **witness-gen-fri:** update harness+zk_evm to fix witness gen and proof gen failure ([#2113](https://github.com/matter-labs/zksync-2-dev/issues/2113)) ([d445325](https://github.com/matter-labs/zksync-2-dev/commit/d445325cb7f70ffbfa2a3555ccc0f674e8810ee6))

## [5.12.1](https://github.com/matter-labs/zksync-2-dev/compare/v5.12.0...v5.12.1) (2023-07-04)

### Bug Fixes

* **api:** Gracefull shutdown web3 api ([#2075](https://github.com/matter-labs/zksync-2-dev/issues/2075)) ([bd45e57](https://github.com/matter-labs/zksync-2-dev/commit/bd45e574d11e137924e4be5ecc6ae10a5d0f465b))
* **external node:** Remove SK config from EN's TxSender ([#2093](https://github.com/matter-labs/zksync-2-dev/issues/2093)) ([aa04eaf](https://github.com/matter-labs/zksync-2-dev/commit/aa04eaf0f3b795b32dc1d6e25725a8ac7257ef99))
* **witness-gen:** update harness to fix FRI node agg witness-gen error ([#2104](https://github.com/matter-labs/zksync-2-dev/issues/2104)) ([eb68c5a](https://github.com/matter-labs/zksync-2-dev/commit/eb68c5a47d4674aa43edfafca161526c64bd912a))

## [5.12.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.11.0...v5.12.0) (2023-07-04)

### Features

* **contract-verifier:** add new zkvyper binaries and enable test ([#2096](https://github.com/matter-labs/zksync-2-dev/issues/2096)) ([308259e](https://github.com/matter-labs/zksync-2-dev/commit/308259e2f063e3a9fcf032372427be13344ed227))

### Bug Fixes

* **init:** Run gas adjuster only if necessary ([#2081](https://github.com/matter-labs/zksync-2-dev/issues/2081)) ([2ea9560](https://github.com/matter-labs/zksync-2-dev/commit/2ea95601fe433db759cc067e062d7e3b9c346a16))

## [5.11.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.10.1...v5.11.0) (2023-07-04)

### Features

* **api:** add `gas_per_pubdata` to `zks_getTransactionDetails` ([#2085](https://github.com/matter-labs/zksync-2-dev/issues/2085)) ([dd91bb6](https://github.com/matter-labs/zksync-2-dev/commit/dd91bb673b29a17cea91e12ec95f53deba556798))

### Bug Fixes

* **prover-fri:** update harness+circuits+boojum to fix proof failures ([#2094](https://github.com/matter-labs/zksync-2-dev/issues/2094)) ([e70c6f5](https://github.com/matter-labs/zksync-2-dev/commit/e70c6f5f08093a45a6958c80128518a431c63082))

## [5.10.1](https://github.com/matter-labs/zksync-2-dev/compare/v5.10.0...v5.10.1) (2023-07-03)

### Bug Fixes

* **witness-gen-fri:** pass server db url while processing to witness-gen ([#2091](https://github.com/matter-labs/zksync-2-dev/issues/2091)) ([b904ffb](https://github.com/matter-labs/zksync-2-dev/commit/b904ffb0e51add2e6e9ed80244bd5ca51f988ada))

## [5.10.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.9.0...v5.10.0) (2023-07-03)

### Features

* **api:** blockHash support in eth_getLogs ([#2072](https://github.com/matter-labs/zksync-2-dev/issues/2072)) ([4110bc0](https://github.com/matter-labs/zksync-2-dev/commit/4110bc0ef6085578770bad68f23990546f9fe8a9))
* **object store:** Make object store and GCS async ([#2050](https://github.com/matter-labs/zksync-2-dev/issues/2050)) ([266ee68](https://github.com/matter-labs/zksync-2-dev/commit/266ee68639cafcf198c0d19c2cdbcb07108ff0de))

### Bug Fixes

* **db:** add FOR UPDATE to query ([#2086](https://github.com/matter-labs/zksync-2-dev/issues/2086)) ([4f42cdb](https://github.com/matter-labs/zksync-2-dev/commit/4f42cdbddde46ee8f7ac3404b98d5384bf2ff3ec))
* set effective_gas_price for priority txs ([#2078](https://github.com/matter-labs/zksync-2-dev/issues/2078)) ([2bcdd52](https://github.com/matter-labs/zksync-2-dev/commit/2bcdd521e64fc5029acf7313232e821847670674))
* **witness-generator-fri:** pass prover DB variant as well to FRI witness-gen ([#2090](https://github.com/matter-labs/zksync-2-dev/issues/2090)) ([98b2743](https://github.com/matter-labs/zksync-2-dev/commit/98b274372e376e5e0630ad1dffc3269000927442))

### Performance Improvements

* **state-keeper:** Seal miniblocks in parallel to their execution ([#2026](https://github.com/matter-labs/zksync-2-dev/issues/2026)) ([4f4ba82](https://github.com/matter-labs/zksync-2-dev/commit/4f4ba823f0954f3cac46b1956a0eda3c3de274d9))

## [5.9.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.8.0...v5.9.0) (2023-07-01)

### Features

* **prover-fri:** move storing proofs away from DB to GCS ([#2070](https://github.com/matter-labs/zksync-2-dev/issues/2070)) ([4f97d3d](https://github.com/matter-labs/zksync-2-dev/commit/4f97d3de7d99b180fc5c1fc647be2a1367d0919d))
* **witness-gen:** split witness-gen config for FRI and old ([#2073](https://github.com/matter-labs/zksync-2-dev/issues/2073)) ([5903ca0](https://github.com/matter-labs/zksync-2-dev/commit/5903ca0c185bf38df743f614c83912926b0931e4))

### Bug Fixes

* **crypto:** update VK's from FRI prover ([#2074](https://github.com/matter-labs/zksync-2-dev/issues/2074)) ([833f57f](https://github.com/matter-labs/zksync-2-dev/commit/833f57f2fc9505ced4964cf00b7dc057c74928ae))
* **witness-gen:** update harness to fix FRI main VM proving ([#2080](https://github.com/matter-labs/zksync-2-dev/issues/2080)) ([edbad6b](https://github.com/matter-labs/zksync-2-dev/commit/edbad6b840231f78ad02542dce5be4ce1e7c1c91))

## [5.8.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.7.0...v5.8.0) (2023-06-30)

### Features

* **contract-verifier:** implement vyper contracts verification ([#2059](https://github.com/matter-labs/zksync-2-dev/issues/2059)) ([6535506](https://github.com/matter-labs/zksync-2-dev/commit/65355065ec84ee4236eea1d48db9b929ad40bf24))
* **fri-prover:** added proof verification based on config ([#2063](https://github.com/matter-labs/zksync-2-dev/issues/2063)) ([78aab56](https://github.com/matter-labs/zksync-2-dev/commit/78aab56ab8153b0f7fe7f6fc74a52c1f5bba7601))
* **witness-gen:** add # of dependent jobs in node agg ([#2066](https://github.com/matter-labs/zksync-2-dev/issues/2066)) ([5f4f780](https://github.com/matter-labs/zksync-2-dev/commit/5f4f780d3399282491144ea8d2efbaba0904fc7a))

### Bug Fixes

* **dal:** add indices for new provers related table ([#2068](https://github.com/matter-labs/zksync-2-dev/issues/2068)) ([2aeb3be](https://github.com/matter-labs/zksync-2-dev/commit/2aeb3be478bda00dd01547dda3364436c1417f50))
* stage tests ([#2058](https://github.com/matter-labs/zksync-2-dev/issues/2058)) ([707cfb5](https://github.com/matter-labs/zksync-2-dev/commit/707cfb57858ee590a40e36ce89124709836f99f8))
* **witness-gen:** update harness to fix proof gen failure for fri pro… ([#2064](https://github.com/matter-labs/zksync-2-dev/issues/2064)) ([d9f7e88](https://github.com/matter-labs/zksync-2-dev/commit/d9f7e88be2650fc9c29f45829222758d086c356f))

## [5.7.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.6.0...v5.7.0) (2023-06-29)

### Features

* **contract-verifier:** add zksolc v1.3.12 ([#2060](https://github.com/matter-labs/zksync-2-dev/issues/2060)) ([b379af9](https://github.com/matter-labs/zksync-2-dev/commit/b379af9d1b8435ec5ac0069c56e054ed4114de00))

## [5.6.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.5.1...v5.6.0) (2023-06-29)

### Features

* (DONT MERGE!) Integrate WETH bridge into server & SDK ([#1929](https://github.com/matter-labs/zksync-2-dev/issues/1929)) ([b3caf1e](https://github.com/matter-labs/zksync-2-dev/commit/b3caf1e35718c742e8d1d59427855df3b9109300))
* add tx_index_in_l1_batch field to L2ToL1Log ([#2032](https://github.com/matter-labs/zksync-2-dev/issues/2032)) ([3ce5779](https://github.com/matter-labs/zksync-2-dev/commit/3ce5779f500d5738c92e09eff13d553e20625055))
* Clasify crypto alerts and monitor them ([#1895](https://github.com/matter-labs/zksync-2-dev/issues/1895)) ([e05fb64](https://github.com/matter-labs/zksync-2-dev/commit/e05fb642c03acd07ad800735648c00eea20d90da))
* **contract-verifier:** vyper contract verification ([#2041](https://github.com/matter-labs/zksync-2-dev/issues/2041)) ([f22d3ec](https://github.com/matter-labs/zksync-2-dev/commit/f22d3ecd272041185958b1d79e13fafafb191cdb))
* **external node:** Config fixups ([#2037](https://github.com/matter-labs/zksync-2-dev/issues/2037)) ([fe050e4](https://github.com/matter-labs/zksync-2-dev/commit/fe050e415e15fa090a81ffa21c11f8d926c3e964))
* **house-keeper:** added scheduler dependency tracker and queuer ([#2045](https://github.com/matter-labs/zksync-2-dev/issues/2045)) ([ca23434](https://github.com/matter-labs/zksync-2-dev/commit/ca23434532d97506480b25d22f3a016c42232de1))
* **house-keeper:** move FRI witness-gen leaf jobs to queued when ready ([#2020](https://github.com/matter-labs/zksync-2-dev/issues/2020)) ([f1c2287](https://github.com/matter-labs/zksync-2-dev/commit/f1c2287ab0edaeb8b96d264f98cab86333d86439))
* **house-keeper:** re-queue stuck FRI prover & witness-gen jobs ([#2047](https://github.com/matter-labs/zksync-2-dev/issues/2047)) ([4d38ff9](https://github.com/matter-labs/zksync-2-dev/commit/4d38ff949c9a0a71c1439db14bb9e24eda980fbd))
* **housekeeper:** Move node jobs from waiting to queued ([#2042](https://github.com/matter-labs/zksync-2-dev/issues/2042)) ([03bee75](https://github.com/matter-labs/zksync-2-dev/commit/03bee7514ce55119ea84184181b5056f767616aa))
* **prover-fri:** add is_node_final_proof for scheduler proving ([#2054](https://github.com/matter-labs/zksync-2-dev/issues/2054)) ([57a8686](https://github.com/matter-labs/zksync-2-dev/commit/57a86862ddea3c9887be7a0623fe88691ec0680d))
* **prover-fri:** added leaf layer proof gen and used cached setup data ([#2005](https://github.com/matter-labs/zksync-2-dev/issues/2005)) ([7512769](https://github.com/matter-labs/zksync-2-dev/commit/75127696d3aef473423d252c17fc1fa9dceed563))
* **setup-data:** add logic for generating VK's and setup-data for node+scheduler circuit ([#2035](https://github.com/matter-labs/zksync-2-dev/issues/2035)) ([d627826](https://github.com/matter-labs/zksync-2-dev/commit/d627826ce64d08c44fc83744c1c6ae464418db3a))
* **test_node:** Added ability to fetch & apply mainnet/testnet transaction ([#2012](https://github.com/matter-labs/zksync-2-dev/issues/2012)) ([90dd419](https://github.com/matter-labs/zksync-2-dev/commit/90dd41976a3a73eb7ea4158fc86c762d31fd507b))
* **witness-gen:** add impl for scheduler witness-gen ([#2051](https://github.com/matter-labs/zksync-2-dev/issues/2051)) ([f22704c](https://github.com/matter-labs/zksync-2-dev/commit/f22704cc4c30d8928996c8db652c47622c2890a7))
* **witness-gen:** impl node witness-gen ([#1991](https://github.com/matter-labs/zksync-2-dev/issues/1991)) ([4118022](https://github.com/matter-labs/zksync-2-dev/commit/4118022cba3f205f9b57e0cc8fa3103ac8bc3026))

### Bug Fixes

* **api:** unconditionally allow getLogs for single block ([#2039](https://github.com/matter-labs/zksync-2-dev/issues/2039)) ([70dfb19](https://github.com/matter-labs/zksync-2-dev/commit/70dfb19b889b9f90bd5283ef532dca494da57e0a))
* **eth-sender:** fix next nonce loading ([#2030](https://github.com/matter-labs/zksync-2-dev/issues/2030)) ([2b639ac](https://github.com/matter-labs/zksync-2-dev/commit/2b639ac56fa831628773e7720c16426f488cc9db))
* **external node:** Make sure that batch status updater progress is processed ([#2024](https://github.com/matter-labs/zksync-2-dev/issues/2024)) ([8ed95c5](https://github.com/matter-labs/zksync-2-dev/commit/8ed95c52962b49d4394e951f41e32ac67c7b832d))
* make tx_index_in_l1_batch_optional ([#2053](https://github.com/matter-labs/zksync-2-dev/issues/2053)) ([c0972f6](https://github.com/matter-labs/zksync-2-dev/commit/c0972f6ccf99b4790d97c1a55af2eb87b812efbd))
* **prover:** Add more traces for troubleshooting prover startup ([#2031](https://github.com/matter-labs/zksync-2-dev/issues/2031)) ([9c7e832](https://github.com/matter-labs/zksync-2-dev/commit/9c7e832f4f9cbf6dba311f3a105afbc07ef38863))
* **prover:** Make socket_listener tokio compliant ([#2049](https://github.com/matter-labs/zksync-2-dev/issues/2049)) ([3c7fa82](https://github.com/matter-labs/zksync-2-dev/commit/3c7fa8212126a2fec0537bde0bd210a5f6598643))
* **prover:** Split logging and sentry, add logging to prover subsystems and remove unnecessary traces ([#2033](https://github.com/matter-labs/zksync-2-dev/issues/2033)) ([15538b5](https://github.com/matter-labs/zksync-2-dev/commit/15538b542f708e8f9667f8b2c9e7ce2fa85eba6a))

### Performance Improvements

* **db:** Cache L1 batch number in `PostgresStorage` ([#2028](https://github.com/matter-labs/zksync-2-dev/issues/2028)) ([092a32c](https://github.com/matter-labs/zksync-2-dev/commit/092a32ced4d10e420e284360e3d2ab8f21eed71a))

### Reverts

* **contract-verifier:** vyper contract verification ([#2041](https://github.com/matter-labs/zksync-2-dev/issues/2041)) ([#2057](https://github.com/matter-labs/zksync-2-dev/issues/2057)) ([c263643](https://github.com/matter-labs/zksync-2-dev/commit/c263643d3dcc1bc34588ff7607537ef0f82377a4))

## [5.5.1](https://github.com/matter-labs/zksync-2-dev/compare/v5.5.0...v5.5.1) (2023-06-22)

### Bug Fixes

* **state-keeper:** Do not treat default CF as obsolete ([#2017](https://github.com/matter-labs/zksync-2-dev/issues/2017)) ([8b53210](https://github.com/matter-labs/zksync-2-dev/commit/8b53210f1a587bd908e3dfe5506ba99e2c61fdc6))

## [5.5.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.4.1...v5.5.0) (2023-06-22)

### Features

* **external node:** create a single method to fetch all miniblock data required ([#1999](https://github.com/matter-labs/zksync-2-dev/issues/1999)) ([e4912f1](https://github.com/matter-labs/zksync-2-dev/commit/e4912f1a427ce0f46ccabb122f15a54650f9ec02))
* **prover-setup-data:** added binary to generate prover setup data ([#1954](https://github.com/matter-labs/zksync-2-dev/issues/1954)) ([d3773d4](https://github.com/matter-labs/zksync-2-dev/commit/d3773d435c18434c8f39515eb35021fa74428d69))

### Bug Fixes

* **merkle tree:** Fix opening RocksDB with obsolete CFs ([#2007](https://github.com/matter-labs/zksync-2-dev/issues/2007)) ([667fe4c](https://github.com/matter-labs/zksync-2-dev/commit/667fe4ce14a09609c1c3cf7b34c26fdc488ac6b3))

### Performance Improvements

* **merkle tree:** Prune old tree versions ([#1984](https://github.com/matter-labs/zksync-2-dev/issues/1984)) ([55ddb20](https://github.com/matter-labs/zksync-2-dev/commit/55ddb208a9325e3cfbe28917a841a4773cc88066))

## [5.4.1](https://github.com/matter-labs/zksync-2-dev/compare/v5.4.0...v5.4.1) (2023-06-21)

### Bug Fixes

* **api:** Acquire VM permit on the method handler level ([#1997](https://github.com/matter-labs/zksync-2-dev/issues/1997)) ([5701593](https://github.com/matter-labs/zksync-2-dev/commit/570159317d0ce1e1b5694e6e1f5dfacf3e7f92af))

## [5.4.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.3.0...v5.4.0) (2023-06-20)

### Features

* **eth:** use `finalized` block tag ([#1981](https://github.com/matter-labs/zksync-2-dev/issues/1981)) ([8e83e42](https://github.com/matter-labs/zksync-2-dev/commit/8e83e426992c32d763c019e80778aeeab544f6ce))
* **fri-vk:** added logic for generating recursive vk ([#1987](https://github.com/matter-labs/zksync-2-dev/issues/1987)) ([4d3f07e](https://github.com/matter-labs/zksync-2-dev/commit/4d3f07e766c0c70e83c2b18ee648a37d6e3fe449))
* **testing:** In memory node with forking ([#1989](https://github.com/matter-labs/zksync-2-dev/issues/1989)) ([79820b5](https://github.com/matter-labs/zksync-2-dev/commit/79820b59f9569bba22538522def2d07214a9be32))
* **witness-gen:** added impl for leaf aggregation witness-gen ([#1985](https://github.com/matter-labs/zksync-2-dev/issues/1985)) ([033fb73](https://github.com/matter-labs/zksync-2-dev/commit/033fb73d794b157fa3a7766f8a2cc029fedebc52))

### Bug Fixes

* **prover:** Fix tokio usage in prover ([#1998](https://github.com/matter-labs/zksync-2-dev/issues/1998)) ([c905497](https://github.com/matter-labs/zksync-2-dev/commit/c905497e6e650dad6394da397d3cb3d1691c536e))

## [5.3.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.2.1...v5.3.0) (2023-06-16)

### Features

* **api:** Implement concurrent VM limiter ([#1982](https://github.com/matter-labs/zksync-2-dev/issues/1982)) ([c818fec](https://github.com/matter-labs/zksync-2-dev/commit/c818feccd63674bb45d0b0ac293cc5ee76fcd63d))
* **prover:** integrate new prover for basic circuit ([#1965](https://github.com/matter-labs/zksync-2-dev/issues/1965)) ([7d63db7](https://github.com/matter-labs/zksync-2-dev/commit/7d63db7122619d36b3af92b28ae85f130284a0ea))
* **witness-gen:** enable basic circuit witness-gen by copying input to shadow tables ([#1970](https://github.com/matter-labs/zksync-2-dev/issues/1970)) ([1c818a2](https://github.com/matter-labs/zksync-2-dev/commit/1c818a28eac7a81283ba3b890340707ac65c6fb3))

### Bug Fixes

* **circuits:** mark_witness_job_as_failed must use different dbs ([#1974](https://github.com/matter-labs/zksync-2-dev/issues/1974)) ([143f319](https://github.com/matter-labs/zksync-2-dev/commit/143f3195d3393312364a60a19fc4bbf5e78f5212))
* **eth-sender:** simplify logic for getting executed blocks ([#1973](https://github.com/matter-labs/zksync-2-dev/issues/1973)) ([2781006](https://github.com/matter-labs/zksync-2-dev/commit/2781006c918553e54f20afdbe80cca7d64ecc389))
* **loadtest:** Make fail fast semantics optional ([#1983](https://github.com/matter-labs/zksync-2-dev/issues/1983)) ([ec4037c](https://github.com/matter-labs/zksync-2-dev/commit/ec4037ca0d9dc148eda3ca9e04380302574e03d8))
* **manifests:** Fix Package Manifests ([#1947](https://github.com/matter-labs/zksync-2-dev/issues/1947)) ([57a66e4](https://github.com/matter-labs/zksync-2-dev/commit/57a66e4487caef59fd3836535ad604da5f1d633f))

## [5.2.1](https://github.com/matter-labs/zksync-2-dev/compare/v5.2.0...v5.2.1) (2023-06-15)

### Bug Fixes

* **db:** add missing indices ([#1966](https://github.com/matter-labs/zksync-2-dev/issues/1966)) ([1580e89](https://github.com/matter-labs/zksync-2-dev/commit/1580e893609d5f1e813443f54e3172f3704d6626))
* **eth-sender:** fix get_ready_for_execute_blocks if no ready blocks ([#1972](https://github.com/matter-labs/zksync-2-dev/issues/1972)) ([cd9262a](https://github.com/matter-labs/zksync-2-dev/commit/cd9262ac2477e40b2b3156505ee22b0b90f186ab))
* **external node:** Fix external_node_synced metric ([#1967](https://github.com/matter-labs/zksync-2-dev/issues/1967)) ([bacb3f5](https://github.com/matter-labs/zksync-2-dev/commit/bacb3f5f4fcce651dfffed7cf63436f3fa680b8e))
* **loadtest:** cast to u128, not to u64 to avoid overflow ([#1969](https://github.com/matter-labs/zksync-2-dev/issues/1969)) ([90f73c0](https://github.com/matter-labs/zksync-2-dev/commit/90f73c0fb89888624e13c8c13f7a2aa6ee29522d))
* **witness-gen:** use both db in witness-gen ([#1971](https://github.com/matter-labs/zksync-2-dev/issues/1971)) ([79f1843](https://github.com/matter-labs/zksync-2-dev/commit/79f1843f28f97d3da074c580623f5bbf4b12f6aa))

## [5.2.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.1.0...v5.2.0) (2023-06-14)

### Features

* **loadtest:** enhance loadtest observability for partners in DBS ([#1948](https://github.com/matter-labs/zksync-2-dev/issues/1948)) ([d3e4688](https://github.com/matter-labs/zksync-2-dev/commit/d3e4688e870d3414c211ecd2d70bdda4dc0fd40a))
* Make DAL interface async ([#1938](https://github.com/matter-labs/zksync-2-dev/issues/1938)) ([0e078ca](https://github.com/matter-labs/zksync-2-dev/commit/0e078ca3f7da9e218b952d7a9d307b927847c914))
* **merkle tree:** Collect stats on inserted node level ([#1964](https://github.com/matter-labs/zksync-2-dev/issues/1964)) ([ecf474d](https://github.com/matter-labs/zksync-2-dev/commit/ecf474dbe2b72b31c34e340c2b79e060a96c560e))
* **prover:** Split prover subsystems in it's own db under main branch ([#1951](https://github.com/matter-labs/zksync-2-dev/issues/1951)) ([b0d329d](https://github.com/matter-labs/zksync-2-dev/commit/b0d329def1791e57a11e1fd79eb38c560f17b74c))
* vm 1.3.3 update + initial witness generator for 1.4 ([#1928](https://github.com/matter-labs/zksync-2-dev/issues/1928)) ([46e260b](https://github.com/matter-labs/zksync-2-dev/commit/46e260b7b5a6b2940e4e6002d58d05166dbf0a62))
* **witness-gen:** basic-circuit witness-gen for FRI prover ([#1937](https://github.com/matter-labs/zksync-2-dev/issues/1937)) ([5b5fb28](https://github.com/matter-labs/zksync-2-dev/commit/5b5fb28cf02be4704428a92ffbf898448b367e2b))

### Bug Fixes

* **api:** use all tokens in api ([#1959](https://github.com/matter-labs/zksync-2-dev/issues/1959)) ([cc11149](https://github.com/matter-labs/zksync-2-dev/commit/cc11149c14484dd4da8397311cbd187548c7d371))
* **db:** `storage_logs_contract_address_tx_hash_idx` index ([#1956](https://github.com/matter-labs/zksync-2-dev/issues/1956)) ([6cc5edd](https://github.com/matter-labs/zksync-2-dev/commit/6cc5eddd191b4304fbe8f524745614ceee9a8cae))
* **eth-sender:** Do not send execute tx with a gap between batches ([#1934](https://github.com/matter-labs/zksync-2-dev/issues/1934)) ([ab8dc59](https://github.com/matter-labs/zksync-2-dev/commit/ab8dc59e7f7ad9ee4fe0aa053a111855c1f91c04))
* **eth-sender:** Move getting base system contracts to the loop itera… ([#1958](https://github.com/matter-labs/zksync-2-dev/issues/1958)) ([292122a](https://github.com/matter-labs/zksync-2-dev/commit/292122a89d23b75bb126abcf5b96bc8a1e1c71ac))
* **external node:** Separate batch status updater and fetcher ([#1961](https://github.com/matter-labs/zksync-2-dev/issues/1961)) ([2c59d4c](https://github.com/matter-labs/zksync-2-dev/commit/2c59d4c2e92a5e6716fb131f537a6ddaf73297ab))
* **metrics:** switch to pull based metrics in prover & synthesizer ([#1918](https://github.com/matter-labs/zksync-2-dev/issues/1918)) ([e634c73](https://github.com/matter-labs/zksync-2-dev/commit/e634c73b21587bc0aecbd4e43b17ff711d346ad1))

## [5.1.0](https://github.com/matter-labs/zksync-2-dev/compare/v5.0.1...v5.1.0) (2023-06-07)

### Features

* **contract-verifier:** add zksolc v1.3.11 ([#1936](https://github.com/matter-labs/zksync-2-dev/issues/1936)) ([4a13986](https://github.com/matter-labs/zksync-2-dev/commit/4a139868217414bcf3aa77c75aac05722ea4a096))
* **explorer-api:** include miniblock timestamp to explorer api  ([#1894](https://github.com/matter-labs/zksync-2-dev/issues/1894)) ([1e86627](https://github.com/matter-labs/zksync-2-dev/commit/1e86627c70823d557ead871696a726b4aee29bec))
* **external node:** Explicitly state that EN is alpha ([#1917](https://github.com/matter-labs/zksync-2-dev/issues/1917)) ([b81dccd](https://github.com/matter-labs/zksync-2-dev/commit/b81dccd8c076d7c9e43f0bebd44eabd88096b054))
* **external node:** Prepare docker image for public use ([#1906](https://github.com/matter-labs/zksync-2-dev/issues/1906)) ([1fcf5b5](https://github.com/matter-labs/zksync-2-dev/commit/1fcf5b543bfea63ad305eb868487f53ad0ba223a))
* **loadtest:** run loadtest on stage2 daily  ([#1852](https://github.com/matter-labs/zksync-2-dev/issues/1852)) ([196d9e4](https://github.com/matter-labs/zksync-2-dev/commit/196d9e40ec2a57075b061beba06e675c78564b6a))
* **merkle tree:** Add tag to tree manifest ([#1873](https://github.com/matter-labs/zksync-2-dev/issues/1873)) ([cd18a63](https://github.com/matter-labs/zksync-2-dev/commit/cd18a639a262c8ffea2d3e55f80d8b454fe22a1a))
* **vk:** added vk generator for new prover ([#1931](https://github.com/matter-labs/zksync-2-dev/issues/1931)) ([669e976](https://github.com/matter-labs/zksync-2-dev/commit/669e97626dc63a2f566f16957dd61ac94eabc226))

### Bug Fixes

* **external node checker:** Fix Sync Bug ([#1924](https://github.com/matter-labs/zksync-2-dev/issues/1924)) ([1a37f6b](https://github.com/matter-labs/zksync-2-dev/commit/1a37f6ba87c8a3aa0c0e30682db9e8a57b3c462a))
* Remove binary search from logs ([#1911](https://github.com/matter-labs/zksync-2-dev/issues/1911)) ([f3553f5](https://github.com/matter-labs/zksync-2-dev/commit/f3553f57c3f40e292e51ab18cc81ba3fdac20dbb))

### Performance Improvements

* Box storage and event logs ([#1887](https://github.com/matter-labs/zksync-2-dev/issues/1887)) ([13e7078](https://github.com/matter-labs/zksync-2-dev/commit/13e70780704037cdb32ab91427ef2bb1d6a2d622))
* improve performance of repeated far calls ([#1902](https://github.com/matter-labs/zksync-2-dev/issues/1902)) ([b9b96e7](https://github.com/matter-labs/zksync-2-dev/commit/b9b96e7d230fdbd705236425a65a8e698cdfdbb9))

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

* **contract-verifier:** add zksolc v1.3.11 ([#1754](https://github.com/matter-labs/zksync-2-dev/issues/1754)) ([f6dd7fe](https://github.com/matter-labs/zksync-2-dev/commit/f6dd7fe31b42b6304478c45481e40bbf9f59fdbb))
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
