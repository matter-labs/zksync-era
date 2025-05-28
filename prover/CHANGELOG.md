# Changelog

## [21.1.0](https://github.com/matter-labs/zksync-era/compare/prover-v21.0.0...prover-v21.1.0) (2025-05-22)


### Features

* Configuration system PoC ([#3851](https://github.com/matter-labs/zksync-era/issues/3851)) ([7b449c2](https://github.com/matter-labs/zksync-era/commit/7b449c216aa250cf99bb79e69df810f566dcc28a))
* **prover:** Autoscaler detect which pod out of resources and reschedule ([#4008](https://github.com/matter-labs/zksync-era/issues/4008)) ([56658c8](https://github.com/matter-labs/zksync-era/commit/56658c89f83f3d6c188df5c1944ac9f9a7307ef9))


### Bug Fixes

* Prefix env vars for `zksync_server` ([#4039](https://github.com/matter-labs/zksync-era/issues/4039)) ([3b2dd70](https://github.com/matter-labs/zksync-era/commit/3b2dd70e858312d1709b8ac64a651f4f7f806e05))
* **prover:** Fix revert GPU prover table migration ([#4047](https://github.com/matter-labs/zksync-era/issues/4047)) ([764e73c](https://github.com/matter-labs/zksync-era/commit/764e73c92ea737e19e8d6f5903df587413ed79a7))
* **vm:** update deps ([#4019](https://github.com/matter-labs/zksync-era/issues/4019)) ([2b51c6d](https://github.com/matter-labs/zksync-era/commit/2b51c6df08b45cc41fd6b4d930c31670d004a93d))
* **vm:** update v27 ([#4035](https://github.com/matter-labs/zksync-era/issues/4035)) ([b7aeab6](https://github.com/matter-labs/zksync-era/commit/b7aeab64ce5c915233a773542ef64e79bf3893ee))

## [21.0.0](https://github.com/matter-labs/zksync-era/compare/prover-v20.3.1...prover-v21.0.0) (2025-05-13)


### ⚠ BREAKING CHANGES

* update zksync-protocol and zksync-crypto-gpu deps ([#3948](https://github.com/matter-labs/zksync-era/issues/3948))

### Features

* Add Support for Protocol Version v28 ([#3821](https://github.com/matter-labs/zksync-era/issues/3821)) ([5419420](https://github.com/matter-labs/zksync-era/commit/5419420e23a3c083187065219a0722179dab0419))
* bump rustc to `nightly-2025-03-19` ([#3985](https://github.com/matter-labs/zksync-era/issues/3985)) ([d27390e](https://github.com/matter-labs/zksync-era/commit/d27390e14a586de7dccb974a0cb54352de3536b3))
* update zksync-protocol and zksync-crypto-gpu deps ([#3948](https://github.com/matter-labs/zksync-era/issues/3948)) ([1ddf85a](https://github.com/matter-labs/zksync-era/commit/1ddf85a14b6a54ce926482eafc84ef0979c9afd3))


### Bug Fixes

* Join on chain_id in WG queries ([#3950](https://github.com/matter-labs/zksync-era/issues/3950)) ([0975a88](https://github.com/matter-labs/zksync-era/commit/0975a8887c13698f7682be641cc5f0762ce7c146))
* number of chunks for inserting prover jobs ([#3952](https://github.com/matter-labs/zksync-era/issues/3952)) ([5c59a11](https://github.com/matter-labs/zksync-era/commit/5c59a1192f8c0e68b4a2ab8a75390aceff5c413e))
* **prover:** Check pods list for stale ones every `pod_check_interval` ([#3982](https://github.com/matter-labs/zksync-era/issues/3982)) ([0bc1e46](https://github.com/matter-labs/zksync-era/commit/0bc1e46d710aea254f8005993ffca345b5aa3868))

## [20.3.1](https://github.com/matter-labs/zksync-era/compare/prover-v20.3.0...prover-v20.3.1) (2025-05-02)


### Bug Fixes

* Fix issue with EVM bytecodes not being stored in DB ([#3928](https://github.com/matter-labs/zksync-era/issues/3928)) ([0e8f1da](https://github.com/matter-labs/zksync-era/commit/0e8f1dacb27be3b33aaabf842ea4350e20a3cada))
* **prover:** Scale down replicas using correct speed for queue calculation ([#3926](https://github.com/matter-labs/zksync-era/issues/3926)) ([868d6dc](https://github.com/matter-labs/zksync-era/commit/868d6dc96806276ce1d9b97f434febac46838dcc))

## [20.3.0](https://github.com/matter-labs/zksync-era/compare/prover-v20.2.0...prover-v20.3.0) (2025-04-30)


### Features

* Proof data handler client ([#3874](https://github.com/matter-labs/zksync-era/issues/3874)) ([daf6f7b](https://github.com/matter-labs/zksync-era/commit/daf6f7b80a018204693f8ad7296574b8b55dc6d9))
* **prover:** Add support of H100 into provers and gpu-checker ([#3887](https://github.com/matter-labs/zksync-era/issues/3887)) ([ec9e4a9](https://github.com/matter-labs/zksync-era/commit/ec9e4a9da99502e8e853574ea3a27cd0bb16e8fe))
* Reversed prover gateway server ([#3855](https://github.com/matter-labs/zksync-era/issues/3855)) ([a78c3ae](https://github.com/matter-labs/zksync-era/commit/a78c3ae1f74c0f4adfc9b94681c7b80b44dd16f1))
* rework prover job identifiers ([#3888](https://github.com/matter-labs/zksync-era/issues/3888)) ([073326f](https://github.com/matter-labs/zksync-era/commit/073326f124eae808ef0e25694e99f0dab5ee7af4))


### Bug Fixes

* Aux output witness object store key ([#3918](https://github.com/matter-labs/zksync-era/issues/3918)) ([4609d0f](https://github.com/matter-labs/zksync-era/commit/4609d0fc28a4cb201242017bf6433126dee39343))

## [20.2.0](https://github.com/matter-labs/zksync-era/compare/prover-v20.1.0...prover-v20.2.0) (2025-04-11)


### Features

* Rework serialization of prover API-related types ([#3805](https://github.com/matter-labs/zksync-era/issues/3805)) ([afafc29](https://github.com/matter-labs/zksync-era/commit/afafc292ea003e19d280c0178ca6b49b5b14917f))
* Update prover job ordering ([#3769](https://github.com/matter-labs/zksync-era/issues/3769)) ([5b74022](https://github.com/matter-labs/zksync-era/commit/5b740224fd27b0792d2d1602577c032fd3a31074))


### Bug Fixes

* Address parameter escaping in WG DAL ([#3812](https://github.com/matter-labs/zksync-era/issues/3812)) ([afc659f](https://github.com/matter-labs/zksync-era/commit/afc659fc0e6d5f3932d7be3e6706dd594bd4af59))
* Fix security issues (bump dependencies) ([#3813](https://github.com/matter-labs/zksync-era/issues/3813)) ([c6def9c](https://github.com/matter-labs/zksync-era/commit/c6def9c0e480bd73fc0ea29a7d3393c297c8afb7))
* **prover:** Replace Histogram with Gauge for startup_time/run_time/shutdown_time metrics ([#3793](https://github.com/matter-labs/zksync-era/issues/3793)) ([0bd6dc9](https://github.com/matter-labs/zksync-era/commit/0bd6dc9433bd5121c7dd583efde43f296c73fc63))
* Remove Circuit & CircuitWrapper duplication ([#3816](https://github.com/matter-labs/zksync-era/issues/3816)) ([6de1014](https://github.com/matter-labs/zksync-era/commit/6de1014c659247d3c29d349d5771778424241ecd))

## [20.1.0](https://github.com/matter-labs/zksync-era/compare/prover-v20.0.0...prover-v20.1.0) (2025-04-02)


### Features

* Update zksync-protocol deps to 0.151.5 ([#3790](https://github.com/matter-labs/zksync-era/issues/3790)) ([92beffe](https://github.com/matter-labs/zksync-era/commit/92beffe52ed2d40b11d800afbe97e9099d6f90a8))


### Bug Fixes

* **prover:** Correctly calculate NeedToMove and recent scale errors ([#3783](https://github.com/matter-labs/zksync-era/issues/3783)) ([aa46f89](https://github.com/matter-labs/zksync-era/commit/aa46f897596180b49b760dec2d2e418c760ab70e))
* **prover:** Force set all the `prover_job` labels to 0 ([#3787](https://github.com/matter-labs/zksync-era/issues/3787)) ([3ecc8db](https://github.com/matter-labs/zksync-era/commit/3ecc8db0862f6b893a9ab09f76532d937dc8cf62))

## [20.0.0](https://github.com/matter-labs/zksync-era/compare/prover-v19.0.0...prover-v20.0.0) (2025-04-01)


### ⚠ BREAKING CHANGES

* Remove old prover stack ([#3729](https://github.com/matter-labs/zksync-era/issues/3729))
* V27 update ([#3580](https://github.com/matter-labs/zksync-era/issues/3580))

### Features

* Add S3 implementation for object_store ([#3664](https://github.com/matter-labs/zksync-era/issues/3664)) ([a848927](https://github.com/matter-labs/zksync-era/commit/a848927082bfb1b5edcc7d5e4dc33d6f39271953))
* **consensus:** Add consensus protocol versioning ([#3720](https://github.com/matter-labs/zksync-era/issues/3720)) ([d1b4308](https://github.com/matter-labs/zksync-era/commit/d1b4308ff82da11515d8080c8e83f67c0f1812eb))
* **gateway:** Migration to Gateway  ([#3654](https://github.com/matter-labs/zksync-era/issues/3654)) ([2858ba0](https://github.com/matter-labs/zksync-era/commit/2858ba028a4e59eb518515e8dd56de9f609c3469))
* Remove old prover stack ([#3729](https://github.com/matter-labs/zksync-era/issues/3729)) ([fbbdc76](https://github.com/matter-labs/zksync-era/commit/fbbdc76b86bf4f474c4c045778b69f80a30e9c60))
* Use JSON-RPC for core &lt;&gt; prover interaction ([#3626](https://github.com/matter-labs/zksync-era/issues/3626)) ([4e74730](https://github.com/matter-labs/zksync-era/commit/4e7473011e6551bbeb3e7862872e99721aeba232))
* V27 update ([#3580](https://github.com/matter-labs/zksync-era/issues/3580)) ([9e18550](https://github.com/matter-labs/zksync-era/commit/9e1855050e3457ecef2b45a75e993dcdc2de370a))


### Bug Fixes

* API URL for prover gateway ([#3716](https://github.com/matter-labs/zksync-era/issues/3716)) ([ca2c4a4](https://github.com/matter-labs/zksync-era/commit/ca2c4a4eb2bfe3330ea48cac6efb26fc8dfeaf48))
* make proof data handler backwards compatible ([#3767](https://github.com/matter-labs/zksync-era/issues/3767)) ([bdbbaaa](https://github.com/matter-labs/zksync-era/commit/bdbbaaa4974399afec2394e0ffea9f9f6876e1e2))
* Prover job ordering ([#3738](https://github.com/matter-labs/zksync-era/issues/3738)) ([8f7f831](https://github.com/matter-labs/zksync-era/commit/8f7f83124046d3337a2752a92fe9b8eef44df6c0))
* **prover:** Fixing shutdown time for circuit-prover ([#3768](https://github.com/matter-labs/zksync-era/issues/3768)) ([6c9de26](https://github.com/matter-labs/zksync-era/commit/6c9de268a1583483fbddd0eacf60a9d265f01d73))
* **prover:** Reevaluation of 'heavy' jobs for WVG ([#3754](https://github.com/matter-labs/zksync-era/issues/3754)) ([2a8d33b](https://github.com/matter-labs/zksync-era/commit/2a8d33b92bb72540d37d35ce4677454d073ba002))
* **prover:** Remove deleted pods from autoscaler-agent cluster cache ([#3739](https://github.com/matter-labs/zksync-era/issues/3739)) ([e94985f](https://github.com/matter-labs/zksync-era/commit/e94985f38ec4c3d3ba5dca87a064ab2edf890549))

## [19.0.0](https://github.com/matter-labs/zksync-era/compare/prover-v18.0.0...prover-v19.0.0) (2025-03-04)


### ⚠ BREAKING CHANGES

* Combine GPU and simple Scaler into single Scaler ([#3621](https://github.com/matter-labs/zksync-era/issues/3621))

### Features

* add custom DA support in external node ([#3445](https://github.com/matter-labs/zksync-era/issues/3445)) ([1a8546d](https://github.com/matter-labs/zksync-era/commit/1a8546ddcd6b126657a99f68576b2a837a4c416d))
* **contract-verifier:** Partial matching & automatic verification ([#3527](https://github.com/matter-labs/zksync-era/issues/3527)) ([bf9fe85](https://github.com/matter-labs/zksync-era/commit/bf9fe85f4fd1d739105e7b21d0eebb377f752bac))
* make `zksync_types` thinner ([#3574](https://github.com/matter-labs/zksync-era/issues/3574)) ([e7f93e4](https://github.com/matter-labs/zksync-era/commit/e7f93e43dd55674a1442111cc1f08c9d229d3e22))
* preparation for new precompiles ([#3535](https://github.com/matter-labs/zksync-era/issues/3535)) ([3c1f3fb](https://github.com/matter-labs/zksync-era/commit/3c1f3fb0f24d1c19dce52b98df521703fa1bf638))
* **prover:** Add gpu_checker binary ([#3573](https://github.com/matter-labs/zksync-era/issues/3573)) ([51b9a03](https://github.com/matter-labs/zksync-era/commit/51b9a03941a9836fb9cda92ced35c10a92ab4e3f))
* Refactor WG DAL ([#3560](https://github.com/matter-labs/zksync-era/issues/3560)) ([d1ede36](https://github.com/matter-labs/zksync-era/commit/d1ede36dc17d750baa7c76d6c027e8d0e9385c7e))
* Rework prover job handling ([#3561](https://github.com/matter-labs/zksync-era/issues/3561)) ([ddc4243](https://github.com/matter-labs/zksync-era/commit/ddc42435e5aa41122c165b14be64bf6b071c25a4))
* Updating information about keys & commitments for releases ([#3486](https://github.com/matter-labs/zksync-era/issues/3486)) ([cd21c9e](https://github.com/matter-labs/zksync-era/commit/cd21c9ea31192889d83d85be549290cd99c7bba6))
* **vm:** Allow caching signature verification ([#3505](https://github.com/matter-labs/zksync-era/issues/3505)) ([7bb5ed3](https://github.com/matter-labs/zksync-era/commit/7bb5ed377719227f5c9861231e110dd9a5bb2ac0))


### Bug Fixes

* Add autoscaler_queue metric back ([#3668](https://github.com/matter-labs/zksync-era/issues/3668)) ([7f36ed9](https://github.com/matter-labs/zksync-era/commit/7f36ed98fc6066c1224ff07c95282b647a8114fc))
* Allow witness_generator to use Prometheus push gateway in continuous mode ([#3555](https://github.com/matter-labs/zksync-era/issues/3555)) ([0f1a5e0](https://github.com/matter-labs/zksync-era/commit/0f1a5e0d61fbd7a2755bae6191184039f1c46e03))
* Simplify prover_jobs_fri indices ([#3577](https://github.com/matter-labs/zksync-era/issues/3577)) ([559edc3](https://github.com/matter-labs/zksync-era/commit/559edc34617e89c5d37e6a569b5d7a0740abdc46))


### Performance Improvements

* Add prover jobs as one multi-insert for Witness Generator ([#3587](https://github.com/matter-labs/zksync-era/issues/3587)) ([d150dca](https://github.com/matter-labs/zksync-era/commit/d150dcaf767eca45237ba9605fa5ccbc9f3f63e2))


### Code Refactoring

* Combine GPU and simple Scaler into single Scaler ([#3621](https://github.com/matter-labs/zksync-era/issues/3621)) ([8e24403](https://github.com/matter-labs/zksync-era/commit/8e24403190c4f7e306c88ccdb3c2f4a86e0e56fa))

## [18.0.0](https://github.com/matter-labs/zksync-era/compare/prover-v17.1.1...prover-v18.0.0) (2025-01-27)


### ⚠ BREAKING CHANGES

* **contracts:** gateway integration ([#1934](https://github.com/matter-labs/zksync-era/issues/1934))

### Features

* Compressor optimizations ([#3476](https://github.com/matter-labs/zksync-era/issues/3476)) ([3e931be](https://github.com/matter-labs/zksync-era/commit/3e931be6bddaacbd7d029c537db03a3c191fdc21))
* **consensus:** Added view_timeout to consensus config ([#3383](https://github.com/matter-labs/zksync-era/issues/3383)) ([fc02a8f](https://github.com/matter-labs/zksync-era/commit/fc02a8f1c9f0bffb438fb27769d6dced3ce14cd9))
* **consensus:** Update consensus dependencies ([#3339](https://github.com/matter-labs/zksync-era/issues/3339)) ([aa9575f](https://github.com/matter-labs/zksync-era/commit/aa9575fccbbc941f416d597256442afa974efd0a))
* **contracts:** gateway integration ([#1934](https://github.com/matter-labs/zksync-era/issues/1934)) ([f06cb79](https://github.com/matter-labs/zksync-era/commit/f06cb79883bf320f50089099e0abeb95eaace470))
* **eth-watch:** Change protocol upgrade schema ([#3435](https://github.com/matter-labs/zksync-era/issues/3435)) ([2c778fd](https://github.com/matter-labs/zksync-era/commit/2c778fdd3fcd1e774bcb945f14a640ccf4227a2f))
* FFLONK support for compressor ([#3359](https://github.com/matter-labs/zksync-era/issues/3359)) ([1a297be](https://github.com/matter-labs/zksync-era/commit/1a297bedd226c56fc2ba02dc54d79129a271a1eb))
* Support stable compiler for VM (and some other crates) ([#3248](https://github.com/matter-labs/zksync-era/issues/3248)) ([cbee99d](https://github.com/matter-labs/zksync-era/commit/cbee99d8661b38aa6b49784c3934b8070a743fb4))


### Bug Fixes

* added missing quote in prover query ([#3347](https://github.com/matter-labs/zksync-era/issues/3347)) ([668ca51](https://github.com/matter-labs/zksync-era/commit/668ca51f5d52646e64b19b973acec05daa1c6f09))
* Compressor setup data ([#3526](https://github.com/matter-labs/zksync-era/issues/3526)) ([62aea8b](https://github.com/matter-labs/zksync-era/commit/62aea8b4dcd986587de2cf17979e1042307d6b3e))
* **prover:** Create reqwest client only once ([#3324](https://github.com/matter-labs/zksync-era/issues/3324)) ([40f8123](https://github.com/matter-labs/zksync-era/commit/40f8123a67970efbba3519f7954f807958a76cff))

## [17.1.1](https://github.com/matter-labs/zksync-era/compare/prover-v17.1.0...prover-v17.1.1) (2024-11-26)


### Bug Fixes

* Add prometheus exporter to circut prover ([#3311](https://github.com/matter-labs/zksync-era/issues/3311)) ([920eba1](https://github.com/matter-labs/zksync-era/commit/920eba15fc64b40f10fcc9f6910d5e7607f3d313))

## [17.1.0](https://github.com/matter-labs/zksync-era/compare/prover-v17.0.0...prover-v17.1.0) (2024-11-18)


### Features

* Add min_replicas for SimpleScaler, apply_min_to_namespace config  ([#3282](https://github.com/matter-labs/zksync-era/issues/3282)) ([bc00c4a](https://github.com/matter-labs/zksync-era/commit/bc00c4a44a212def3cc25567f3b271530d76b6a4))
* allow vm2 tracers to stop execution ([#3183](https://github.com/matter-labs/zksync-era/issues/3183)) ([9dae839](https://github.com/matter-labs/zksync-era/commit/9dae839935d82a1e73be220d17567f3382131039))
* **contract-verifier:** Support Solidity contracts with EVM bytecode in contract verifier ([#3225](https://github.com/matter-labs/zksync-era/issues/3225)) ([8a3a82c](https://github.com/matter-labs/zksync-era/commit/8a3a82ca16479183e96505bc91011fc07bfc6889))
* **prover:** Add cluster name autodetection ([#3227](https://github.com/matter-labs/zksync-era/issues/3227)) ([bd32aec](https://github.com/matter-labs/zksync-era/commit/bd32aecdf982c51202c1a69d12fcf1d878fe6d05))
* **prover:** Add queue metric to report autoscaler view of the queue. ([#3206](https://github.com/matter-labs/zksync-era/issues/3206)) ([2721396](https://github.com/matter-labs/zksync-era/commit/272139690e028d3bdebdb6bcb1824fec23cefd0f))
* ProverJobProcessor & circuit prover ([#3287](https://github.com/matter-labs/zksync-era/issues/3287)) ([98823f9](https://github.com/matter-labs/zksync-era/commit/98823f95c0b95feeb37eb9086cc88d4ac5220904))
* **prover:** Move prover_autoscaler config into crate ([#3222](https://github.com/matter-labs/zksync-era/issues/3222)) ([1b33b5e](https://github.com/matter-labs/zksync-era/commit/1b33b5e9ec04bea0010350798332a90413c482d3))


### Bug Fixes

* **prover:** Remove unneeded dependencies, add default for graceful_shutdown_timeout ([#3242](https://github.com/matter-labs/zksync-era/issues/3242)) ([1bfff0e](https://github.com/matter-labs/zksync-era/commit/1bfff0e007e2fb5a4b4b885cf5c69a5cd290888b))

## [17.0.0](https://github.com/matter-labs/zksync-era/compare/prover-v16.6.0...prover-v17.0.0) (2024-10-31)


### ⚠ BREAKING CHANGES

* force minor bump for provers release -- v25 ([#3208](https://github.com/matter-labs/zksync-era/issues/3208))

### Features

* force minor bump for provers release -- v25 ([#3208](https://github.com/matter-labs/zksync-era/issues/3208)) ([6851e35](https://github.com/matter-labs/zksync-era/commit/6851e353f01cde5d385403dedb68bcae76d06966))

## [16.6.0](https://github.com/matter-labs/zksync-era/compare/prover-v16.5.0...prover-v16.6.0) (2024-10-31)


### Features

* (DB migration) Rename recursion_scheduler_level_vk_hash to snark_wrapper_vk_hash ([#2809](https://github.com/matter-labs/zksync-era/issues/2809)) ([64f9551](https://github.com/matter-labs/zksync-era/commit/64f95514c99f95da2a19a97ff064c29a97efc22f))
* Add  initial version prover_autoscaler ([#2993](https://github.com/matter-labs/zksync-era/issues/2993)) ([ebf9604](https://github.com/matter-labs/zksync-era/commit/ebf9604c5ab2a1cae1ffd2f9c922f35a1d0ad876))
* added seed_peers to consensus global config ([#2920](https://github.com/matter-labs/zksync-era/issues/2920)) ([e9d1d90](https://github.com/matter-labs/zksync-era/commit/e9d1d905f1ce86f9de2cf39d79be4b5aada4a81d))
* attester committees data extractor (BFT-434) ([#2684](https://github.com/matter-labs/zksync-era/issues/2684)) ([92dde03](https://github.com/matter-labs/zksync-era/commit/92dde039ee8a0bc08e2019b7fa6f243a34d9816f))
* Bump crypto and protocol deps ([#2825](https://github.com/matter-labs/zksync-era/issues/2825)) ([a5ffaf1](https://github.com/matter-labs/zksync-era/commit/a5ffaf1b4e291d6f09ba8c1f224f5900665bffc4))
* **circuit_prover:** Add circuit prover ([#2908](https://github.com/matter-labs/zksync-era/issues/2908)) ([48317e6](https://github.com/matter-labs/zksync-era/commit/48317e640a00b016bf7bf782cc94fccaf077ed6d))
* **consensus:** Support for syncing blocks before consensus genesis over p2p network ([#3040](https://github.com/matter-labs/zksync-era/issues/3040)) ([d3edc3d](https://github.com/matter-labs/zksync-era/commit/d3edc3d817c151ed00d4fa822fdae0a746e33356))
* **da-clients:** add secrets ([#2954](https://github.com/matter-labs/zksync-era/issues/2954)) ([f4631e4](https://github.com/matter-labs/zksync-era/commit/f4631e4466de620cc1401b326d864cdb8b48a05d))
* gateway preparation ([#3006](https://github.com/matter-labs/zksync-era/issues/3006)) ([16f2757](https://github.com/matter-labs/zksync-era/commit/16f275756cd28024a6b11ac1ac327eb5b8b446e1))
* Integrate tracers and implement circuits tracer in vm2 ([#2653](https://github.com/matter-labs/zksync-era/issues/2653)) ([87b02e3](https://github.com/matter-labs/zksync-era/commit/87b02e3ab5c1f61d59dd0f0eefa9ec33a7b55488))
* Move prover data to /home/popzxc/workspace/current/zksync-era/prover/data ([#2778](https://github.com/matter-labs/zksync-era/issues/2778)) ([62e4d46](https://github.com/matter-labs/zksync-era/commit/62e4d4619dde9d6bd9102f1410eea75b0e2051c5))
* Prover e2e test ([#2975](https://github.com/matter-labs/zksync-era/issues/2975)) ([0edd796](https://github.com/matter-labs/zksync-era/commit/0edd7962429b3530ae751bd7cc947c97193dd0ca))
* **prover:** add CLI option to run prover with max allocation ([#2794](https://github.com/matter-labs/zksync-era/issues/2794)) ([35e4cae](https://github.com/matter-labs/zksync-era/commit/35e4cae29314fa98ce356a875e08b3e869a31036))
* **prover:** Add endpoint to PJM to get queue reports ([#2918](https://github.com/matter-labs/zksync-era/issues/2918)) ([2cec83f](https://github.com/matter-labs/zksync-era/commit/2cec83f26e0b9309387135ca43718af4fcd6f6b1))
* **prover:** Add error to panic message of prover ([#2807](https://github.com/matter-labs/zksync-era/issues/2807)) ([6e057eb](https://github.com/matter-labs/zksync-era/commit/6e057ebf277e0cbc7964079c01ef0348e006a53b))
* **prover:** Add min_provers and dry_run features. Improve metrics and test. ([#3129](https://github.com/matter-labs/zksync-era/issues/3129)) ([7c28964](https://github.com/matter-labs/zksync-era/commit/7c289649b7b3c418c7193a35b51c264cf4970f3c))
* **prover:** Add scale failure events watching and pods eviction. ([#3175](https://github.com/matter-labs/zksync-era/issues/3175)) ([dd166f8](https://github.com/matter-labs/zksync-era/commit/dd166f887b11a8dfb039a0030dda923c481f67af))
* **prover:** Add sending scale requests for Scaler targets ([#3194](https://github.com/matter-labs/zksync-era/issues/3194)) ([767c5bc](https://github.com/matter-labs/zksync-era/commit/767c5bc6a62c402c099abe93b7dbecbb59e4acb7))
* **prover:** Add support for scaling WGs and compressor ([#3179](https://github.com/matter-labs/zksync-era/issues/3179)) ([c41db9e](https://github.com/matter-labs/zksync-era/commit/c41db9ecec1c21b80969604f703ac6990f6f3434))
* **prover:** Autoscaler sends scale request to appropriate agents. ([#3150](https://github.com/matter-labs/zksync-era/issues/3150)) ([bfedac0](https://github.com/matter-labs/zksync-era/commit/bfedac03b53055c6e2d5fa6bd6bdc78e2cb1724c))
* **prover:** Extract keystore into a separate crate ([#2797](https://github.com/matter-labs/zksync-era/issues/2797)) ([e239260](https://github.com/matter-labs/zksync-era/commit/e239260d77b55fcce0b1f485029762a605cdb6d0))
* **prover:** Optimize setup keys loading ([#2847](https://github.com/matter-labs/zksync-era/issues/2847)) ([19887ef](https://github.com/matter-labs/zksync-era/commit/19887ef21a8bbd26977353f8ee277b711850dfd2))
* **prover:** Refactor WitnessGenerator ([#2845](https://github.com/matter-labs/zksync-era/issues/2845)) ([934634b](https://github.com/matter-labs/zksync-era/commit/934634b149377c730ec39e904508c40628ff4019))
* **prover:** Update witness generator to zkevm_test_harness 0.150.6 ([#3029](https://github.com/matter-labs/zksync-era/issues/3029)) ([2151c28](https://github.com/matter-labs/zksync-era/commit/2151c2832498ca6e7ee1eee0bfdf6a0568345fee))
* **prover:** Use query macro instead string literals for queries ([#2930](https://github.com/matter-labs/zksync-era/issues/2930)) ([1cf959d](https://github.com/matter-labs/zksync-era/commit/1cf959da12d2b6369f34a67ccc2575b4b173d75a))
* **prover:** WG refactoring [#3](https://github.com/matter-labs/zksync-era/issues/3) ([#2942](https://github.com/matter-labs/zksync-era/issues/2942)) ([df68762](https://github.com/matter-labs/zksync-era/commit/df6876221936a44fa2fb8c80c01d043d229621fc))
* **prover:** WitnessGenerator refactoring [#2](https://github.com/matter-labs/zksync-era/issues/2) ([#2899](https://github.com/matter-labs/zksync-era/issues/2899)) ([36e5340](https://github.com/matter-labs/zksync-era/commit/36e534091f73f4e3ce86e322fb20842cda6a6b61))
* Refactor metrics/make API use binaries ([#2735](https://github.com/matter-labs/zksync-era/issues/2735)) ([8ed086a](https://github.com/matter-labs/zksync-era/commit/8ed086afecfcad30bfda44fc4d29a00beea71cca))
* Remove prover db from house keeper ([#2795](https://github.com/matter-labs/zksync-era/issues/2795)) ([85b7346](https://github.com/matter-labs/zksync-era/commit/85b734664b4306e988da07005860a7ea0fb7d22d))
* **tee:** use hex serialization for RPC responses ([#2887](https://github.com/matter-labs/zksync-era/issues/2887)) ([abe0440](https://github.com/matter-labs/zksync-era/commit/abe0440811ae4daf4a0f307922a282e9664308e0))
* **utils:** Rework locate_workspace, introduce Workspace type ([#2830](https://github.com/matter-labs/zksync-era/issues/2830)) ([d256092](https://github.com/matter-labs/zksync-era/commit/d2560928cc67b40a97a5497ac8542915bf6f91a9))
* vm2 tracers can access storage ([#3114](https://github.com/matter-labs/zksync-era/issues/3114)) ([e466b52](https://github.com/matter-labs/zksync-era/commit/e466b52948e3c4ed1cb5af4fd999a52028e4d216))
* **vm:** Do not panic on VM divergence ([#2705](https://github.com/matter-labs/zksync-era/issues/2705)) ([7aa5721](https://github.com/matter-labs/zksync-era/commit/7aa5721d22e253d05d369a60d5bcacbf52021c48))
* **vm:** EVM emulator support – base ([#2979](https://github.com/matter-labs/zksync-era/issues/2979)) ([deafa46](https://github.com/matter-labs/zksync-era/commit/deafa460715334a77edf9fe8aa76fa90029342c4))
* **vm:** Extract batch executor to separate crate ([#2702](https://github.com/matter-labs/zksync-era/issues/2702)) ([b82dfa4](https://github.com/matter-labs/zksync-era/commit/b82dfa4d29fce107223c3638fe490b5cb0f28d8c))
* **zk_toolbox:** `zk_supervisor prover` subcommand ([#2820](https://github.com/matter-labs/zksync-era/issues/2820)) ([3506731](https://github.com/matter-labs/zksync-era/commit/3506731d1702bdec8c6b5b41cabca9a257f0269b))
* **zk_toolbox:** Add external_node consensus support ([#2821](https://github.com/matter-labs/zksync-era/issues/2821)) ([4a10d7d](https://github.com/matter-labs/zksync-era/commit/4a10d7d9554d6c1aa2f4fc46557d40baaad8ff2f))
* **zk_toolbox:** Add SQL format for zk supervisor ([#2950](https://github.com/matter-labs/zksync-era/issues/2950)) ([540e5d7](https://github.com/matter-labs/zksync-era/commit/540e5d7554f54e80d52f1bfae37e03ca8f787baf))
* **zk_toolbox:** deploy legacy bridge ([#2837](https://github.com/matter-labs/zksync-era/issues/2837)) ([93b4e08](https://github.com/matter-labs/zksync-era/commit/93b4e08257802d11108870d867dd59fa35e52733))
* **zk_toolbox:** Redesign zk_toolbox commands ([#3003](https://github.com/matter-labs/zksync-era/issues/3003)) ([114834f](https://github.com/matter-labs/zksync-era/commit/114834f357421c62d596a1954fac8ce615cfde49))
* **zkstack_cli:** Build dependencies at zkstack build time ([#3157](https://github.com/matter-labs/zksync-era/issues/3157)) ([724d9a9](https://github.com/matter-labs/zksync-era/commit/724d9a9c7f2127263845b640c843e751fd3c21ae))


### Bug Fixes

* allow compilation under current toolchain ([#3176](https://github.com/matter-labs/zksync-era/issues/3176)) ([89eadd3](https://github.com/matter-labs/zksync-era/commit/89eadd353c4fb84bb815ae56b29f4ff3467b80f3))
* **api:** Return correct flat call tracer ([#2917](https://github.com/matter-labs/zksync-era/issues/2917)) ([218646a](https://github.com/matter-labs/zksync-era/commit/218646aa1c56200f4ffee99b7f83366e2689354f))
* count SECP256 precompile to account validation gas limit as well ([#2859](https://github.com/matter-labs/zksync-era/issues/2859)) ([fee0c2a](https://github.com/matter-labs/zksync-era/commit/fee0c2ad08a5ab4a04252765b367eb9fbb1f3db7))
* Fix Doc lint. ([#3158](https://github.com/matter-labs/zksync-era/issues/3158)) ([c79949b](https://github.com/matter-labs/zksync-era/commit/c79949b8ffde9867b961192afa6c815b44865ae4))
* ignore unknown fields in rpc json response ([#2962](https://github.com/matter-labs/zksync-era/issues/2962)) ([692ea73](https://github.com/matter-labs/zksync-era/commit/692ea73f75a5fb9db2b4ac33ad24d20568638742))
* **prover:** Do not exit on missing watcher data. ([#3119](https://github.com/matter-labs/zksync-era/issues/3119)) ([76ed6d9](https://github.com/matter-labs/zksync-era/commit/76ed6d966051c56f8e894c18461c5ea284b1a74b))
* **prover:** fix setup_metadata_to_setup_data_key ([#2875](https://github.com/matter-labs/zksync-era/issues/2875)) ([4ae5a93](https://github.com/matter-labs/zksync-era/commit/4ae5a93e9e96cd0cd529baf9ffa78c1b21a9c4b1))
* **prover:** Run for zero queue to allow scaling down to 0 ([#3115](https://github.com/matter-labs/zksync-era/issues/3115)) ([bbe1919](https://github.com/matter-labs/zksync-era/commit/bbe191937fa5c5711a7164fd4f0c2ae65cda0833))
* **tee_verifier:** correctly initialize storage for re-execution ([#3017](https://github.com/matter-labs/zksync-era/issues/3017)) ([9d88373](https://github.com/matter-labs/zksync-era/commit/9d88373f1b745c489e98e5ef542644a70e815498))
* **vm:** Prepare new VM for use in API server and fix divergences ([#2994](https://github.com/matter-labs/zksync-era/issues/2994)) ([741b77e](https://github.com/matter-labs/zksync-era/commit/741b77e080f75c6a93d3ee779b1c9ce4297618f9))

## [16.5.0](https://github.com/matter-labs/zksync-era/compare/prover-v16.4.0...prover-v16.5.0) (2024-08-28)


### Features

* **prover_cli:** Add test for status, l1 and config commands. ([#2263](https://github.com/matter-labs/zksync-era/issues/2263)) ([6a2e3b0](https://github.com/matter-labs/zksync-era/commit/6a2e3b05b7d9c9e8b476fb207631c2285e1bd881))
* **prover_cli:** Stuck status ([#2441](https://github.com/matter-labs/zksync-era/issues/2441)) ([232a817](https://github.com/matter-labs/zksync-era/commit/232a817a73fa842ca4b3be419bc775c85204901e))
* **prover:** Add ProverJobMonitor ([#2666](https://github.com/matter-labs/zksync-era/issues/2666)) ([e22cfb6](https://github.com/matter-labs/zksync-era/commit/e22cfb6cffd2c4b2ad1ec3f3f433616fcd738511))
* **prover:** parallelized memory queues simulation in BWG ([#2652](https://github.com/matter-labs/zksync-era/issues/2652)) ([b4ffcd2](https://github.com/matter-labs/zksync-era/commit/b4ffcd237ee594fc659ccfa96668868f5a87d5e3))
* Provide easy prover setup ([#2683](https://github.com/matter-labs/zksync-era/issues/2683)) ([30edda4](https://github.com/matter-labs/zksync-era/commit/30edda404193938fbd55815bed164b5321d7c642))


### Bug Fixes

* **prover_cli:** Remove congif file check ([#2695](https://github.com/matter-labs/zksync-era/issues/2695)) ([2f456f0](https://github.com/matter-labs/zksync-era/commit/2f456f05937dec62d6a10cec8c948a2915650b92))
* **prover_cli:** Update prover cli README ([#2700](https://github.com/matter-labs/zksync-era/issues/2700)) ([5a9bbb3](https://github.com/matter-labs/zksync-era/commit/5a9bbb3ccf900cea738290ceed2b1ed78908990c))
* **prover:** change bucket for RAM permutation witnesses ([#2672](https://github.com/matter-labs/zksync-era/issues/2672)) ([8b4cbf4](https://github.com/matter-labs/zksync-era/commit/8b4cbf43e52203aac829324aa48450575b70c656))
* **prover:** fail when fri prover job is not found ([#2711](https://github.com/matter-labs/zksync-era/issues/2711)) ([8776875](https://github.com/matter-labs/zksync-era/commit/87768755e8653e4be5f29945b56fd05a5246d5a8))
* **prover:** Revert use of spawn_blocking in LWG/NWG ([#2682](https://github.com/matter-labs/zksync-era/issues/2682)) ([edfcc7d](https://github.com/matter-labs/zksync-era/commit/edfcc7dbb7fb60f0f42fff4f3d350974128127b4))
* **prover:** speed up LWG and NWG ([#2661](https://github.com/matter-labs/zksync-era/issues/2661)) ([6243399](https://github.com/matter-labs/zksync-era/commit/6243399a9ebee88a80fbc6c7e794519712f6e955))
* **vm:** Fix used bytecodes divergence ([#2741](https://github.com/matter-labs/zksync-era/issues/2741)) ([923e33e](https://github.com/matter-labs/zksync-era/commit/923e33e81bba83f72b97ca9590c5cdf2da2a311b))

## [16.4.0](https://github.com/matter-labs/zksync-era/compare/prover-v16.3.0...prover-v16.4.0) (2024-08-16)


### Features

* Bump harness & gpu deps ([#2634](https://github.com/matter-labs/zksync-era/issues/2634)) ([2a7d566](https://github.com/matter-labs/zksync-era/commit/2a7d566ffeb63dc0a038d6b38cbda6bef7c7b105))
* Poll the main node API for attestation status - relaxed (BFT-496) ([#2583](https://github.com/matter-labs/zksync-era/issues/2583)) ([b45aa91](https://github.com/matter-labs/zksync-era/commit/b45aa9168dd66d07ca61c8bb4c01f73dda822040))
* **vlog:** Report observability config, flush, and shutdown ([#2622](https://github.com/matter-labs/zksync-era/issues/2622)) ([e23e661](https://github.com/matter-labs/zksync-era/commit/e23e6611731835ef3abd34f3f9867f9dc533eb21))
* **vm:** Extract VM interface to separate crate ([#2638](https://github.com/matter-labs/zksync-era/issues/2638)) ([cb9ac4e](https://github.com/matter-labs/zksync-era/commit/cb9ac4e59fd16e6c125586bc02ef90e3b97ff80b))
* **vm:** Fast VM integration ([#1949](https://github.com/matter-labs/zksync-era/issues/1949)) ([b752a54](https://github.com/matter-labs/zksync-era/commit/b752a54bebe6eb3bf0bea044996f5116cc5dc4e2))


### Bug Fixes

* Bump prover dependencies & rust toolchain ([#2600](https://github.com/matter-labs/zksync-era/issues/2600)) ([849c6a5](https://github.com/matter-labs/zksync-era/commit/849c6a5dcd095e8fead0630a2a403f282c26a2aa))
* **prover:** Fix NWG ([#2590](https://github.com/matter-labs/zksync-era/issues/2590)) ([9b58ae9](https://github.com/matter-labs/zksync-era/commit/9b58ae97875455d58d42fe203cfb1f51cb270f62))
* **prover:** Updated README.md ([#2604](https://github.com/matter-labs/zksync-era/issues/2604)) ([be9f357](https://github.com/matter-labs/zksync-era/commit/be9f357099ed281892c1ff4618514fc7c25f9b59))

## [16.3.0](https://github.com/matter-labs/zksync-era/compare/prover-v16.2.0...prover-v16.3.0) (2024-08-07)


### Features

* Poll the main node API for attestation status - relaxed (BFT-496) ([#2583](https://github.com/matter-labs/zksync-era/issues/2583)) ([b45aa91](https://github.com/matter-labs/zksync-era/commit/b45aa9168dd66d07ca61c8bb4c01f73dda822040))


### Bug Fixes

* **prover:** Fix NWG ([#2590](https://github.com/matter-labs/zksync-era/issues/2590)) ([9b58ae9](https://github.com/matter-labs/zksync-era/commit/9b58ae97875455d58d42fe203cfb1f51cb270f62))

## [16.2.0](https://github.com/matter-labs/zksync-era/compare/prover-v16.1.0...prover-v16.2.0) (2024-08-02)


### Features

* **configs:** Do not panic if config is only partially filled ([#2545](https://github.com/matter-labs/zksync-era/issues/2545)) ([db13fe3](https://github.com/matter-labs/zksync-era/commit/db13fe3550598c69f59cd66b4bb9618ebea041ca))
* Introduce more tracing instrumentation ([#2523](https://github.com/matter-labs/zksync-era/issues/2523)) ([79d407a](https://github.com/matter-labs/zksync-era/commit/79d407ac47ac51667196aa2cd028d05b1622130f))
* New prover documentation ([#2466](https://github.com/matter-labs/zksync-era/issues/2466)) ([1b61d07](https://github.com/matter-labs/zksync-era/commit/1b61d0797062ab8b0aa2c1e92b23a3a0d8fd2c61))
* optimize LWG and NWG ([#2512](https://github.com/matter-labs/zksync-era/issues/2512)) ([0d00650](https://github.com/matter-labs/zksync-era/commit/0d00650f3e97248617b88a7e082c515ac48d5d5b))
* Poll the main node for the next batch to sign (BFT-496) ([#2544](https://github.com/matter-labs/zksync-era/issues/2544)) ([22cf820](https://github.com/matter-labs/zksync-era/commit/22cf820abbd14b852dffe60f6b564713fe4c8919))
* Remove CI and docker images for CPU prover & compressor ([#2540](https://github.com/matter-labs/zksync-era/issues/2540)) ([0dda805](https://github.com/matter-labs/zksync-era/commit/0dda80532baa77f6ac8b8d9cb7e3712c2eca22f6))
* Remove unused VKs, add docs for BWG ([#2468](https://github.com/matter-labs/zksync-era/issues/2468)) ([2fa6bf0](https://github.com/matter-labs/zksync-era/commit/2fa6bf0ffa5d7a5ff62d595a0efeff9dcd9e5a1a))
* Support sending logs via OTLP ([#2556](https://github.com/matter-labs/zksync-era/issues/2556)) ([1d206c0](https://github.com/matter-labs/zksync-era/commit/1d206c0af8f28eb00eb1498d6f2cdbb45ffef72a))
* Update to consensus 0.1.0-rc.4 (BFT-486) ([#2475](https://github.com/matter-labs/zksync-era/issues/2475)) ([ff6b10c](https://github.com/matter-labs/zksync-era/commit/ff6b10c4a994cf70297a034202bcb55152748cba))
* **vlog:** New vlog interface + opentelemtry improvements ([#2472](https://github.com/matter-labs/zksync-era/issues/2472)) ([c0815cd](https://github.com/matter-labs/zksync-era/commit/c0815cdaf878afcd9c41dddd9fe56bcf8d910633))
* **witness_vector_generator:** Make it possible to run multiple wvg instances in one binary ([#2493](https://github.com/matter-labs/zksync-era/issues/2493)) ([572ad40](https://github.com/matter-labs/zksync-era/commit/572ad40175784b62d61aa46dcf901a7e2f6ff8f5))
* **zk_toolbox:** use configs from the main repo ([#2470](https://github.com/matter-labs/zksync-era/issues/2470)) ([4222d13](https://github.com/matter-labs/zksync-era/commit/4222d135b62eb4de103c4aebb35e9c302d94ad63))


### Bug Fixes

* **prover:** Parallelize circuit metadata uploading for BWG ([#2520](https://github.com/matter-labs/zksync-era/issues/2520)) ([f49720f](https://github.com/matter-labs/zksync-era/commit/f49720fbafdab8f102d908b2be3fa869482a92fa))
* **prover:** Reduce database connection overhead for BWG ([#2543](https://github.com/matter-labs/zksync-era/issues/2543)) ([c2439e9](https://github.com/matter-labs/zksync-era/commit/c2439e904e2520d22d9ed58b4dd6583da91fb4e3))
* **witness_generator:** Only spawn 1 prometheus exporter per witness generator ([#2492](https://github.com/matter-labs/zksync-era/issues/2492)) ([b9cb222](https://github.com/matter-labs/zksync-era/commit/b9cb222ed7c27daa73ce20b8911afd94106e70a6))


### Reverts

* "feat: Poll the main node for the next batch to sign (BFT-496)" ([#2574](https://github.com/matter-labs/zksync-era/issues/2574)) ([72d3be8](https://github.com/matter-labs/zksync-era/commit/72d3be87efcb059f70b4633cddd707346612c4db))

## [16.1.0](https://github.com/matter-labs/zksync-era/compare/prover-v16.0.0...prover-v16.1.0) (2024-07-24)


### Features

* **prover:** Make it possible to run prover out of GCP ([#2448](https://github.com/matter-labs/zksync-era/issues/2448)) ([c9da549](https://github.com/matter-labs/zksync-era/commit/c9da5497e2aa9d85f204ab7b74fefcfe941793ff))
* remove leftovers after BWIP ([#2456](https://github.com/matter-labs/zksync-era/issues/2456)) ([990676c](https://github.com/matter-labs/zksync-era/commit/990676c5f84afd2ff8cd337f495c82e8d1f305a4))


### Bug Fixes

* **prover:** BWG optimizations ([#2469](https://github.com/matter-labs/zksync-era/issues/2469)) ([d8851c8](https://github.com/matter-labs/zksync-era/commit/d8851c8af2cd4b595f4edb9c36c81e2310835a77))

## [16.0.0](https://github.com/matter-labs/zksync-era/compare/prover-v15.1.0...prover-v16.0.0) (2024-07-11)


### ⚠ BREAKING CHANGES

* **prover:** Bump prover protocol patch ([#2428](https://github.com/matter-labs/zksync-era/issues/2428))

### Features

* L1 batch signing (BFT-474) ([#2414](https://github.com/matter-labs/zksync-era/issues/2414)) ([ab699db](https://github.com/matter-labs/zksync-era/commit/ab699dbe8cffa8bd291d6054579061b47fd4aa0e))
* **prover:** Bump prover protocol patch ([#2428](https://github.com/matter-labs/zksync-era/issues/2428)) ([1dffae9](https://github.com/matter-labs/zksync-era/commit/1dffae90d0d6a56434bb076135ac2a957ab20b83))

## [15.1.0](https://github.com/matter-labs/zksync-era/compare/prover-v15.0.0...prover-v15.1.0) (2024-07-10)


### Features

* **api:** Retry `read_value` ([#2352](https://github.com/matter-labs/zksync-era/issues/2352)) ([256a43c](https://github.com/matter-labs/zksync-era/commit/256a43cdd01619b89e348419bc361454ba4fdabb))
* Base Token Fundamentals ([#2204](https://github.com/matter-labs/zksync-era/issues/2204)) ([39709f5](https://github.com/matter-labs/zksync-era/commit/39709f58071ac77bfd447145e1c3342b7da70560))
* BWIP ([#2258](https://github.com/matter-labs/zksync-era/issues/2258)) ([75bdfcc](https://github.com/matter-labs/zksync-era/commit/75bdfcc0ef4a99d93ac152db12a59ef2b2af0d27))
* change `zkSync` occurences to `ZKsync` ([#2227](https://github.com/matter-labs/zksync-era/issues/2227)) ([0b4104d](https://github.com/matter-labs/zksync-era/commit/0b4104dbb996ec6333619ea05f3a99e6d4f3b8fa))
* **config:** Make getaway_url optional ([#2412](https://github.com/matter-labs/zksync-era/issues/2412)) ([200bc82](https://github.com/matter-labs/zksync-era/commit/200bc825032b18ad9d8f3f49d4eb7cb0e1b5b645))
* consensus support for pruning (BFT-473) ([#2334](https://github.com/matter-labs/zksync-era/issues/2334)) ([abc4256](https://github.com/matter-labs/zksync-era/commit/abc4256570b899e2b47ed8362e69ae0150247490))
* **contract-verifier:** Add file based config for contract verifier ([#2415](https://github.com/matter-labs/zksync-era/issues/2415)) ([f4410e3](https://github.com/matter-labs/zksync-era/commit/f4410e3254dafdfe400e1c2c420f664ba951e2cd))
* **en:** file based configs for en ([#2110](https://github.com/matter-labs/zksync-era/issues/2110)) ([7940fa3](https://github.com/matter-labs/zksync-era/commit/7940fa32a27ee4de43753c7083f92ca8c2ebe86b))
* Make all core workspace crate names start with zksync_ ([#2294](https://github.com/matter-labs/zksync-era/issues/2294)) ([8861f29](https://github.com/matter-labs/zksync-era/commit/8861f2994b674be3c654511416452c0a555d0f73))
* Minimal External API Fetcher ([#2383](https://github.com/matter-labs/zksync-era/issues/2383)) ([9f255c0](https://github.com/matter-labs/zksync-era/commit/9f255c073cfdab60832fcf9a6d3a4a9258641ef3))
* **prover:** Add file based config for compressor ([#2353](https://github.com/matter-labs/zksync-era/issues/2353)) ([1d6f87d](https://github.com/matter-labs/zksync-era/commit/1d6f87dde88ee1b09e42d57a8d285eb257068bae))
* **prover:** Add file based config for prover fri ([#2184](https://github.com/matter-labs/zksync-era/issues/2184)) ([f851615](https://github.com/matter-labs/zksync-era/commit/f851615ab3753bb9353fd4456a6e49d55d67c626))
* **prover:** Add file based config for witness vector generator ([#2337](https://github.com/matter-labs/zksync-era/issues/2337)) ([f86eb13](https://github.com/matter-labs/zksync-era/commit/f86eb132aa2f5b75c45a65189e9664d3d1e2682f))
* **prover:** Add file based config support for vk-setup-data-generator-server-fri ([#2371](https://github.com/matter-labs/zksync-era/issues/2371)) ([b0e72c9](https://github.com/matter-labs/zksync-era/commit/b0e72c9ecbb659850f7dd27386984b99877e7a5c))
* **prover:** Add prometheus port to witness generator config ([#2385](https://github.com/matter-labs/zksync-era/issues/2385)) ([d0e1add](https://github.com/matter-labs/zksync-era/commit/d0e1addfccf6b5d3b21facd6bb74455f098f0177))
* **prover:** Add prover_cli stats command ([#2362](https://github.com/matter-labs/zksync-era/issues/2362)) ([fe65319](https://github.com/matter-labs/zksync-era/commit/fe65319da0f26ca45e95f067c1e8b97cf7874c45))
* Remove cached commitments, add BWIP to docs ([#2400](https://github.com/matter-labs/zksync-era/issues/2400)) ([e652e4d](https://github.com/matter-labs/zksync-era/commit/e652e4d8548570d060fa4c901c75745b7ea6b296))
* Remove initialize_components function ([#2284](https://github.com/matter-labs/zksync-era/issues/2284)) ([0a38891](https://github.com/matter-labs/zksync-era/commit/0a388911914bfcf58785e394db9d5ddce3afdef0))
* snark proof is already verified inside wrap_proof function ([#1903](https://github.com/matter-labs/zksync-era/issues/1903)) ([2c8cf35](https://github.com/matter-labs/zksync-era/commit/2c8cf35bc1b03f82073bad9e28ebb409d48bad98))
* Switch to using crates.io deps ([#2409](https://github.com/matter-labs/zksync-era/issues/2409)) ([27fabaf](https://github.com/matter-labs/zksync-era/commit/27fabafbec66bf4cb65c4fa9e3fab4c3c981d0f2))
* **tee:** TEE Prover Gateway ([#2333](https://github.com/matter-labs/zksync-era/issues/2333)) ([f8df34d](https://github.com/matter-labs/zksync-era/commit/f8df34d9bff5e165fe40d4f67afa582a84038303))
* upgraded encoding of transactions in consensus Payload. ([#2245](https://github.com/matter-labs/zksync-era/issues/2245)) ([cb6a6c8](https://github.com/matter-labs/zksync-era/commit/cb6a6c88de54806d0f4ae4af7ea873a911605780))
* Validium with DA ([#2010](https://github.com/matter-labs/zksync-era/issues/2010)) ([fe03d0e](https://github.com/matter-labs/zksync-era/commit/fe03d0e254a98fea60ecb7485a7de9e7fdecaee1))
* **zk_toolbox:** Add prover run ([#2272](https://github.com/matter-labs/zksync-era/issues/2272)) ([598ef7b](https://github.com/matter-labs/zksync-era/commit/598ef7b73cf141007d2cf031b21fce4744eec44f))


### Bug Fixes

* Fix rustls setup for jsonrpsee clients ([#2417](https://github.com/matter-labs/zksync-era/issues/2417)) ([a040f09](https://github.com/matter-labs/zksync-era/commit/a040f099cd9863d47d49cbdb3360e53a82e0423e))
* **proof_compressor:** Fix backward compatibility ([#2356](https://github.com/matter-labs/zksync-era/issues/2356)) ([76508c4](https://github.com/matter-labs/zksync-era/commit/76508c42e83770ee50a0a9ced03b437687d383cd))
* prover Cargo.lock ([#2280](https://github.com/matter-labs/zksync-era/issues/2280)) ([05c6f35](https://github.com/matter-labs/zksync-era/commit/05c6f357eee591262e3ddd870fcde0fe50ce05cc))
* **prover_cli:** Fix Minor Bugs in Prover CLI ([#2264](https://github.com/matter-labs/zksync-era/issues/2264)) ([440f2a7](https://github.com/matter-labs/zksync-era/commit/440f2a7ae0def22bab65c4bb5c531b3234841b76))
* **prover_cli:** Remove outdated fix for circuit id in node wg ([#2248](https://github.com/matter-labs/zksync-era/issues/2248)) ([db8e71b](https://github.com/matter-labs/zksync-era/commit/db8e71b55393b3d0e419886b62712b61305ac030))

## [15.0.0](https://github.com/matter-labs/zksync-era/compare/prover-v14.5.0...prover-v15.0.0) (2024-06-14)


### ⚠ BREAKING CHANGES

* updated boojum and nightly rust compiler ([#2126](https://github.com/matter-labs/zksync-era/issues/2126))

### Features

* added debug_proof to prover_cli ([#2052](https://github.com/matter-labs/zksync-era/issues/2052)) ([b1ad01b](https://github.com/matter-labs/zksync-era/commit/b1ad01b50392a0ee241c2263ac22bb3258fae2d7))
* faster & cleaner VK generation ([#2084](https://github.com/matter-labs/zksync-era/issues/2084)) ([89c8cac](https://github.com/matter-labs/zksync-era/commit/89c8cac6a747b3e05529218091b90ceb8e520c7a))
* **node:** Move some stuff around ([#2151](https://github.com/matter-labs/zksync-era/issues/2151)) ([bad5a6c](https://github.com/matter-labs/zksync-era/commit/bad5a6c0ec2e166235418a2796b6ccf6f8b3b05f))
* **object-store:** Allow caching object store objects locally ([#2153](https://github.com/matter-labs/zksync-era/issues/2153)) ([6c6e65c](https://github.com/matter-labs/zksync-era/commit/6c6e65ce646bcb4ed9ba8b2dd6be676bb6e66324))
* **proof_data_handler:** add new endpoints to the TEE prover interface API ([#1993](https://github.com/matter-labs/zksync-era/issues/1993)) ([eca98cc](https://github.com/matter-labs/zksync-era/commit/eca98cceeb74a979040279caaf1d05d1fdf1b90c))
* **prover:** Add file based config for fri prover gateway ([#2150](https://github.com/matter-labs/zksync-era/issues/2150)) ([81ffc6a](https://github.com/matter-labs/zksync-era/commit/81ffc6a753fb72747c01ddc8a37211bf6a8a1a27))
* **prover:** file based configs for witness generator ([#2161](https://github.com/matter-labs/zksync-era/issues/2161)) ([24b8f93](https://github.com/matter-labs/zksync-era/commit/24b8f93fbcc537792a7615f34bce8b6702a55ccd))
* support debugging of recursive circuits in prover_cli ([#2217](https://github.com/matter-labs/zksync-era/issues/2217)) ([7d2e12d](https://github.com/matter-labs/zksync-era/commit/7d2e12d80db072be1952102183648b95a48834c6))
* updated boojum and nightly rust compiler ([#2126](https://github.com/matter-labs/zksync-era/issues/2126)) ([9e39f13](https://github.com/matter-labs/zksync-era/commit/9e39f13c29788e66645ea57f623555c4b36b8aff))
* verification of L1Batch witness (BFT-471) - attempt 2 ([#2232](https://github.com/matter-labs/zksync-era/issues/2232)) ([dbcf3c6](https://github.com/matter-labs/zksync-era/commit/dbcf3c6d02a6bfb9197bf4278f296632b0fd7d66))
* verification of L1Batch witness (BFT-471) ([#2019](https://github.com/matter-labs/zksync-era/issues/2019)) ([6cc5455](https://github.com/matter-labs/zksync-era/commit/6cc54555972804be4cd2ca118f0e425c490fbfca))


### Bug Fixes

* **config:** Split object stores ([#2187](https://github.com/matter-labs/zksync-era/issues/2187)) ([9bcdabc](https://github.com/matter-labs/zksync-era/commit/9bcdabcaa8462ae19da1688052a7a78fa4108298))
* **prover_cli:** Fix delete command ([#2119](https://github.com/matter-labs/zksync-era/issues/2119)) ([214f981](https://github.com/matter-labs/zksync-era/commit/214f981880ca1ea879e805f8fc392f5c422be08d))
* **prover_cli:** Fix the issues with `home` path ([#2104](https://github.com/matter-labs/zksync-era/issues/2104)) ([1e18af2](https://github.com/matter-labs/zksync-era/commit/1e18af20d082065f269c6cad65bee99363e2d770))
* **prover:** config ([#2165](https://github.com/matter-labs/zksync-era/issues/2165)) ([e5daf8e](https://github.com/matter-labs/zksync-era/commit/e5daf8e8358eff65963d6a1b2294d0bd1fccab89))
* **prover:** Disallow state changes from successful ([#2233](https://github.com/matter-labs/zksync-era/issues/2233)) ([2488a76](https://github.com/matter-labs/zksync-era/commit/2488a767a362ea3b40a348ae9822bed77d4b8de9))
* Treat 502s and 503s as transient for GCS OS ([#2202](https://github.com/matter-labs/zksync-era/issues/2202)) ([0a12c52](https://github.com/matter-labs/zksync-era/commit/0a12c5224b0b6b6d937311e6d6d81c26b03b1d9d))


### Reverts

* verification of L1Batch witness (BFT-471) ([#2230](https://github.com/matter-labs/zksync-era/issues/2230)) ([227e101](https://github.com/matter-labs/zksync-era/commit/227e10180396fbb54a2e99cab775f13bc93745f3))

## [14.5.0](https://github.com/matter-labs/zksync-era/compare/prover-v14.4.0...prover-v14.5.0) (2024-06-04)


### Features

* update VKs and bump cargo.lock ([#2112](https://github.com/matter-labs/zksync-era/issues/2112)) ([6510317](https://github.com/matter-labs/zksync-era/commit/65103173085a0b500a626cb8179fad77ee97fadd))
* use semver for metrics, move constants to prover workspace ([#2098](https://github.com/matter-labs/zksync-era/issues/2098)) ([7a50a9f](https://github.com/matter-labs/zksync-era/commit/7a50a9f79e516ec150d1f30b9f1c781a5523375b))


### Bug Fixes

* **block-reverter:** Fix reverting snapshot files ([#2064](https://github.com/matter-labs/zksync-era/issues/2064)) ([17a7e78](https://github.com/matter-labs/zksync-era/commit/17a7e782d9e35eaf38acf920c2326d4037c7781e))
* **house-keeper:** Fix queue size queries ([#2106](https://github.com/matter-labs/zksync-era/issues/2106)) ([183502a](https://github.com/matter-labs/zksync-era/commit/183502a17eb47a747f50b6a9d38ab78de984f80e))

## [14.4.0](https://github.com/matter-labs/zksync-era/compare/prover-v14.3.0...prover-v14.4.0) (2024-05-30)


### Features

* Make house keeper emit correct protocol version ([#2062](https://github.com/matter-labs/zksync-era/issues/2062)) ([a58a7e8](https://github.com/matter-labs/zksync-era/commit/a58a7e8ec8599eb957e5693308b789e7ace5c126))
* **pli:** add support for persistent config ([#1907](https://github.com/matter-labs/zksync-era/issues/1907)) ([9d5631c](https://github.com/matter-labs/zksync-era/commit/9d5631cdd330a288335db11a71ecad89ee32a0f4))
* Protocol semantic version ([#2059](https://github.com/matter-labs/zksync-era/issues/2059)) ([3984dcf](https://github.com/matter-labs/zksync-era/commit/3984dcfbdd890f0862c9c0f3e7757fb8b0c8184a))
* **prover:** Add `prover_version` binary. ([#2089](https://github.com/matter-labs/zksync-era/issues/2089)) ([e1822f6](https://github.com/matter-labs/zksync-era/commit/e1822f6ad150a28df75b06b97b9ff01d671b83b6))


### Bug Fixes

* fix null protocol version error ([#2094](https://github.com/matter-labs/zksync-era/issues/2094)) ([aab3a7f](https://github.com/matter-labs/zksync-era/commit/aab3a7ff97870aea155fbc542c4c0f55ee816341))
* fix query for proof compressor metrics ([#2103](https://github.com/matter-labs/zksync-era/issues/2103)) ([d23d24e](https://github.com/matter-labs/zksync-era/commit/d23d24e9e13af052612be81e913da89bc160de4d))
* **prover_dal:** fix `save_prover_protocol_version` query ([#2096](https://github.com/matter-labs/zksync-era/issues/2096)) ([d8dd1ae](https://github.com/matter-labs/zksync-era/commit/d8dd1aedd7b67b09b6d5c0f29ba90069e0c80b4e))

## [14.3.0](https://github.com/matter-labs/zksync-era/compare/prover-v14.2.0...prover-v14.3.0) (2024-05-23)


### Features

* **config:** remove zksync home ([#2022](https://github.com/matter-labs/zksync-era/issues/2022)) ([d08fe81](https://github.com/matter-labs/zksync-era/commit/d08fe81f4ec6c3aaeb5ad98351e44a63e5b100be))
* **prover_cli:** add general status for batch command ([#1953](https://github.com/matter-labs/zksync-era/issues/1953)) ([7b0df3b](https://github.com/matter-labs/zksync-era/commit/7b0df3b22f04f1fdead308ec30572f565b34dd5c))
* **prover:** add GPU feature for compressor ([#1838](https://github.com/matter-labs/zksync-era/issues/1838)) ([e9a2213](https://github.com/matter-labs/zksync-era/commit/e9a2213985928cd3804a3855ccfde6a7d99da238))


### Bug Fixes

* **prover:** Fix path to vk_setup_data_generator_server_fri ([#2025](https://github.com/matter-labs/zksync-era/issues/2025)) ([dbe4d6f](https://github.com/matter-labs/zksync-era/commit/dbe4d6f1724a458e61ab56cd94d17e1ecfa4c207))

## [14.2.0](https://github.com/matter-labs/zksync-era/compare/prover-v14.1.1...prover-v14.2.0) (2024-05-17)


### Features

* Added support for making EN a (non-leader) consensus validator (BFT-426) ([#1905](https://github.com/matter-labs/zksync-era/issues/1905)) ([9973629](https://github.com/matter-labs/zksync-era/commit/9973629e35cec9af9eac81452631a2526dd336a8))
* **configs:** Extract secrets to an additional config  ([#1956](https://github.com/matter-labs/zksync-era/issues/1956)) ([bab4d65](https://github.com/matter-labs/zksync-era/commit/bab4d6579828e484453c84df417550bbaf1013b6))
* **eth-client:** Generalize RPC client ([#1898](https://github.com/matter-labs/zksync-era/issues/1898)) ([a4e099f](https://github.com/matter-labs/zksync-era/commit/a4e099fe961f329ff2d604d657862819732446b4))
* **Prover CLI:** `delete` cmd ([#1802](https://github.com/matter-labs/zksync-era/issues/1802)) ([6e4a92e](https://github.com/matter-labs/zksync-era/commit/6e4a92eb93aacec8641770e15fc6faf6a78faafa))
* **Prover CLI:** `requeue` cmd ([#1719](https://github.com/matter-labs/zksync-era/issues/1719)) ([f722df7](https://github.com/matter-labs/zksync-era/commit/f722df7c0ae429f43d047ff79e24bca39f81230c))
* **Prover CLI:** `status batch --verbose` ([#1899](https://github.com/matter-labs/zksync-era/issues/1899)) ([cf80184](https://github.com/matter-labs/zksync-era/commit/cf80184941a1fc62c3a755b99571d370949d8566))
* **state-keeper:** Parallel l2 block sealing ([#1801](https://github.com/matter-labs/zksync-era/issues/1801)) ([9b06dd8](https://github.com/matter-labs/zksync-era/commit/9b06dd848e85e20f2e94d2a0e858c3f207da5f47))
* tee_verifier_input_producer ([#1860](https://github.com/matter-labs/zksync-era/issues/1860)) ([fea7f16](https://github.com/matter-labs/zksync-era/commit/fea7f165cfb96bf673353ef562fb5c06f3e49736))


### Bug Fixes

* **Prover CLI:** `status batch` bugs ([#1865](https://github.com/matter-labs/zksync-era/issues/1865)) ([09682f2](https://github.com/matter-labs/zksync-era/commit/09682f2951f5f62fa0942057e96f855d78bf67c8))
* **prover:** Bump Cargo.lock and update VKs ([#1959](https://github.com/matter-labs/zksync-era/issues/1959)) ([367baad](https://github.com/matter-labs/zksync-era/commit/367baad77466769e7ad5e517cc78836f1e3c23d3))

## [14.1.1](https://github.com/matter-labs/zksync-era/compare/prover-v14.1.0...prover-v14.1.1) (2024-05-14)


### Bug Fixes

* **core/prover:** Changes to support Validium ([#1910](https://github.com/matter-labs/zksync-era/issues/1910)) ([1cb0dc5](https://github.com/matter-labs/zksync-era/commit/1cb0dc5504d226c55217accca87f2fd75addc917))

## [14.1.0](https://github.com/matter-labs/zksync-era/compare/prover-v14.0.0...prover-v14.1.0) (2024-05-11)


### Features

* **en:** Brush up EN observability config ([#1897](https://github.com/matter-labs/zksync-era/issues/1897)) ([086f768](https://github.com/matter-labs/zksync-era/commit/086f7683307b2b9c6a43cb8cf2c1a8a8874277a7))
* Extract several crates from zksync_core ([#1859](https://github.com/matter-labs/zksync-era/issues/1859)) ([7dcf796](https://github.com/matter-labs/zksync-era/commit/7dcf79606e0f37b468c82b6bdcb374149bc30f34))
* **Prover CLI:** Configuration (with flag) ([#1861](https://github.com/matter-labs/zksync-era/issues/1861)) ([620c880](https://github.com/matter-labs/zksync-era/commit/620c88020363cfc534fe8f6889cb2ed8bba89cdd))
* **Prover CLI:** Initial Docs ([#1862](https://github.com/matter-labs/zksync-era/issues/1862)) ([8b094aa](https://github.com/matter-labs/zksync-era/commit/8b094aa91b89f713fd6f0f5ce0412a340a059650))
* **Prover CLI:** status l1 command ([#1706](https://github.com/matter-labs/zksync-era/issues/1706)) ([8ddd039](https://github.com/matter-labs/zksync-era/commit/8ddd0397bd12742e7784c71dedaa08b075d39cda))
* prover components versioning ([#1660](https://github.com/matter-labs/zksync-era/issues/1660)) ([29a4ffc](https://github.com/matter-labs/zksync-era/commit/29a4ffc6b9420590f32a9e1d1585ebffb95eeb6c))


### Bug Fixes

* 1.5.0 + validium (contracts only) patches ([#1911](https://github.com/matter-labs/zksync-era/issues/1911)) ([bf439f4](https://github.com/matter-labs/zksync-era/commit/bf439f4b382e7fd396ce8f035de8e2a5640f8151))
* Update prover cargo lock ([#1896](https://github.com/matter-labs/zksync-era/issues/1896)) ([2ca01f5](https://github.com/matter-labs/zksync-era/commit/2ca01f5f00459570c210890d85c11345271bbed0))

## [14.0.0](https://github.com/matter-labs/zksync-era/compare/prover-v13.0.0...prover-v14.0.0) (2024-05-06)


### ⚠ BREAKING CHANGES

* **prover:** Protocol Upgrade 1.5.0 ([#1699](https://github.com/matter-labs/zksync-era/issues/1699))
* shared bridge ([#298](https://github.com/matter-labs/zksync-era/issues/298))

### Features

* **config:** Wrap sensitive urls ([#1828](https://github.com/matter-labs/zksync-era/issues/1828)) ([c8ee740](https://github.com/matter-labs/zksync-era/commit/c8ee740a4cc7dc9196d4223397e0bfc9fd8198cf))
* **Prover CLI:** `status batch` command ([#1638](https://github.com/matter-labs/zksync-era/issues/1638)) ([3fd6d65](https://github.com/matter-labs/zksync-era/commit/3fd6d653cad1e783f7a52eead13b322d4c6639a9))
* **prover:** Protocol Upgrade 1.5.0 ([#1699](https://github.com/matter-labs/zksync-era/issues/1699)) ([6a557f7](https://github.com/matter-labs/zksync-era/commit/6a557f7766b727f72195d65e338cc41740cdbdbd))
* **prover:** remove redundant config fields ([#1787](https://github.com/matter-labs/zksync-era/issues/1787)) ([a784ea6](https://github.com/matter-labs/zksync-era/commit/a784ea64c847f31010af0ee71b1e64e9961dc5e1))
* shared bridge ([#298](https://github.com/matter-labs/zksync-era/issues/298)) ([8c3478a](https://github.com/matter-labs/zksync-era/commit/8c3478ae27c78a60c272f68c15d2bd59c99c8391))
* **vm-runner:** implement VM runner storage layer ([#1651](https://github.com/matter-labs/zksync-era/issues/1651)) ([543f9e9](https://github.com/matter-labs/zksync-era/commit/543f9e9397915e893d7b747ceccd9b76f9d571aa))


### Bug Fixes

* **en:** Remove duplicate reorg detector ([#1783](https://github.com/matter-labs/zksync-era/issues/1783)) ([3417941](https://github.com/matter-labs/zksync-era/commit/34179412aa9bb11b8b2809d4028fbc200cf4d712))
* **prover:** Use all 1.5.0 groups for nodes ([#1851](https://github.com/matter-labs/zksync-era/issues/1851)) ([70178e5](https://github.com/matter-labs/zksync-era/commit/70178e589dc8db0d8eeead63b645e8bd9c570966))

## [13.0.0](https://github.com/matter-labs/zksync-era/compare/prover-v12.2.0...prover-v13.0.0) (2024-04-22)


### ⚠ BREAKING CHANGES

* **vm:** 1 5 0 support ([#1508](https://github.com/matter-labs/zksync-era/issues/1508))

### Features

* Archive old prover jobs ([#1516](https://github.com/matter-labs/zksync-era/issues/1516)) ([201476c](https://github.com/matter-labs/zksync-era/commit/201476c8c1869c30605eb2acd462ae1dfe026fd1))
* Archiving of prover in gpu_prover_queue ([#1537](https://github.com/matter-labs/zksync-era/issues/1537)) ([a970629](https://github.com/matter-labs/zksync-era/commit/a9706294fe740cbc9af37eef8d968584a3ec4859))
* **configs:** Implement new format of configs and implement protobuf for it    ([#1501](https://github.com/matter-labs/zksync-era/issues/1501)) ([086ba5b](https://github.com/matter-labs/zksync-era/commit/086ba5b40565db7c23697830af2b9910b8bd0e34))
* **db:** Wrap sqlx errors in DAL ([#1522](https://github.com/matter-labs/zksync-era/issues/1522)) ([6e9ed8c](https://github.com/matter-labs/zksync-era/commit/6e9ed8c0499830ba71a22b5e112d94aa7e91d517))
* fix availability checker ([#1574](https://github.com/matter-labs/zksync-era/issues/1574)) ([b2f21fb](https://github.com/matter-labs/zksync-era/commit/b2f21fb1d72e65a738db9f7bc9f162a410d36c9b))
* Prover CLI Scaffoldings ([#1609](https://github.com/matter-labs/zksync-era/issues/1609)) ([9a22fa0](https://github.com/matter-labs/zksync-era/commit/9a22fa07b0c577dbcdc48450ae209c83772a1fc0))
* Remove zksync-rs SDK ([#1559](https://github.com/matter-labs/zksync-era/issues/1559)) ([cc78e1d](https://github.com/matter-labs/zksync-era/commit/cc78e1d98cc51e86b3c1b70dbe3dc38deaa9f3c2))
* **sqlx:** Use offline mode by default ([#1539](https://github.com/matter-labs/zksync-era/issues/1539)) ([af01edd](https://github.com/matter-labs/zksync-era/commit/af01edd6cedd96c0ce73b24e0d74452ec6c38d43))
* **vm:** 1 5 0 support ([#1508](https://github.com/matter-labs/zksync-era/issues/1508)) ([a6ccd25](https://github.com/matter-labs/zksync-era/commit/a6ccd2533b65a7464f097e2082b690bd426d7694))


### Bug Fixes

* **en:** Fix miscellaneous snapshot recovery nits ([#1701](https://github.com/matter-labs/zksync-era/issues/1701)) ([13bfecc](https://github.com/matter-labs/zksync-era/commit/13bfecc760c8b7217802b1d6e2c5da9afe61af39))
* made consensus store certificates asynchronously from statekeeper ([#1711](https://github.com/matter-labs/zksync-era/issues/1711)) ([d1032ab](https://github.com/matter-labs/zksync-era/commit/d1032ab2b4328352d602783606dbffe6c0c9f635))


### Performance Improvements

* **merkle tree:** Manage indices / filters in RocksDB ([#1550](https://github.com/matter-labs/zksync-era/issues/1550)) ([6bbfa06](https://github.com/matter-labs/zksync-era/commit/6bbfa064371089a31d6751de05682edda1dbfe2e))


### Reverts

* **env:** Remove `ZKSYNC_HOME` env var from server ([#1713](https://github.com/matter-labs/zksync-era/issues/1713)) ([aed23e1](https://github.com/matter-labs/zksync-era/commit/aed23e1b2c100d6c8fc9259a1e573e790bfad36b))

## [12.2.0](https://github.com/matter-labs/zksync-era/compare/prover-v12.1.0...prover-v12.2.0) (2024-03-28)


### Features

* **api:** introduce mempool cache ([#1460](https://github.com/matter-labs/zksync-era/issues/1460)) ([c5d6c4b](https://github.com/matter-labs/zksync-era/commit/c5d6c4b96034da05fec15beb44dd54c3a0a249e7))
* **commitment-generator:** `events_queue` shadow mode ([#1138](https://github.com/matter-labs/zksync-era/issues/1138)) ([9bb47fa](https://github.com/matter-labs/zksync-era/commit/9bb47faba73eecfddf706d1a5382caa733c7ed37))
* Drop prover tables in core database ([#1436](https://github.com/matter-labs/zksync-era/issues/1436)) ([0d78122](https://github.com/matter-labs/zksync-era/commit/0d78122833e8f92b63fad7c7a9974b9693c1d792))
* Follow-up for DAL split ([#1464](https://github.com/matter-labs/zksync-era/issues/1464)) ([c072288](https://github.com/matter-labs/zksync-era/commit/c072288a19511cb353e2895818178337b9a2d8ae))
* **prover:** export prover traces through OTLP ([#1427](https://github.com/matter-labs/zksync-era/issues/1427)) ([16dce75](https://github.com/matter-labs/zksync-era/commit/16dce7588ae6435bade23f48f6f8475312935445))
* **prover:** File-info tool to help prover debugging ([#1216](https://github.com/matter-labs/zksync-era/issues/1216)) ([9759907](https://github.com/matter-labs/zksync-era/commit/9759907ab720844877d9274f59ecceb08550b457))
* Separate Prover and Server DAL ([#1334](https://github.com/matter-labs/zksync-era/issues/1334)) ([103a56b](https://github.com/matter-labs/zksync-era/commit/103a56b2a66d58ffb9a16d4fb64cbfd90c2d5d7b))
* support running consensus from snapshot (BFT-418) ([#1429](https://github.com/matter-labs/zksync-era/issues/1429)) ([f9f4d38](https://github.com/matter-labs/zksync-era/commit/f9f4d38258916fe8fc9a317bf96bfd9e3ad26e46))


### Bug Fixes

* **api:** Fix API server shutdown flow ([#1425](https://github.com/matter-labs/zksync-era/issues/1425)) ([780f6b0](https://github.com/matter-labs/zksync-era/commit/780f6b041991c6f26ead50f8227dd6f8ff949208))
* **prover:** Remove FriProtocolVersionId ([#1510](https://github.com/matter-labs/zksync-era/issues/1510)) ([6aa51b0](https://github.com/matter-labs/zksync-era/commit/6aa51b0c04b9da5ba5d1c5a208ccf253188d45ef))

## [12.1.0](https://github.com/matter-labs/zksync-era/compare/prover-v12.0.1...prover-v12.1.0) (2024-03-12)


### Features

* add run-observability to zk ([#1359](https://github.com/matter-labs/zksync-era/issues/1359)) ([2b520f6](https://github.com/matter-labs/zksync-era/commit/2b520f6e0efc2f996e46d06e66be8eb273138633))
* block revert support for consensus component ([#1213](https://github.com/matter-labs/zksync-era/issues/1213)) ([8a3a938](https://github.com/matter-labs/zksync-era/commit/8a3a938c1ce871e77a478b4cd06b23714a6d7c64))
* enhance unit test for batch tip ([#1253](https://github.com/matter-labs/zksync-era/issues/1253)) ([ca7d194](https://github.com/matter-labs/zksync-era/commit/ca7d19429cf5f0be8451c930423cb9733e55e7b1))
* Moving 1.4.x to use the circuit_api ([#1383](https://github.com/matter-labs/zksync-era/issues/1383)) ([8add2d6](https://github.com/matter-labs/zksync-era/commit/8add2d6d8d5837359f1d309c959d05f23e5393fe))
* **prover:** Fixing snark verification keys ([#1225](https://github.com/matter-labs/zksync-era/issues/1225)) ([5cbef73](https://github.com/matter-labs/zksync-era/commit/5cbef736996a4320437ec6a9c2ffb9da0d66eed0))
* replacing 1.3.3 test harness with circuit sequencer api ([#1382](https://github.com/matter-labs/zksync-era/issues/1382)) ([a628d56](https://github.com/matter-labs/zksync-era/commit/a628d56f449889ae5c5a0e48294e0be3e560728b))
* Start using a new test harness interface without generics ([#1378](https://github.com/matter-labs/zksync-era/issues/1378)) ([1e431a6](https://github.com/matter-labs/zksync-era/commit/1e431a65d17c3fdd3b873e31b3193ac72a0842f9))


### Bug Fixes

* **api:** SQL: use = instead of ANY where possible in events-related queries ([#1346](https://github.com/matter-labs/zksync-era/issues/1346)) ([160b4d4](https://github.com/matter-labs/zksync-era/commit/160b4d4a59851c90ae9f439ac3e960d073a0ea18))
* Updated Verification keys ([#1414](https://github.com/matter-labs/zksync-era/issues/1414)) ([66667d2](https://github.com/matter-labs/zksync-era/commit/66667d2383ee64fec8173275976be2b8e1a8b364))

## [12.0.1](https://github.com/matter-labs/zksync-era/compare/prover-v12.0.0...prover-v12.0.1) (2024-03-05)


### Performance Improvements

* remove CSReferenceAssembly structure instantiation in GPU prover ([#1100](https://github.com/matter-labs/zksync-era/issues/1100)) ([5c405ba](https://github.com/matter-labs/zksync-era/commit/5c405ba00dd41e579f0ea575ea723f4dd3aa0e63))

## [12.0.0](https://github.com/matter-labs/zksync-era/compare/prover-v11.0.0...prover-v12.0.0) (2024-03-04)


### ⚠ BREAKING CHANGES

* **prover:** Add EIP4844 support for provers subsystem ([#1200](https://github.com/matter-labs/zksync-era/issues/1200))
* Set 21 as latest protocol version ([#1262](https://github.com/matter-labs/zksync-era/issues/1262))

### Features

* Adding ability to generate 4844 setup key and refactor ([#1143](https://github.com/matter-labs/zksync-era/issues/1143)) ([975f54b](https://github.com/matter-labs/zksync-era/commit/975f54bca211b2b6de22a83d1dec120b31f4d39b))
* **api:** Remove unused and obsolete token info ([#1071](https://github.com/matter-labs/zksync-era/issues/1071)) ([e920897](https://github.com/matter-labs/zksync-era/commit/e920897d41583b822eefc72fb5899cfcbeefd621))
* **dal:** `zksync_types::Transaction` to use protobuf for wire encoding (BFT-407) ([#1047](https://github.com/matter-labs/zksync-era/issues/1047)) ([ee94bee](https://github.com/matter-labs/zksync-era/commit/ee94beed9e610adee94ea5073f0c748df7447135))
* **db:** Soft-remove `storage` table ([#982](https://github.com/matter-labs/zksync-era/issues/982)) ([601f893](https://github.com/matter-labs/zksync-era/commit/601f893b98613222422961b95473560445e34637))
* **en:** Integrate snapshots recovery into EN ([#1032](https://github.com/matter-labs/zksync-era/issues/1032)) ([c7cfaf9](https://github.com/matter-labs/zksync-era/commit/c7cfaf94e20987fa5b9d91c6b65bd66f5d907620))
* **healthcheck:** Various healthcheck improvements ([#1166](https://github.com/matter-labs/zksync-era/issues/1166)) ([1e34148](https://github.com/matter-labs/zksync-era/commit/1e3414831ffebbad67a552dcaededfa93d6762df))
* improving verification key generation ([#1050](https://github.com/matter-labs/zksync-era/issues/1050)) ([6f715c8](https://github.com/matter-labs/zksync-era/commit/6f715c8dd7093b95f95d4262b5202b2e6ce77f49))
* Prover interface and L1 interface crates ([#959](https://github.com/matter-labs/zksync-era/issues/959)) ([4f7e107](https://github.com/matter-labs/zksync-era/commit/4f7e10783afdff67a24246f17f03b536f743352d))
* **prover:** Add EIP4844 support for provers subsystem ([#1200](https://github.com/matter-labs/zksync-era/issues/1200)) ([6953e89](https://github.com/matter-labs/zksync-era/commit/6953e8989571d65230c66f3731c8831fd7abb274))
* **prover:** Added --recompute-if-missing option to key generator ([#1151](https://github.com/matter-labs/zksync-era/issues/1151)) ([cad7278](https://github.com/matter-labs/zksync-era/commit/cad727893b68a34d0d622e5f0349264119b207e8))
* **prover:** Added 4844 circuit to verification keys ([#1141](https://github.com/matter-labs/zksync-era/issues/1141)) ([8b0cc4a](https://github.com/matter-labs/zksync-era/commit/8b0cc4a81a49d3d8b58bb0e94e5bd20f9fec14d6))
* **prover:** Adding first support for 4844 circuit ([#1155](https://github.com/matter-labs/zksync-era/issues/1155)) ([6f63c53](https://github.com/matter-labs/zksync-era/commit/6f63c53549fda7e253e1d4cb4ed42ec6a2e3e9f1))
* **prover:** adding keystore object to handle reading and writing of prover keys ([#1132](https://github.com/matter-labs/zksync-era/issues/1132)) ([1471615](https://github.com/matter-labs/zksync-era/commit/147161500b209e4dc05fca252cbf23f61c47a7a9))
* **prover:** merging key generation into a single binary  ([#1101](https://github.com/matter-labs/zksync-era/issues/1101)) ([6de8b84](https://github.com/matter-labs/zksync-era/commit/6de8b84985a5dff25cd38c0f9a88972d85f39a70))
* **prover:** Moved setup key generation logic to test harness ([#1113](https://github.com/matter-labs/zksync-era/issues/1113)) ([469ab06](https://github.com/matter-labs/zksync-era/commit/469ab06af3a57b86ae504dc2a2644407ea4ee4e2))
* **prover:** Use new shivini function for 4844 circuits ([#1205](https://github.com/matter-labs/zksync-era/issues/1205)) ([376c09e](https://github.com/matter-labs/zksync-era/commit/376c09eddb8fd26cb823c5b2d1e9ce4a619610bb))
* Set 21 as latest protocol version ([#1262](https://github.com/matter-labs/zksync-era/issues/1262)) ([30579ef](https://github.com/matter-labs/zksync-era/commit/30579ef6cf5e8f96572d9b215fceec79556ce0ea))
* **vlog:** Remove env getters from vlog ([#1077](https://github.com/matter-labs/zksync-era/issues/1077)) ([00d3429](https://github.com/matter-labs/zksync-era/commit/00d342932c6305055f719afaecf6a0eeeb3ca63a))


### Bug Fixes

* fix link ([#1007](https://github.com/matter-labs/zksync-era/issues/1007)) ([f1424ce](https://github.com/matter-labs/zksync-era/commit/f1424ced16b5609e736d9075ef1339b955154260))
* make `zk status prover` use the new prover table ([#1044](https://github.com/matter-labs/zksync-era/issues/1044)) ([9b21d7f](https://github.com/matter-labs/zksync-era/commit/9b21d7fc2602bf9e6c6ae560dc267ede387ac4ea))
* **prover:** Decouple core/ prover database management ([#1029](https://github.com/matter-labs/zksync-era/issues/1029)) ([37674fd](https://github.com/matter-labs/zksync-era/commit/37674fdfc44c1130cad5f265ea57cb15aa81a08f))
* **prover:** Fix initial prover migration ([#1083](https://github.com/matter-labs/zksync-era/issues/1083)) ([6d54010](https://github.com/matter-labs/zksync-era/commit/6d540100aeb3fa237cb23b2f1168d82f9d8e1930))
* **prover:** QoL socket utilization ([#1020](https://github.com/matter-labs/zksync-era/issues/1020)) ([13a6816](https://github.com/matter-labs/zksync-era/commit/13a68160c0562d27fd3688dc57fb7730e42ea4db))
* update harness to include fix to new boojum OOM ([#1053](https://github.com/matter-labs/zksync-era/issues/1053)) ([4976941](https://github.com/matter-labs/zksync-era/commit/4976941b677cbfe19a40bac1b7541a67c323f7cd))


### Performance Improvements

* bump harness version ([#1003](https://github.com/matter-labs/zksync-era/issues/1003)) ([1cbb4c9](https://github.com/matter-labs/zksync-era/commit/1cbb4c9bb77a63494e30cb13b07c70712e3741c2))
* reduce memory consumption of witness generation ([#696](https://github.com/matter-labs/zksync-era/issues/696)) ([dea6768](https://github.com/matter-labs/zksync-era/commit/dea676832bcd54a4adc8998f309fed42f098a529))
* upgrade harness version to improve witness generation memory spike ([#1034](https://github.com/matter-labs/zksync-era/issues/1034)) ([09bbb84](https://github.com/matter-labs/zksync-era/commit/09bbb841c37bfd87deedf204d9832c486dad7bc7))
* use jemalloc in witness generator ([#1014](https://github.com/matter-labs/zksync-era/issues/1014)) ([917b2dc](https://github.com/matter-labs/zksync-era/commit/917b2dc9bfbb2b2d29145ade787febbb569a4b11))

## [11.0.0](https://github.com/matter-labs/zksync-era/compare/prover-v10.1.0...prover-v11.0.0) (2024-01-29)


### ⚠ BREAKING CHANGES

* **vm:** fee model updates + 1.4.1 ([#791](https://github.com/matter-labs/zksync-era/issues/791))

### Features

* **api:** Make Web3 API server work with pruned data ([#838](https://github.com/matter-labs/zksync-era/issues/838)) ([0b7cd0b](https://github.com/matter-labs/zksync-era/commit/0b7cd0b50ead2406915528becad2fac8b7e48f85))
* consensus component config for main node and external node ([#881](https://github.com/matter-labs/zksync-era/issues/881)) ([1aed8de](https://github.com/matter-labs/zksync-era/commit/1aed8de0f1651686bf9e9f8aa7dc9ba15625cc42))
* **en:** Restore state keeper storage from snapshot ([#885](https://github.com/matter-labs/zksync-era/issues/885)) ([a9553b5](https://github.com/matter-labs/zksync-era/commit/a9553b537a857a6f6a755cd700da4c096c1f80f0))
* fee model updates + 1.4.1 stage upgrade ([#897](https://github.com/matter-labs/zksync-era/issues/897)) ([fa48c13](https://github.com/matter-labs/zksync-era/commit/fa48c13da0cfa20117f68c51c243ee3738184408))
* protobuf-generated json configs for the main node (BFT-371) ([#458](https://github.com/matter-labs/zksync-era/issues/458)) ([f938314](https://github.com/matter-labs/zksync-era/commit/f9383143b4f1f0c18af658980bae8ec93b6b588f))
* Remove zkevm_test_harness public reexport from zksync_types ([#929](https://github.com/matter-labs/zksync-era/issues/929)) ([dd1a35e](https://github.com/matter-labs/zksync-era/commit/dd1a35eec006b40db66da73e6fa3d8963efb7d60))
* **state-keeper:** circuits seal criterion ([#729](https://github.com/matter-labs/zksync-era/issues/729)) ([c4a86bb](https://github.com/matter-labs/zksync-era/commit/c4a86bbbc5697b5391a517299bbd7a5e882a7314))
* **vm:** fee model updates + 1.4.1 ([#791](https://github.com/matter-labs/zksync-era/issues/791)) ([3564aff](https://github.com/matter-labs/zksync-era/commit/3564affbb246c87d668ea2ec74809384bc9d621f))


### Bug Fixes

* address issue with spellchecker not checking against prover workspace ([#855](https://github.com/matter-labs/zksync-era/issues/855)) ([4f55926](https://github.com/matter-labs/zksync-era/commit/4f55926f48aaec3f43322594626148af0a0358dd))
* addresses broken links in preparation for ci link check ([#869](https://github.com/matter-labs/zksync-era/issues/869)) ([a78d03c](https://github.com/matter-labs/zksync-era/commit/a78d03cc53d0097f6be892de65a2c35bd7f1baa3))
* **prover:** Update shivini ([#915](https://github.com/matter-labs/zksync-era/issues/915)) ([f141a00](https://github.com/matter-labs/zksync-era/commit/f141a00cd25ae5e5d2a054aa4ecda544a2abbbd7))
* **witness-generator:** Update era-zkevm_test_harness ([#912](https://github.com/matter-labs/zksync-era/issues/912)) ([c03c2e3](https://github.com/matter-labs/zksync-era/commit/c03c2e3df71b5737cbdae889c8330511345c52c2))
* **witness-generator:** Update zkevm_test_harness ([#930](https://github.com/matter-labs/zksync-era/issues/930)) ([16fdcff](https://github.com/matter-labs/zksync-era/commit/16fdcffc67274f30a4a254c26b8969f4928918bc))

## [10.1.0](https://github.com/matter-labs/zksync-era/compare/prover-v10.0.2...prover-v10.1.0) (2024-01-05)


### Features

* **prover:** Remove circuit-synthesizer ([#801](https://github.com/matter-labs/zksync-era/issues/801)) ([1426b1b](https://github.com/matter-labs/zksync-era/commit/1426b1ba3c8b700e0531087b781ced0756c12e3c))
* **prover:** Remove old prover ([#810](https://github.com/matter-labs/zksync-era/issues/810)) ([8be1925](https://github.com/matter-labs/zksync-era/commit/8be1925b18dcbf268eb03b8ea5f07adfd5330876))


### Bug Fixes

* **prover:** increase DB polling interval for witness vector generators ([#697](https://github.com/matter-labs/zksync-era/issues/697)) ([94579cc](https://github.com/matter-labs/zksync-era/commit/94579cc524514cb867843336cd9787db1b6b99d3))
* **prover:** Remove prover-utils from core ([#819](https://github.com/matter-labs/zksync-era/issues/819)) ([2ceb911](https://github.com/matter-labs/zksync-era/commit/2ceb9114659f4c4583c87b1bbc8ee230eb1c44db))

## [10.0.2](https://github.com/matter-labs/zksync-era/compare/prover-v10.0.1...prover-v10.0.2) (2023-12-21)


### Bug Fixes

* **prover:** Reduce amount of prover connections per prover subcomponent ([#734](https://github.com/matter-labs/zksync-era/issues/734)) ([d38aa85](https://github.com/matter-labs/zksync-era/commit/d38aa8590c60a04278599a470debeb00e37c2395))

## [10.0.1](https://github.com/matter-labs/zksync-era/compare/prover-v10.0.0...prover-v10.0.1) (2023-12-21)


### Bug Fixes

* **prover:** Add logging for prover + WVGs ([#723](https://github.com/matter-labs/zksync-era/issues/723)) ([d7ce14c](https://github.com/matter-labs/zksync-era/commit/d7ce14c5d0434326a1ebf406d77c20676ae526ae))
* **prover:** update rescue_poseidon version ([#726](https://github.com/matter-labs/zksync-era/issues/726)) ([3db25cb](https://github.com/matter-labs/zksync-era/commit/3db25cbea80180a1238531944d7e45a5408003dc))

## [10.0.0](https://github.com/matter-labs/zksync-era/compare/prover-v9.1.0...prover-v10.0.0) (2023-12-05)


### ⚠ BREAKING CHANGES

* boojum integration ([#112](https://github.com/matter-labs/zksync-era/issues/112))
* Update to protocol version 17 ([#384](https://github.com/matter-labs/zksync-era/issues/384))

### Features

* Add various metrics to the Prover subsystems ([#541](https://github.com/matter-labs/zksync-era/issues/541)) ([58a4e6c](https://github.com/matter-labs/zksync-era/commit/58a4e6c4c22bd7f002ede1c6def0dc260706185e))
* adds spellchecker workflow, and corrects misspelled words ([#559](https://github.com/matter-labs/zksync-era/issues/559)) ([beac0a8](https://github.com/matter-labs/zksync-era/commit/beac0a85bb1535b05c395057171f197cd976bf82))
* boojum integration ([#112](https://github.com/matter-labs/zksync-era/issues/112)) ([e76d346](https://github.com/matter-labs/zksync-era/commit/e76d346d02ded771dea380aa8240da32119d7198))
* **boojum:** Adding README to prover directory ([#189](https://github.com/matter-labs/zksync-era/issues/189)) ([c175033](https://github.com/matter-labs/zksync-era/commit/c175033b48a8da4969d88b6850dd0247c4004794))
* **config:** Extract everything not related to the env config from zksync_config crate ([#245](https://github.com/matter-labs/zksync-era/issues/245)) ([42c64e9](https://github.com/matter-labs/zksync-era/commit/42c64e91e13b6b37619f1459f927fa046ef01097))
* **core:** Split config definitions and deserialization ([#414](https://github.com/matter-labs/zksync-era/issues/414)) ([c7c6b32](https://github.com/matter-labs/zksync-era/commit/c7c6b321a63dbcc7f1af045aa7416e697beab08f))
* **dal:** Do not load config from env in DAL crate ([#444](https://github.com/matter-labs/zksync-era/issues/444)) ([3fe1bb2](https://github.com/matter-labs/zksync-era/commit/3fe1bb21f8d33557353f447811ca86c60f1fe51a))
* **en:** Implement gossip fetcher ([#371](https://github.com/matter-labs/zksync-era/issues/371)) ([a49b61d](https://github.com/matter-labs/zksync-era/commit/a49b61d7769f9dd7b4cbc4905f8f8a23abfb541c))
* **fri-prover:** In witness - panic if protocol version is not available ([#192](https://github.com/matter-labs/zksync-era/issues/192)) ([0403749](https://github.com/matter-labs/zksync-era/commit/040374900656c854a7b9de32e5dbaf47c1c47889))
* **hyperchain:** Adding prover related commands to zk stack ([#440](https://github.com/matter-labs/zksync-era/issues/440)) ([580cada](https://github.com/matter-labs/zksync-era/commit/580cada003bdfe2fff686a1fc3ce001b4959aa4d))
* **job-processor:** report attempts metrics ([#448](https://github.com/matter-labs/zksync-era/issues/448)) ([ab31f03](https://github.com/matter-labs/zksync-era/commit/ab31f031dfcaa7ddf296786ddccb78e8edd2d3c5))
* **merkle tree:** Allow random-order tree recovery ([#485](https://github.com/matter-labs/zksync-era/issues/485)) ([146e4cf](https://github.com/matter-labs/zksync-era/commit/146e4cf2f8d890ff0a8d33229e224442e14be437))
* **merkle tree:** Snapshot recovery for Merkle tree ([#163](https://github.com/matter-labs/zksync-era/issues/163)) ([9e20703](https://github.com/matter-labs/zksync-era/commit/9e2070380e6720d84563a14a2246fc18fdb1f8f9))
* Rewrite server binary to use `vise` metrics ([#120](https://github.com/matter-labs/zksync-era/issues/120)) ([26ee1fb](https://github.com/matter-labs/zksync-era/commit/26ee1fbb16cbd7c4fad334cbc6804e7d779029b6))
* Update to protocol version 17 ([#384](https://github.com/matter-labs/zksync-era/issues/384)) ([ba271a5](https://github.com/matter-labs/zksync-era/commit/ba271a5f34d64d04c0135b8811685b80f26a8c32))
* **vm:** Move all vm versions to the one crate ([#249](https://github.com/matter-labs/zksync-era/issues/249)) ([e3fb489](https://github.com/matter-labs/zksync-era/commit/e3fb4894d08aa98a84e64eaa95b51001055cf911))
* **witness-generator:** add logs to leaf aggregation job ([#542](https://github.com/matter-labs/zksync-era/issues/542)) ([7e95a3a](https://github.com/matter-labs/zksync-era/commit/7e95a3a66ea48be7b6059d34630e22c503399bdf))


### Bug Fixes

* Change no pending batches 404 error into a success response ([#279](https://github.com/matter-labs/zksync-era/issues/279)) ([e8fd805](https://github.com/matter-labs/zksync-era/commit/e8fd805c8be7980de7676bca87cfc2d445aab9e1))
* **ci:** Use the same nightly rust ([#530](https://github.com/matter-labs/zksync-era/issues/530)) ([67ef133](https://github.com/matter-labs/zksync-era/commit/67ef1339d42786efbeb83c22fac99f3bf5dd4380))
* **crypto:** update deps to include circuit fixes ([#402](https://github.com/matter-labs/zksync-era/issues/402)) ([4c82015](https://github.com/matter-labs/zksync-era/commit/4c820150714dfb01c304c43e27f217f17deba449))
* **crypto:** update shivini to switch to era-cuda ([#469](https://github.com/matter-labs/zksync-era/issues/469)) ([38bb482](https://github.com/matter-labs/zksync-era/commit/38bb4823c7b5e0e651d9f531feede66c24afd19f))
* **crypto:** update snark-vk to be used in server and update args for proof wrapping ([#240](https://github.com/matter-labs/zksync-era/issues/240)) ([4a5c54c](https://github.com/matter-labs/zksync-era/commit/4a5c54c48bbc100c29fa719c4b1dc3535743003d))
* **docs:** Add links to setup-data keys ([#360](https://github.com/matter-labs/zksync-era/issues/360)) ([1d4fe69](https://github.com/matter-labs/zksync-era/commit/1d4fe697e4e98a8e64642cde4fe202338ce5ec61))
* **path:** update gpu prover setup data path to remove extra gpu suffix ([#454](https://github.com/matter-labs/zksync-era/issues/454)) ([2e145c1](https://github.com/matter-labs/zksync-era/commit/2e145c192b348b2756acf61fac5bfe0ca5a6575f))
* **prover-fri:** Update setup loading for node agg circuit ([#323](https://github.com/matter-labs/zksync-era/issues/323)) ([d1034b0](https://github.com/matter-labs/zksync-era/commit/d1034b05754219b603508ef79c114d908c94c1e9))
* **prover-logging:** tasks_allowed_to_finish set to true for 1 off jobs ([#227](https://github.com/matter-labs/zksync-era/issues/227)) ([0fac66f](https://github.com/matter-labs/zksync-era/commit/0fac66f5ff86cc801ea0bb6f9272cb397cd03a95))
* Sync protocol version between consensus and server blocks ([#568](https://github.com/matter-labs/zksync-era/issues/568)) ([56776f9](https://github.com/matter-labs/zksync-era/commit/56776f929f547b1a91c5b70f89e87ef7dc25c65a))
* Update prover to use the correct storage oracle ([#446](https://github.com/matter-labs/zksync-era/issues/446)) ([835dd82](https://github.com/matter-labs/zksync-era/commit/835dd828ef5610a446ec8c456e4df1def0e213ab))

## [9.1.0](https://github.com/matter-labs/zksync-era/compare/prover-v9.0.0...prover-v9.1.0) (2023-12-05)


### Features

* Add various metrics to the Prover subsystems ([#541](https://github.com/matter-labs/zksync-era/issues/541)) ([58a4e6c](https://github.com/matter-labs/zksync-era/commit/58a4e6c4c22bd7f002ede1c6def0dc260706185e))
* adds spellchecker workflow, and corrects misspelled words ([#559](https://github.com/matter-labs/zksync-era/issues/559)) ([beac0a8](https://github.com/matter-labs/zksync-era/commit/beac0a85bb1535b05c395057171f197cd976bf82))
* **dal:** Do not load config from env in DAL crate ([#444](https://github.com/matter-labs/zksync-era/issues/444)) ([3fe1bb2](https://github.com/matter-labs/zksync-era/commit/3fe1bb21f8d33557353f447811ca86c60f1fe51a))
* **en:** Implement gossip fetcher ([#371](https://github.com/matter-labs/zksync-era/issues/371)) ([a49b61d](https://github.com/matter-labs/zksync-era/commit/a49b61d7769f9dd7b4cbc4905f8f8a23abfb541c))
* **hyperchain:** Adding prover related commands to zk stack ([#440](https://github.com/matter-labs/zksync-era/issues/440)) ([580cada](https://github.com/matter-labs/zksync-era/commit/580cada003bdfe2fff686a1fc3ce001b4959aa4d))
* **job-processor:** report attempts metrics ([#448](https://github.com/matter-labs/zksync-era/issues/448)) ([ab31f03](https://github.com/matter-labs/zksync-era/commit/ab31f031dfcaa7ddf296786ddccb78e8edd2d3c5))
* **merkle tree:** Allow random-order tree recovery ([#485](https://github.com/matter-labs/zksync-era/issues/485)) ([146e4cf](https://github.com/matter-labs/zksync-era/commit/146e4cf2f8d890ff0a8d33229e224442e14be437))
* **witness-generator:** add logs to leaf aggregation job ([#542](https://github.com/matter-labs/zksync-era/issues/542)) ([7e95a3a](https://github.com/matter-labs/zksync-era/commit/7e95a3a66ea48be7b6059d34630e22c503399bdf))


### Bug Fixes

* Change no pending batches 404 error into a success response ([#279](https://github.com/matter-labs/zksync-era/issues/279)) ([e8fd805](https://github.com/matter-labs/zksync-era/commit/e8fd805c8be7980de7676bca87cfc2d445aab9e1))
* **ci:** Use the same nightly rust ([#530](https://github.com/matter-labs/zksync-era/issues/530)) ([67ef133](https://github.com/matter-labs/zksync-era/commit/67ef1339d42786efbeb83c22fac99f3bf5dd4380))
* **crypto:** update shivini to switch to era-cuda ([#469](https://github.com/matter-labs/zksync-era/issues/469)) ([38bb482](https://github.com/matter-labs/zksync-era/commit/38bb4823c7b5e0e651d9f531feede66c24afd19f))
* Sync protocol version between consensus and server blocks ([#568](https://github.com/matter-labs/zksync-era/issues/568)) ([56776f9](https://github.com/matter-labs/zksync-era/commit/56776f929f547b1a91c5b70f89e87ef7dc25c65a))

## [9.0.0](https://github.com/matter-labs/zksync-era/compare/prover-v8.1.0...prover-v9.0.0) (2023-11-09)


### ⚠ BREAKING CHANGES

* boojum integration ([#112](https://github.com/matter-labs/zksync-era/issues/112))

### Features

* boojum integration ([#112](https://github.com/matter-labs/zksync-era/issues/112)) ([e76d346](https://github.com/matter-labs/zksync-era/commit/e76d346d02ded771dea380aa8240da32119d7198))
* **core:** Split config definitions and deserialization ([#414](https://github.com/matter-labs/zksync-era/issues/414)) ([c7c6b32](https://github.com/matter-labs/zksync-era/commit/c7c6b321a63dbcc7f1af045aa7416e697beab08f))


### Bug Fixes

* **crypto:** update deps to include circuit fixes ([#402](https://github.com/matter-labs/zksync-era/issues/402)) ([4c82015](https://github.com/matter-labs/zksync-era/commit/4c820150714dfb01c304c43e27f217f17deba449))
* **path:** update gpu prover setup data path to remove extra gpu suffix ([#454](https://github.com/matter-labs/zksync-era/issues/454)) ([2e145c1](https://github.com/matter-labs/zksync-era/commit/2e145c192b348b2756acf61fac5bfe0ca5a6575f))
* Update prover to use the correct storage oracle ([#446](https://github.com/matter-labs/zksync-era/issues/446)) ([835dd82](https://github.com/matter-labs/zksync-era/commit/835dd828ef5610a446ec8c456e4df1def0e213ab))

## [8.1.0](https://github.com/matter-labs/zksync-era/compare/prover-v7.2.0...prover-v8.1.0) (2023-11-03)


### ⚠ BREAKING CHANGES

* Update to protocol version 17 ([#384](https://github.com/matter-labs/zksync-era/issues/384))

### Features

* **boojum:** Adding README to prover directory ([#189](https://github.com/matter-labs/zksync-era/issues/189)) ([c175033](https://github.com/matter-labs/zksync-era/commit/c175033b48a8da4969d88b6850dd0247c4004794))
* **config:** Extract everything not related to the env config from zksync_config crate ([#245](https://github.com/matter-labs/zksync-era/issues/245)) ([42c64e9](https://github.com/matter-labs/zksync-era/commit/42c64e91e13b6b37619f1459f927fa046ef01097))
* **fri-prover:** In witness - panic if protocol version is not available ([#192](https://github.com/matter-labs/zksync-era/issues/192)) ([0403749](https://github.com/matter-labs/zksync-era/commit/040374900656c854a7b9de32e5dbaf47c1c47889))
* **merkle tree:** Snapshot recovery for Merkle tree ([#163](https://github.com/matter-labs/zksync-era/issues/163)) ([9e20703](https://github.com/matter-labs/zksync-era/commit/9e2070380e6720d84563a14a2246fc18fdb1f8f9))
* Rewrite server binary to use `vise` metrics ([#120](https://github.com/matter-labs/zksync-era/issues/120)) ([26ee1fb](https://github.com/matter-labs/zksync-era/commit/26ee1fbb16cbd7c4fad334cbc6804e7d779029b6))
* Update to protocol version 17 ([#384](https://github.com/matter-labs/zksync-era/issues/384)) ([ba271a5](https://github.com/matter-labs/zksync-era/commit/ba271a5f34d64d04c0135b8811685b80f26a8c32))
* **vm:** Move all vm versions to the one crate ([#249](https://github.com/matter-labs/zksync-era/issues/249)) ([e3fb489](https://github.com/matter-labs/zksync-era/commit/e3fb4894d08aa98a84e64eaa95b51001055cf911))


### Bug Fixes

* **crypto:** update snark-vk to be used in server and update args for proof wrapping ([#240](https://github.com/matter-labs/zksync-era/issues/240)) ([4a5c54c](https://github.com/matter-labs/zksync-era/commit/4a5c54c48bbc100c29fa719c4b1dc3535743003d))
* **docs:** Add links to setup-data keys ([#360](https://github.com/matter-labs/zksync-era/issues/360)) ([1d4fe69](https://github.com/matter-labs/zksync-era/commit/1d4fe697e4e98a8e64642cde4fe202338ce5ec61))
* **prover-fri:** Update setup loading for node agg circuit ([#323](https://github.com/matter-labs/zksync-era/issues/323)) ([d1034b0](https://github.com/matter-labs/zksync-era/commit/d1034b05754219b603508ef79c114d908c94c1e9))
* **prover-logging:** tasks_allowed_to_finish set to true for 1 off jobs ([#227](https://github.com/matter-labs/zksync-era/issues/227)) ([0fac66f](https://github.com/matter-labs/zksync-era/commit/0fac66f5ff86cc801ea0bb6f9272cb397cd03a95))

## [7.2.0](https://github.com/matter-labs/zksync-era/compare/prover-v7.1.1...prover-v7.2.0) (2023-10-17)


### Features

* **boojum:** Adding README to prover directory ([#189](https://github.com/matter-labs/zksync-era/issues/189)) ([c175033](https://github.com/matter-labs/zksync-era/commit/c175033b48a8da4969d88b6850dd0247c4004794))
* **fri-prover:** In witness - panic if protocol version is not available ([#192](https://github.com/matter-labs/zksync-era/issues/192)) ([0403749](https://github.com/matter-labs/zksync-era/commit/040374900656c854a7b9de32e5dbaf47c1c47889))
* Rewrite server binary to use `vise` metrics ([#120](https://github.com/matter-labs/zksync-era/issues/120)) ([26ee1fb](https://github.com/matter-labs/zksync-era/commit/26ee1fbb16cbd7c4fad334cbc6804e7d779029b6))


### Bug Fixes

* **prover-logging:** tasks_allowed_to_finish set to true for 1 off jobs ([#227](https://github.com/matter-labs/zksync-era/issues/227)) ([0fac66f](https://github.com/matter-labs/zksync-era/commit/0fac66f5ff86cc801ea0bb6f9272cb397cd03a95))

## [7.1.1](https://github.com/matter-labs/zksync-2-dev/compare/prover-v7.1.0...prover-v7.1.1) (2023-09-27)


### Bug Fixes

* **crypto:** update harness to fix snark proof verification ([#2659](https://github.com/matter-labs/zksync-2-dev/issues/2659)) ([b48248b](https://github.com/matter-labs/zksync-2-dev/commit/b48248babb6200bb3735715a51a7fac01c5353d3))

## [7.1.0](https://github.com/matter-labs/zksync-2-dev/compare/prover-v7.0.1...prover-v7.1.0) (2023-09-26)


### Features

* **proof-compressor:** Add explicit verification for wrapped proofs based on config ([#2650](https://github.com/matter-labs/zksync-2-dev/issues/2650)) ([9b36bf3](https://github.com/matter-labs/zksync-2-dev/commit/9b36bf3ed76f1e5b4bb5ce36ed94392890aa34ba))
* Rewrite libraries to use `vise` metrics ([#2616](https://github.com/matter-labs/zksync-2-dev/issues/2616)) ([d8cdbe9](https://github.com/matter-labs/zksync-2-dev/commit/d8cdbe9ad8ce40f55bbd8c788f3ca055a33989e6))


### Bug Fixes

* **workflow:** update workflow to remove downloaded setup data before creating PR ([#2642](https://github.com/matter-labs/zksync-2-dev/issues/2642)) ([b915f41](https://github.com/matter-labs/zksync-2-dev/commit/b915f4108fea034ae75fa9c6e8068fc2e2a9823e))

## [7.0.1](https://github.com/matter-labs/zksync-2-dev/compare/prover-v7.0.0...prover-v7.0.1) (2023-09-22)


### Bug Fixes

* **prover-fri:** added handling for graceful shutdown to update DB status for FRI GPU prover ([#2637](https://github.com/matter-labs/zksync-2-dev/issues/2637)) ([be5f1bf](https://github.com/matter-labs/zksync-2-dev/commit/be5f1bf26ed7d1ee8fd9e6c755443d30ef57c0c0))

## [7.0.0](https://github.com/matter-labs/zksync-2-dev/compare/prover-v6.7.0...prover-v7.0.0) (2023-09-21)


### ⚠ BREAKING CHANGES

* update verification keys, protocol version 15 ([#2602](https://github.com/matter-labs/zksync-2-dev/issues/2602))

### Features

* update verification keys, protocol version 15 ([#2602](https://github.com/matter-labs/zksync-2-dev/issues/2602)) ([2fff59b](https://github.com/matter-labs/zksync-2-dev/commit/2fff59bab00849996864b68e932739135337ebd7))
* **vlog:** Rework the observability configuration subsystem ([#2608](https://github.com/matter-labs/zksync-2-dev/issues/2608)) ([377f0c5](https://github.com/matter-labs/zksync-2-dev/commit/377f0c5f734c979bc990b429dff0971466872e71))


### Bug Fixes

* **crypto:** update compressor to pass universal setup file ([#2610](https://github.com/matter-labs/zksync-2-dev/issues/2610)) ([39ea81c](https://github.com/matter-labs/zksync-2-dev/commit/39ea81c360026d58222826d8d97bf909ff2c6326))
* **prover-fri:** move saving to GCS behind flag ([#2627](https://github.com/matter-labs/zksync-2-dev/issues/2627)) ([ed49420](https://github.com/matter-labs/zksync-2-dev/commit/ed49420fb782856541c632e64466ff4da8e05e81))

## [6.7.0](https://github.com/matter-labs/zksync-2-dev/compare/prover-v6.6.1...prover-v6.7.0) (2023-09-19)


### Features

* **prover-fri:** added picked-by column in prover fri related tables ([#2600](https://github.com/matter-labs/zksync-2-dev/issues/2600)) ([9e604ab](https://github.com/matter-labs/zksync-2-dev/commit/9e604abf3bae11b6f583f2abd39c07a85dc20f0a))
* Rework metrics approach ([#2387](https://github.com/matter-labs/zksync-2-dev/issues/2387)) ([4855546](https://github.com/matter-labs/zksync-2-dev/commit/48555465d32f8524f6cf488859e8ae8259ecf5da))


### Bug Fixes

* **prover-fri:** update node-agg circuit id loading ([#2595](https://github.com/matter-labs/zksync-2-dev/issues/2595)) ([b4b7df0](https://github.com/matter-labs/zksync-2-dev/commit/b4b7df0a0ca085ad6384c74073b3a51b7ee50e70))

## [6.6.1](https://github.com/matter-labs/zksync-2-dev/compare/prover-v6.6.0...prover-v6.6.1) (2023-09-18)


### Bug Fixes

* **crypto:** update harness to include proof compression time improvement ([#2582](https://github.com/matter-labs/zksync-2-dev/issues/2582)) ([d560c83](https://github.com/matter-labs/zksync-2-dev/commit/d560c83030a1e24d241fd97e8d9a3c032139f1d8))

## [6.6.0](https://github.com/matter-labs/zksync-2-dev/compare/prover-v6.5.1...prover-v6.6.0) (2023-09-15)


### Features

* **prover-fri:** insert missing protocol version in FRI witness-gen table ([#2577](https://github.com/matter-labs/zksync-2-dev/issues/2577)) ([b9af6a5](https://github.com/matter-labs/zksync-2-dev/commit/b9af6a5784b0e6538bd542830593d16f3caf5fe5))
* **prover-server-api:** Add SkippedProofGeneration in SubmitProofRequest ([#2575](https://github.com/matter-labs/zksync-2-dev/issues/2575)) ([9c2653e](https://github.com/matter-labs/zksync-2-dev/commit/9c2653e5bc0e56b2906e9d25be3cb2887ad7d35d))
* Use tracing directly instead of vlog macros ([#2566](https://github.com/matter-labs/zksync-2-dev/issues/2566)) ([53d53af](https://github.com/matter-labs/zksync-2-dev/commit/53d53afc9157214fb911aa0934a97f8b5103e1ec))

## [6.5.1](https://github.com/matter-labs/zksync-2-dev/compare/prover-v6.5.0...prover-v6.5.1) (2023-09-14)


### Bug Fixes

* **proof-compressor:** add blob fetch+save metrics and use pushgateway ([#2564](https://github.com/matter-labs/zksync-2-dev/issues/2564)) ([1fdd347](https://github.com/matter-labs/zksync-2-dev/commit/1fdd347100750b3770e6f32434dcdc9f5d799c0f))

## [6.5.0](https://github.com/matter-labs/zksync-2-dev/compare/prover-v6.4.0...prover-v6.5.0) (2023-09-14)


### Features

* Decrease crate versions back to 0.1.0 ([#2528](https://github.com/matter-labs/zksync-2-dev/issues/2528)) ([adb7614](https://github.com/matter-labs/zksync-2-dev/commit/adb76142882dde197cd64b1aaaffb01906427054))
* **prover-fri:** added logic for generating snark VK ([#2545](https://github.com/matter-labs/zksync-2-dev/issues/2545)) ([1c1c21a](https://github.com/matter-labs/zksync-2-dev/commit/1c1c21a6b293b512b6a594a6d21b6d5ed03a3968))
* **prover-fri:** Restrict prover to pick jobs for which they have vk's ([#2541](https://github.com/matter-labs/zksync-2-dev/issues/2541)) ([cedba03](https://github.com/matter-labs/zksync-2-dev/commit/cedba03ea66fc0da479e60d5ca30d8f67e32358a))
* **vm:** New vm intregration ([#2198](https://github.com/matter-labs/zksync-2-dev/issues/2198)) ([f5e7e7a](https://github.com/matter-labs/zksync-2-dev/commit/f5e7e7a6fa81ab46289016f57a6123ffec83bcf6))


### Bug Fixes

* **crypto:** update crypto to fix snark VK to include gate_selectors_openings_at_z column ([#2556](https://github.com/matter-labs/zksync-2-dev/issues/2556)) ([64bea99](https://github.com/matter-labs/zksync-2-dev/commit/64bea99f0f415df0a8f9e8c035039052903a5ffc))

## [6.4.0](https://github.com/matter-labs/zksync-2-dev/compare/prover-v6.3.0...prover-v6.4.0) (2023-09-07)


### Features

* **prover-fri-compressor:** Create a dedicated component for FRI proof conversion ([#2501](https://github.com/matter-labs/zksync-2-dev/issues/2501)) ([cd43aa7](https://github.com/matter-labs/zksync-2-dev/commit/cd43aa73095bf97b54c9fbcc9934128cc29506c2))
* **prover-fri:** use separate object store config for FRI prover components ([#2494](https://github.com/matter-labs/zksync-2-dev/issues/2494)) ([7f2537f](https://github.com/matter-labs/zksync-2-dev/commit/7f2537fc987c55b6efec925506478d665d20c0c4))

## [6.3.0](https://github.com/matter-labs/zksync-2-dev/compare/prover-v6.2.0...prover-v6.3.0) (2023-09-05)


### Features

* **prover-fri:** Add protocol version for FRI prover related tables ([#2458](https://github.com/matter-labs/zksync-2-dev/issues/2458)) ([784a52b](https://github.com/matter-labs/zksync-2-dev/commit/784a52bc2d2fa784fe82cc10df1d39895255ade5))


### Bug Fixes

* **api:** Use MultiVM in API ([#2476](https://github.com/matter-labs/zksync-2-dev/issues/2476)) ([683582d](https://github.com/matter-labs/zksync-2-dev/commit/683582dab2fb26d09a5e183ac9e4d0d9e61286e4))
* **crypto:** update crypto deps boojum+harness, VK, setupdata ([#2481](https://github.com/matter-labs/zksync-2-dev/issues/2481)) ([4db26ef](https://github.com/matter-labs/zksync-2-dev/commit/4db26ef448ae2051ed7fefe33e4940d34c57a595))

## [6.2.0](https://github.com/matter-labs/zksync-2-dev/compare/prover-v6.1.0...prover-v6.2.0) (2023-09-01)


### Features

* **prover-gateway:** integrate snark wrapper to transform FRI proof to old ([#2413](https://github.com/matter-labs/zksync-2-dev/issues/2413)) ([60bb26b](https://github.com/matter-labs/zksync-2-dev/commit/60bb26bdc31f13f2b9253b245e848951e8e6e501))


### Bug Fixes

* **api:** fix eth_call for old blocks ([#2440](https://github.com/matter-labs/zksync-2-dev/issues/2440)) ([19bba44](https://github.com/matter-labs/zksync-2-dev/commit/19bba4413f8f4197e2178e409106eecf12089d08))
* **crypto:** Update generated verification keys + setup data ([#2460](https://github.com/matter-labs/zksync-2-dev/issues/2460)) ([659c228](https://github.com/matter-labs/zksync-2-dev/commit/659c22895029f643de1fb3eff44057f0468260ce))
* **fri-vk:** add scheduler vk commitment and expose lib for getting cached commitments ([#2446](https://github.com/matter-labs/zksync-2-dev/issues/2446)) ([caccef4](https://github.com/matter-labs/zksync-2-dev/commit/caccef458f9eced3089038e78153097ce8aa868c))
* **prover-fri:** enhance logging ([#2453](https://github.com/matter-labs/zksync-2-dev/issues/2453)) ([6aac273](https://github.com/matter-labs/zksync-2-dev/commit/6aac2730f501dbd0dc82103b75996b3b37d03750))

## [6.1.0](https://github.com/matter-labs/zksync-2-dev/compare/prover-v6.0.0...prover-v6.1.0) (2023-08-25)


### Features

* **prover-server-split:** consume API for fetching proof gen data and submitting proofs ([#2365](https://github.com/matter-labs/zksync-2-dev/issues/2365)) ([6e99471](https://github.com/matter-labs/zksync-2-dev/commit/6e994717086941fd2538fced7c32b4bb5eeb4eac))

## [6.0.0](https://github.com/matter-labs/zksync-2-dev/compare/prover-v5.30.1...prover-v6.0.0) (2023-08-23)


### ⚠ BREAKING CHANGES

* new upgrade system ([#1784](https://github.com/matter-labs/zksync-2-dev/issues/1784))

### Features

* **eth-sender:** add support for loading new proofs from GCS ([#2392](https://github.com/matter-labs/zksync-2-dev/issues/2392)) ([54f6f53](https://github.com/matter-labs/zksync-2-dev/commit/54f6f53953ddd20c19a8d6de092700de2835ad33))
* new upgrade system ([#1784](https://github.com/matter-labs/zksync-2-dev/issues/1784)) ([469a4c3](https://github.com/matter-labs/zksync-2-dev/commit/469a4c332a4f02b5a642b2951fd00228c9317f59))
* **prover-fri:** Add socket listener to receive witness vector over network ([#2367](https://github.com/matter-labs/zksync-2-dev/issues/2367)) ([19c9d89](https://github.com/matter-labs/zksync-2-dev/commit/19c9d89613a5d3ab5e55d9224a512a8952aa3689))
* **prover-fri:** Added GPU prover integ test ([#2287](https://github.com/matter-labs/zksync-2-dev/issues/2287)) ([7095acb](https://github.com/matter-labs/zksync-2-dev/commit/7095acb14d656c34eb738f2d32cb72a454c78b4f))
* **prover-fri:** create lib crate for shared types for FRI prover ([#2363](https://github.com/matter-labs/zksync-2-dev/issues/2363)) ([a42305f](https://github.com/matter-labs/zksync-2-dev/commit/a42305fac1c981ab71ce70660278203cbae1db22))
* **prover-fri:** update gpu prover to perform repeated assembly generation on separate thread ([#2398](https://github.com/matter-labs/zksync-2-dev/issues/2398)) ([0881ee8](https://github.com/matter-labs/zksync-2-dev/commit/0881ee8220991da1d02c7b20e3e7aaecc9cf45d7))
* **prover-server-split:** expose API from server for requesting proof gen data and submitting proof ([#2292](https://github.com/matter-labs/zksync-2-dev/issues/2292)) ([401d0ab](https://github.com/matter-labs/zksync-2-dev/commit/401d0ab51bfce89203fd82b5f8d1a6865f6d19b0))
* **setup-data-fri:** save finalization hints in file to be used for external synthesizer ([#2355](https://github.com/matter-labs/zksync-2-dev/issues/2355)) ([3405110](https://github.com/matter-labs/zksync-2-dev/commit/3405110f27ef931cf4d6a3dc4166325711f8751f))
* **witness-vector-generator:** create docker images for witness vector generator ([#2373](https://github.com/matter-labs/zksync-2-dev/issues/2373)) ([5b3bc1b](https://github.com/matter-labs/zksync-2-dev/commit/5b3bc1b95369b13e7ad2b981d2675b331131f71a))
* **witness-vector-generator:** Perform circuit synthesis on external machine ([#2351](https://github.com/matter-labs/zksync-2-dev/issues/2351)) ([6839f3f](https://github.com/matter-labs/zksync-2-dev/commit/6839f3fbfe472fb2fd492a9648bb97d1654bbb3b))


### Bug Fixes

* **crypto:** update verification keys for new circuits ([#2368](https://github.com/matter-labs/zksync-2-dev/issues/2368)) ([9377c7a](https://github.com/matter-labs/zksync-2-dev/commit/9377c7a6e1f3d99ca77d71707f92b3f60fffa4b7))
* **prover-fri:** enhance logging + update db queue status which taking job from queue if full ([#2395](https://github.com/matter-labs/zksync-2-dev/issues/2395)) ([50aada8](https://github.com/matter-labs/zksync-2-dev/commit/50aada8ff9bd6ece5bcb87062d359c680036dca4))
* **prover-fri:** panic in case of proof verification failure ([#2388](https://github.com/matter-labs/zksync-2-dev/issues/2388)) ([a551ff1](https://github.com/matter-labs/zksync-2-dev/commit/a551ff1a4bdd29a6e6709a1b65ff28608a1b3152))


### Performance Improvements

* **db:** Optimize loading L1 batch header ([#2343](https://github.com/matter-labs/zksync-2-dev/issues/2343)) ([1274469](https://github.com/matter-labs/zksync-2-dev/commit/1274469b0618582d2027bc7b8dbda779486a553d))

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
