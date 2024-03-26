# Changelog

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
