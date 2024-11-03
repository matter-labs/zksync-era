[2m2024-11-03T16:42:58.468299Z[0m [32m INFO[0m [2mzksync_config::observability_ext[0m[2m:[0m Installed observability stack with the following configuration: ObservabilityConfig { sentry_url: None, sentry_environment: None, opentelemetry: None, log_format: "plain", log_directives: Some("zksync_node_test_utils=info,zksync_state_keeper=info,zksync_reorg_detector=info,zksync_consistency_checker=info,zksync_metadata_calculator=info,zksync_node_sync=info,zksync_node_consensus=info,zksync_contract_verification_server=info,zksync_node_api_server=info,zksync_node_framework=info,zksync_block_reverter=info,zksync_commitment_generator=debug,zksync_node_db_pruner=info,zksync_eth_sender=info,zksync_node_fee_model=info,zksync_node_genesis=info,zksync_house_keeper=info,zksync_proof_data_handler=info,zksync_shared_metrics=info,zksync_node_test_utils=info,zksync_vm_runner=info,zksync_consensus_bft=info,zksync_consensus_network=info,zksync_consensus_storage=info,zksync_core_leftovers=debug,zksync_server=debug,zksync_contract_verifier=debug,zksync_dal=info,zksync_db_connection=info,zksync_eth_client=info,zksync_eth_watch=debug,zksync_storage=info,zksync_db_manager=info,zksync_merkle_tree=info,zksync_state=debug,zksync_utils=debug,zksync_queued_job_processor=info,zksync_types=info,zksync_mempool=debug,loadnext=info,vm=info,zksync_object_store=info,zksync_external_node=info,zksync_witness_generator=info,zksync_prover_fri=info,zksync_witness_vector_generator=info,zksync_web3_decl=debug,zksync_health_check=debug,zksync_proof_fri_compressor=info,vise_exporter=error,snapshots_creator=debug,zksync_base_token_adjuster=debug,zksync_external_price_api=debug,zksync_external_proof_integration_api=info") }
[2m2024-11-03T16:42:58.470666Z[0m [32m INFO[0m [2mzksync_db_connection::connection_pool[0m[2m:[0m Created DB pool with parameters ConnectionPoolBuilder { database_url: "postgres://***:***@localhost:5432/zksync_prover_localhost_era", max_size: 27, acquire_timeout: 30s, statement_timeout: None, db: "zksync_prover_dal::Prover" }
[2m2024-11-03T16:42:58.470789Z[0m [32m INFO[0m [2mzksync_circuit_prover[0m[2m:[0m Loading mappings from disk...
[2m2024-11-03T16:43:03.187490Z[0m [32m INFO[0m [2mzksync_circuit_prover[0m[2m:[0m Loaded mappings from disk.
[2m2024-11-03T16:43:03.187520Z[0m [32m INFO[0m [2mzksync_circuit_prover[0m[2m:[0m Starting 23 light WVGs and 3 heavy WVGs.
[2m2024-11-03T16:43:03.188179Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.188237Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.194225Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.194241Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.194258Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.194349Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.196748Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.196758Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.196765Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.196864Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.197221Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.197240Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.197256Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.197317Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.201501Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.201538Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.201562Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.201637Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.201712Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.201750Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.201773Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.201846Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.205535Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.205563Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.205596Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.205665Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.205768Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.205802Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.205826Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.205903Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.210576Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.210617Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.210806Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.210847Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.210869Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.211016Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.215284Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.215556Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.215590Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.215630Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.215731Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.220300Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.220368Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.220411Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.220513Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.224065Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.224120Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.224163Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.224266Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.227691Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.227752Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.227798Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.227919Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.231465Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.231544Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.231593Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.231753Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.236529Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.236628Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.236694Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.236846Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.241064Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.241182Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.241267Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.241404Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.245626Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.245755Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.245844Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.245997Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.250906Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.251041Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.251160Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.251333Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.256903Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.257091Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.257237Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.257415Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.262280Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.262444Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.262604Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.262822Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.268905Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.269090Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.269264Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.269472Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.275399Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.275612Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.275796Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.276017Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.282792Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.283019Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.283196Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.283412Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.288778Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.288945Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.289113Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.289326Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.294460Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.294628Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.294778Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.294981Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.300490Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.300678Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.300846Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.301042Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.308129Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.308293Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.308462Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:03.308628Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:03.314291Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
[2m2024-11-03T16:43:03.314462Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Started picking witness vector generator job
[2m2024-11-03T16:43:03.320508Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_picker[0m[2m:[0m Finished picking witness vector generator job
allocated 262144 bytes on device
allocated 24838668288 bytes on device
allocated 65536 bytes on host
allocated 2147483648 bytes on host
[2m2024-11-03T16:43:04.308580Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:07.166954Z[0m [32m INFO[0m [2mzksync_circuit_prover[0m[2m:[0m Stop signal received, shutting down...
[2m2024-11-03T16:43:09.269934Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:09.270022Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:09.270067Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:09.270092Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:09.270116Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:09.270127Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:09.270130Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:09.270190Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
[2m2024-11-03T16:43:09.302002Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::job_picker_task[0m[2m:[0m Stop signal received, shutting down type JobPickerTask...
[2m2024-11-03T16:43:09.302028Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:09.304182Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:09.304233Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:09.304280Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:09.304288Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:09.304294Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:09.304303Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:09.304308Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
determined cache strategy: CacheStrategy { setup: CacheEvaluationsAndAllCosets, trace: CacheMonomialsAndFriCosets, arguments: CacheMonomialsAndFriCosets }
◆ setup: 308.239521ms
[2m2024-11-03T16:43:09.735129Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:09.735231Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:09.735258Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:09.735268Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:09.777177Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:09.777258Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:09.777264Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
◆ proof: 326.213645ms
[2m2024-11-03T16:43:09.966279Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:09.966343Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:09.966353Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:09.966374Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:09.966382Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:09.966425Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 6.972µs
[2m2024-11-03T16:43:09.974305Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
◆ proof: 280.469016ms
[2m2024-11-03T16:43:10.299323Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:10.299444Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:10.299461Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:10.299469Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:10.299501Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 5.293µs
[2m2024-11-03T16:43:10.306076Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
[2m2024-11-03T16:43:10.422488Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:10.422600Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:10.422615Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:10.422622Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:10.557718Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:10.557796Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:10.557812Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
◆ proof: 298.581402ms
[2m2024-11-03T16:43:10.634721Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:10.634789Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:10.634801Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:10.634820Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:10.634828Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:10.634851Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 4.701µs
[2m2024-11-03T16:43:10.638495Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
◆ proof: 272.401958ms
[2m2024-11-03T16:43:10.964434Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:10.964531Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:10.964572Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:10.964574Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:10.964595Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 5.089µs
[2m2024-11-03T16:43:10.968408Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
◆ proof: 277.392216ms
[2m2024-11-03T16:43:11.287392Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:11.287519Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:11.287542Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:11.287548Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 4.03µs
[2m2024-11-03T16:43:11.291957Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
◆ proof: 274.847721ms
[2m2024-11-03T16:43:11.620629Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:11.620727Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:11.626710Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
[2m2024-11-03T16:43:11.855625Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:11.855727Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:11.855752Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:11.855759Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:11.855758Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::job_picker_task[0m[2m:[0m Stop signal received, shutting down type JobPickerTask...
[2m2024-11-03T16:43:11.855779Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:11.855794Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:11.855811Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:11.855818Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:11.855878Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 7.983µs
[2m2024-11-03T16:43:11.897939Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:11.898074Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:11.898375Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:11.898376Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:11.898397Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:11.898406Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:11.898423Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
◆ proof: 285.395741ms
[2m2024-11-03T16:43:12.182683Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:12.182801Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:12.182820Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:12.182829Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:12.226802Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:12.226878Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:12.226881Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:12.226917Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:12.226936Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 3.876µs
[2m2024-11-03T16:43:12.232431Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
◆ proof: 291.459595ms
[2m2024-11-03T16:43:12.524730Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:12.524863Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:12.524885Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:12.524895Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:12.557204Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:12.557289Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:12.557302Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:12.557332Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:12.557387Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 6.429µs
[2m2024-11-03T16:43:12.562937Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
[2m2024-11-03T16:43:12.662495Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:12.662615Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:12.662633Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:12.662640Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:12.759114Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:12.759208Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:12.759256Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
◆ proof: 275.136062ms
[2m2024-11-03T16:43:12.867652Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:12.867770Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:12.867795Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:12.875406Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:12.875468Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:12.875479Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:12.875499Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:12.875511Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:12.875520Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 8.267µs
[2m2024-11-03T16:43:12.881034Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
[2m2024-11-03T16:43:12.936006Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:12.936098Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:12.936114Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:13.131593Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:13.131703Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:13.131723Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
◆ proof: 278.080501ms
[2m2024-11-03T16:43:13.183501Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:13.183583Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:13.183594Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:13.183616Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
[2m2024-11-03T16:43:13.183620Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:13.183639Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 4.815µs
[2m2024-11-03T16:43:13.187578Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
[2m2024-11-03T16:43:13.333047Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:13.333147Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:13.333164Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:13.445597Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:13.445686Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:13.445698Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
◆ proof: 271.628191ms
[2m2024-11-03T16:43:13.488823Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:13.488939Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:13.488944Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:13.488995Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
[2m2024-11-03T16:43:13.488998Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:13.489035Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 8.267µs
[2m2024-11-03T16:43:13.495069Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
[2m2024-11-03T16:43:13.562673Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:13.562815Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:13.562850Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:13.677599Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:13.677828Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:13.677863Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
◆ proof: 276.338966ms
[2m2024-11-03T16:43:13.846053Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:13.846150Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:13.846157Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:13.846204Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
[2m2024-11-03T16:43:13.846211Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:13.846248Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:13.863134Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:13.937265Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:13.937313Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:13.942631Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:13.942736Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:13.986078Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:13.997906Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
[2m2024-11-03T16:43:14.092213Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:14.097525Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:14.144488Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
reusing cache strategy
reusing setup cache
◆ setup: 5.655µs
[2m2024-11-03T16:43:14.184883Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
◆ proof: 270.718538ms
[2m2024-11-03T16:43:14.504811Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:14.505039Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:14.505116Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:14.505132Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
[2m2024-11-03T16:43:14.505192Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:14.505242Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:14.505303Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:14.505351Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:14.510503Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 18.691µs
◆ proof: 278.386256ms
[2m2024-11-03T16:43:14.812805Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:14.812858Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:14.812878Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
[2m2024-11-03T16:43:14.812881Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:14.812891Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:14.812899Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:14.812903Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:14.812906Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
reusing cache strategy
reusing setup cache
◆ setup: 3.638µs
[2m2024-11-03T16:43:14.815946Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
◆ proof: 269.983982ms
[2m2024-11-03T16:43:15.111422Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:15.111517Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:15.111541Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:15.111547Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:15.111555Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:15.111567Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:15.111582Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:15.111591Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 6.728µs
[2m2024-11-03T16:43:15.114923Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
[2m2024-11-03T16:43:15.314944Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
◆ proof: 272.978611ms
[2m2024-11-03T16:43:15.412258Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:15.412312Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:15.412316Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:15.412346Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:15.412346Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
[2m2024-11-03T16:43:15.412359Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:15.412376Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:15.412391Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
reusing cache strategy
reusing setup cache
◆ setup: 5.517µs
[2m2024-11-03T16:43:15.426715Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
◆ proof: 269.818209ms
[2m2024-11-03T16:43:15.712217Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:15.712279Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:15.712299Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:15.712306Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:15.712305Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
[2m2024-11-03T16:43:15.712313Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:15.712327Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:15.712335Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 3.192µs
[2m2024-11-03T16:43:15.715792Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
[2m2024-11-03T16:43:15.808965Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
◆ proof: 271.528242ms
[2m2024-11-03T16:43:16.011123Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:16.011236Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:16.011275Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:16.011287Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:16.011302Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:16.011307Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:16.011316Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:16.011342Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 2.965µs
[2m2024-11-03T16:43:16.014936Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
◆ proof: 269.56149ms
[2m2024-11-03T16:43:16.310260Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:16.310315Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:16.310335Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:16.310341Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:16.310339Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
[2m2024-11-03T16:43:16.310349Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:16.310354Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:16.310357Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 3.265µs
[2m2024-11-03T16:43:16.314816Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
◆ proof: 270.045719ms
[2m2024-11-03T16:43:16.612711Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:16.612762Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:16.612782Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:16.612781Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
[2m2024-11-03T16:43:16.612788Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:16.612797Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:16.612803Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 3.318µs
[2m2024-11-03T16:43:16.615879Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
◆ proof: 269.285261ms
[2m2024-11-03T16:43:16.913390Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:16.913438Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:16.913456Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:16.913461Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:16.913467Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:16.913535Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
reusing cache strategy
[2m2024-11-03T16:43:16.916426Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
reusing setup cache
◆ setup: 10.9µs
◆ proof: 269.616627ms
[2m2024-11-03T16:43:17.215625Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:17.215691Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:17.215717Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
[2m2024-11-03T16:43:17.215719Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:17.215730Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:17.215737Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 3.603µs
[2m2024-11-03T16:43:17.219226Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
[2m2024-11-03T16:43:17.376646Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:17.376725Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:17.376736Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
[2m2024-11-03T16:43:17.442311Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_executor[0m[2m:[0m Started executing witness vector generator job
[2m2024-11-03T16:43:17.442461Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Started saving witness vector generator job
[2m2024-11-03T16:43:17.442477Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::witness_vector_generator::witness_vector_generator_job_saver[0m[2m:[0m Finished saving witness vector generator job
◆ proof: 272.323034ms
[2m2024-11-03T16:43:17.520600Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:17.520718Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:17.520723Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:17.520745Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
[2m2024-11-03T16:43:17.520753Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:17.520765Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 3.622µs
[2m2024-11-03T16:43:17.523548Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
◆ proof: 268.988466ms
[2m2024-11-03T16:43:17.819244Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:17.819296Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:17.819316Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:17.819316Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
[2m2024-11-03T16:43:17.819322Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:17.819329Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 3.929µs
[2m2024-11-03T16:43:17.822418Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
◆ proof: 270.614795ms
[2m2024-11-03T16:43:18.120666Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:18.120717Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:18.120726Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:18.120743Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:18.120749Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:18.120803Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 6.511µs
[2m2024-11-03T16:43:18.123432Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
◆ proof: 270.418486ms
[2m2024-11-03T16:43:18.418163Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:18.418219Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:18.418225Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:18.418245Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
[2m2024-11-03T16:43:18.418250Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:18.418261Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 4.11µs
[2m2024-11-03T16:43:18.420831Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
◆ proof: 270.343184ms
[2m2024-11-03T16:43:18.721847Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:18.721909Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:18.721935Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:18.721941Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Finished picking gpu circuit prover job
[2m2024-11-03T16:43:18.721947Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:18.721963Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 4.877µs
[2m2024-11-03T16:43:18.725315Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
◆ proof: 270.642152ms
[2m2024-11-03T16:43:19.022393Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:19.022448Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:19.022457Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:19.022476Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_picker[0m[2m:[0m Started picking gpu circuit prover job
[2m2024-11-03T16:43:19.022542Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
[2m2024-11-03T16:43:19.024605Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 4.406µs
◆ proof: 272.194258ms
[2m2024-11-03T16:43:19.445574Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:19.445700Z[0m [32m INFO[0m [2mzksync_prover_job_processor::task_wiring::worker_pool[0m[2m:[0m got 1 value
[2m2024-11-03T16:43:19.445719Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:19.445749Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Started executing gpu circuit prover job
[2m2024-11-03T16:43:19.449269Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
reusing cache strategy
reusing setup cache
◆ setup: 5.225µs
[2m2024-11-03T16:43:19.691407Z[0m [31mERROR[0m [2mzksync_utils::wait_for_tasks[0m[2m:[0m One of actors returned an error during shutdown: failed to pick job

Caused by:
    no Witness Vector Generators are available
◆ proof: 272.086634ms
[2m2024-11-03T16:43:19.813411Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_executor[0m[2m:[0m Finished executing gpu circuit prover job
[2m2024-11-03T16:43:19.941593Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Started saving gpu circuit prover job
[2m2024-11-03T16:43:19.944617Z[0m [32m INFO[0m [2mzksync_circuit_prover_service::gpu_circuit_prover::gpu_circuit_prover_job_saver[0m[2m:[0m Finished saving gpu circuit prover job
