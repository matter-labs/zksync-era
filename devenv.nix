{ pkgs, lib, config, inputs, ... }:

let
  meta = {
    zksync-version = {
      minor = "25";
      patch = "0";
    };
  };

  # PgSQL settings
  db = {
    port = 4897;
    user = "zksync";
    password = "zksyncpass";
    hostname = "localhost";
    name = "zksync";
  };
  db-url = "postgres://${db.user}:${db.password}@localhost:${toString db.port}/${db.name}";

  # S3/Minio settings
  s3 = {
    bucket = "mon-gros-bucket";
    endpoint = "http://localhost:9000";
    region = "us-east-2";
    access-key = "majolieclef";
    secret-key = "grossecret";
    time-out = 1000;
  };

  # Maps to `secrets.yaml`
  zksync-secrets-config = {
    database = {
      prover_url = db-url;
    };
  };

  # Maps to `general.yaml`
  zksync-general-config = {
    postgres = {
      max_connections = 100;
      statement_timeout_sec = 300;
    };

    witness_generator = {
      generation_timeout_in_secs = 900;
      max_attempts = 10;
      shall_save_to_public_bucket = false;
      prometheus_listener_port = 3116;
      max_circuits_in_flight = 500;
    };

    prover = {
      setup_data_path = "data/keys";
      prometheus_port = 3315;
      max_attempts = 10;
      generation_timeout_in_secs = 600;
      setup_load_mode = "FROM_DISK";
      specialized_group_id= 100;
      queue_capacity= 10;
      witness_vector_receiver_port= 3316;
      zone_read_url= "http://metadata.google.internal/computeMetadata/v1/instance/zone";
      shall_save_to_public_bucket= false;
      availability_check_interval_in_secs= 10000;
      public_object_store= {
        file_backed = {
          file_backed_base_path = "artifacts";
        };
        max_retries= 10;
      };
      prover_object_store= {
        # file_backed = {
        #   file_backed_base_path = "artifacts";
        # };
        s3_credentials = {
          endpoint = s3.endpoint;
          region = s3.region;
          bucket = s3.bucket;
          secret_key = s3.secret-key;
          access_key = s3.access-key;
        };
        max_retries= 10;
      };
      cloud_type= "LOCAL";
    };

    observability = {
      log_format = "plain";
      log_directives = " zksync_circuit_prover=trace,zksync_node_test_utils=info,zksync_state_keeper=info,zksync_reorg_detector=info,zksync_consistency_checker=info,zksync_metadata_calculator=info,zksync_node_sync=info,zksync_node_consensus=info,zksync_contract_verification_server=info,zksync_node_api_server=info,zksync_node_framework=info,zksync_block_reverter=info,zksync_commitment_generator=debug,zksync_node_db_pruner=info,zksync_eth_sender=info,zksync_node_fee_model=info,zksync_node_genesis=info,zksync_house_keeper=info,zksync_proof_data_handler=info,zksync_shared_metrics=info,zksync_node_test_utils=info,zksync_vm_runner=info,zksync_consensus_bft=info,zksync_consensus_network=info,zksync_consensus_storage=info,zksync_core_leftovers=debug,zksync_server=debug,zksync_contract_verifier=debug,zksync_dal=info,zksync_db_connection=info,zksync_eth_client=info,zksync_eth_watch=debug,zksync_storage=info,zksync_db_manager=info,zksync_merkle_tree=info,zksync_state=debug,zksync_utils=debug,zksync_queued_job_processor=info,zksync_types=info,zksync_mempool=debug,loadnext=info,vm=info,zksync_object_store=info,zksync_external_node=info,zksync_witness_generator=info,zksync_prover_fri=info,zksync_witness_vector_generator=info,zksync_web3_decl=debug,zksync_health_check=debug,zksync_proof_fri_compressor=info,vise_exporter=error,snapshots_creator=debug,zksync_base_token_adjuster=debug,zksync_external_price_api=debug,zksync_external_proof_integration_api=info";

    };
  };

  secrets-config-file = ((pkgs.formats.yaml {}).generate "secrets.yaml" zksync-secrets-config);
  general-config-file = ((pkgs.formats.yaml {}).generate "general.yaml" zksync-general-config);
in
{
  cachix.enable = false;

  packages = [
    pkgs.git pkgs.rustup pkgs.openssl.dev pkgs.pkg-config
    pkgs.pspg
  ]
  ++ lib.optionals pkgs.stdenv.targetPlatform.isDarwin [
    pkgs.libiconv
    pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
  ];

  env = {
    OPENSSL_DEV = pkgs.openssl.dev;
    ZKSYNC_USE_CUDA_STUBS = "true";
    PSQL_PAGER = "pspg -X -b";
  };

  scripts = {
    # Display the general config file location
    yaml-config.exec = "echo ${general-config-file}";

    # Display the secrets config file location
    yaml-secrets.exec = "echo ${secrets-config-file}";

    # Wipe out the database
    reset-db.exec = "rm -rf ${config.env.DEVENV_STATE}/postgres";

    # Open a shell to the DB
    db.exec = "psql -U ${db.user} -d ${db.name} -p ${toString db.port}";
  };

  tasks = {
    "prover:init" = {
      description = "Insert the metadata related to the given ZKstack version in the DB.";
      exec = ''
      cargo run --bin prover_cli --manifest-path prover/Cargo.toml -- \
      ${db-url} 1 \
      insert-version \
        --version ${meta.zksync-version.minor} --patch ${meta.zksync-version.patch} \
        --snark-wrapper=0x14f97b81e54b35fe673d8708cc1a19e1ea5b5e348e12d31e39824ed4f42bbca2''
      ;
    };
  };

  services = {
    postgres = {
      enable = true;
      listen_addresses = "127.0.0.1";
      port = db.port;
      settings = {
        log_connections = true;
        log_statement = "all";
      };
      initialScript = ''
      CREATE ROLE ${db.user} SUPERUSER LOGIN;
      '';
      initialDatabases = [
        {
          name = db.name;
          user = db.user;
          pass = db.password;
          schema = ./prover/crates/lib/prover_dal/migrations;
        }
      ];
    };
  };

  processes = {
    witness-generator = {
      exec = "cargo run --release --manifest-path=prover/Cargo.toml --bin zksync_witness_generator -- --round=basic_circuits --config-path=${general-config-file} --secrets-path=${secrets-config-file}";
    };
  };
}
