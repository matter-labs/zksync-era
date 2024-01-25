# Changelog

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
