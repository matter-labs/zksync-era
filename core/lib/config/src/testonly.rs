use rand::{distributions::Distribution, Rng};
use secrecy::Secret;
use zksync_basic_types::{
    commitment::L1BatchCommitmentMode,
    network::Network,
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId, VersionPatch},
    secrets::{APIKey, SeedPhrase},
    L1ChainId, L2ChainId,
};
use zksync_consensus_utils::EncodeDist;
use zksync_crypto_primitives::K256PrivateKey;

use crate::{
    configs::{
        self,
        da_client::{
            avail::{AvailClientConfig, AvailDefaultConfig},
            DAClientConfig::Avail,
        },
    },
    AvailConfig,
};

trait Sample {
    fn sample(rng: &mut (impl Rng + ?Sized)) -> Self;
}

impl Sample for Network {
    fn sample(rng: &mut (impl Rng + ?Sized)) -> Network {
        type T = Network;
        match rng.gen_range(0..8) {
            0 => T::Mainnet,
            1 => T::Rinkeby,
            2 => T::Ropsten,
            3 => T::Goerli,
            4 => T::Sepolia,
            5 => T::Localhost,
            6 => T::Unknown,
            _ => T::Test,
        }
    }
}

impl Distribution<configs::chain::FeeModelVersion> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::chain::FeeModelVersion {
        type T = configs::chain::FeeModelVersion;
        match rng.gen_range(0..2) {
            0 => T::V1,
            _ => T::V2,
        }
    }
}

impl Distribution<configs::ContractsConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::ContractsConfig {
        configs::ContractsConfig {
            governance_addr: rng.gen(),
            verifier_addr: rng.gen(),
            default_upgrade_addr: rng.gen(),
            diamond_proxy_addr: rng.gen(),
            validator_timelock_addr: rng.gen(),
            l1_erc20_bridge_proxy_addr: self.sample_opt(|| rng.gen()),
            l2_erc20_bridge_addr: self.sample_opt(|| rng.gen()),
            l1_shared_bridge_proxy_addr: self.sample_opt(|| rng.gen()),
            l2_shared_bridge_addr: self.sample_opt(|| rng.gen()),
            l2_legacy_shared_bridge_addr: self.sample_opt(|| rng.gen()),
            l1_weth_bridge_proxy_addr: self.sample_opt(|| rng.gen()),
            l2_weth_bridge_addr: self.sample_opt(|| rng.gen()),
            l2_testnet_paymaster_addr: self.sample_opt(|| rng.gen()),
            l2_timestamp_asserter_addr: self.sample_opt(|| rng.gen()),
            l1_multicall3_addr: rng.gen(),
            ecosystem_contracts: self.sample(rng),
            base_token_addr: self.sample_opt(|| rng.gen()),
            chain_admin_addr: self.sample_opt(|| rng.gen()),
            l2_da_validator_addr: self.sample_opt(|| rng.gen()),
        }
    }
}

impl Distribution<configs::GenesisConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::GenesisConfig {
        configs::GenesisConfig {
            protocol_version: Some(ProtocolSemanticVersion {
                minor: ProtocolVersionId::try_from(
                    rng.gen_range(0..(ProtocolVersionId::latest() as u16)),
                )
                .unwrap(),
                patch: VersionPatch(rng.gen()),
            }),
            genesis_root_hash: Some(rng.gen()),
            rollup_last_leaf_index: Some(self.sample(rng)),
            genesis_commitment: Some(rng.gen()),
            bootloader_hash: Some(rng.gen()),
            default_aa_hash: Some(rng.gen()),
            evm_emulator_hash: Some(rng.gen()),
            fee_account: rng.gen(),
            l1_chain_id: L1ChainId(self.sample(rng)),
            sl_chain_id: None,
            l2_chain_id: L2ChainId::default(),
            snark_wrapper_vk_hash: rng.gen(),
            dummy_verifier: rng.gen(),
            l1_batch_commit_data_generator_mode: match rng.gen_range(0..2) {
                0 => L1BatchCommitmentMode::Rollup,
                _ => L1BatchCommitmentMode::Validium,
            },
        }
    }
}

impl Distribution<configs::EcosystemContracts> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::EcosystemContracts {
        configs::EcosystemContracts {
            bridgehub_proxy_addr: rng.gen(),
            state_transition_proxy_addr: rng.gen(),
            transparent_proxy_admin_addr: rng.gen(),
        }
    }
}

impl Distribution<configs::consensus::WeightedValidator> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::consensus::WeightedValidator {
        use configs::consensus::{ValidatorPublicKey, WeightedValidator};
        WeightedValidator {
            key: ValidatorPublicKey(self.sample(rng)),
            weight: self.sample(rng),
        }
    }
}

impl Distribution<configs::consensus::WeightedAttester> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::consensus::WeightedAttester {
        use configs::consensus::{AttesterPublicKey, WeightedAttester};
        WeightedAttester {
            key: AttesterPublicKey(self.sample(rng)),
            weight: self.sample(rng),
        }
    }
}

impl Distribution<configs::consensus::GenesisSpec> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::consensus::GenesisSpec {
        use configs::consensus::{
            GenesisSpec, Host, NodePublicKey, ProtocolVersion, ValidatorPublicKey,
        };
        GenesisSpec {
            chain_id: L2ChainId::default(),
            protocol_version: ProtocolVersion(self.sample(rng)),
            validators: self.sample_collect(rng),
            attesters: self.sample_collect(rng),
            leader: ValidatorPublicKey(self.sample(rng)),
            registry_address: self.sample_opt(|| rng.gen()),
            seed_peers: self
                .sample_range(rng)
                .map(|_| (NodePublicKey(self.sample(rng)), Host(self.sample(rng))))
                .collect(),
        }
    }
}

impl Distribution<configs::consensus::ConsensusConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::consensus::ConsensusConfig {
        use configs::consensus::{ConsensusConfig, Host, NodePublicKey};
        ConsensusConfig {
            port: self.sample(rng),
            server_addr: self.sample(rng),
            public_addr: Host(self.sample(rng)),
            max_payload_size: self.sample(rng),
            max_batch_size: self.sample(rng),
            gossip_dynamic_inbound_limit: self.sample(rng),
            gossip_static_inbound: self
                .sample_range(rng)
                .map(|_| NodePublicKey(self.sample(rng)))
                .collect(),
            gossip_static_outbound: self
                .sample_range(rng)
                .map(|_| (NodePublicKey(self.sample(rng)), Host(self.sample(rng))))
                .collect(),
            genesis_spec: self.sample(rng),
            rpc: self.sample(rng),
            debug_page_addr: self.sample(rng),
        }
    }
}

impl Distribution<configs::consensus::RpcConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::consensus::RpcConfig {
        configs::consensus::RpcConfig {
            get_block_rate: self.sample(rng),
        }
    }
}

impl Distribution<configs::consensus::ConsensusSecrets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::consensus::ConsensusSecrets {
        use configs::consensus::{
            AttesterSecretKey, ConsensusSecrets, NodeSecretKey, ValidatorSecretKey,
        };
        ConsensusSecrets {
            validator_key: self.sample_opt(|| ValidatorSecretKey(String::into(self.sample(rng)))),
            attester_key: self.sample_opt(|| AttesterSecretKey(String::into(self.sample(rng)))),
            node_key: self.sample_opt(|| NodeSecretKey(String::into(self.sample(rng)))),
        }
    }
}

impl Distribution<configs::secrets::L1Secrets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::secrets::L1Secrets {
        use configs::secrets::L1Secrets;
        L1Secrets {
            l1_rpc_url: format!("localhost:{}", rng.gen::<u16>()).parse().unwrap(),
        }
    }
}

impl Distribution<configs::secrets::DatabaseSecrets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::secrets::DatabaseSecrets {
        use configs::secrets::DatabaseSecrets;
        DatabaseSecrets {
            server_url: Some(format!("localhost:{}", rng.gen::<u16>()).parse().unwrap()),
            server_replica_url: Some(format!("localhost:{}", rng.gen::<u16>()).parse().unwrap()),
            prover_url: Some(format!("localhost:{}", rng.gen::<u16>()).parse().unwrap()),
        }
    }
}

impl Distribution<configs::secrets::Secrets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::secrets::Secrets {
        use configs::secrets::Secrets;
        Secrets {
            consensus: self.sample_opt(|| self.sample(rng)),
            database: self.sample_opt(|| self.sample(rng)),
            l1: self.sample_opt(|| self.sample(rng)),
            data_availability: self.sample_opt(|| self.sample(rng)),
        }
    }
}

impl Distribution<configs::wallets::Wallet> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::Wallet {
        configs::wallets::Wallet::new(K256PrivateKey::from_bytes(rng.gen()).unwrap())
    }
}

impl Distribution<configs::wallets::AddressWallet> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::AddressWallet {
        configs::wallets::AddressWallet::from_address(rng.gen())
    }
}

impl Distribution<configs::wallets::StateKeeper> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::StateKeeper {
        configs::wallets::StateKeeper {
            fee_account: self.sample(rng),
        }
    }
}

impl Distribution<configs::wallets::EthSender> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::EthSender {
        configs::wallets::EthSender {
            operator: self.sample(rng),
            blob_operator: self.sample_opt(|| self.sample(rng)),
        }
    }
}

impl Distribution<configs::wallets::TokenMultiplierSetter> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::TokenMultiplierSetter {
        configs::wallets::TokenMultiplierSetter {
            wallet: self.sample(rng),
        }
    }
}

impl Distribution<configs::wallets::Wallets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::Wallets {
        configs::wallets::Wallets {
            state_keeper: self.sample_opt(|| self.sample(rng)),
            eth_sender: self.sample_opt(|| self.sample(rng)),
            token_multiplier_setter: self.sample_opt(|| self.sample(rng)),
        }
    }
}

impl Distribution<configs::en_config::ENConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::en_config::ENConfig {
        configs::en_config::ENConfig {
            l2_chain_id: L2ChainId::default(),
            l1_chain_id: L1ChainId(rng.gen()),
            sl_chain_id: None,
            main_node_url: format!("localhost:{}", rng.gen::<u16>()).parse().unwrap(),
            l1_batch_commit_data_generator_mode: match rng.gen_range(0..2) {
                0 => L1BatchCommitmentMode::Rollup,
                _ => L1BatchCommitmentMode::Validium,
            },
            main_node_rate_limit_rps: self.sample_opt(|| rng.gen()),
            gateway_url: self
                .sample_opt(|| format!("localhost:{}", rng.gen::<u16>()).parse().unwrap()),
            bridge_addresses_refresh_interval_sec: self.sample_opt(|| rng.gen()),
        }
    }
}

impl Distribution<configs::da_client::DAClientConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::da_client::DAClientConfig {
        Avail(AvailConfig {
            bridge_api_url: self.sample(rng),
            timeout_ms: self.sample(rng),
            config: AvailClientConfig::FullClient(AvailDefaultConfig {
                api_node_url: self.sample(rng),
                app_id: self.sample(rng),
            }),
        })
    }
}

impl Distribution<configs::secrets::DataAvailabilitySecrets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::secrets::DataAvailabilitySecrets {
        configs::secrets::DataAvailabilitySecrets::Avail(configs::da_client::avail::AvailSecrets {
            seed_phrase: Some(SeedPhrase(Secret::new(self.sample(rng)))),
            gas_relay_api_key: Some(APIKey(Secret::new(self.sample(rng)))),
        })
    }
}

impl Distribution<configs::GeneralConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::GeneralConfig {
        configs::GeneralConfig {
            postgres_config: None,
            api_config: None,
            contract_verifier: None,
            circuit_breaker_config: None,
            mempool_config: None,
            operations_manager_config: None,
            state_keeper_config: None,
            house_keeper_config: None,
            proof_compressor_config: None,
            prover_config: None,
            prover_gateway: None,
            witness_vector_generator: None,
            prover_group_config: None,
            witness_generator_config: None,
            prometheus_config: None,
            proof_data_handler_config: None,
            db_config: None,
            eth: None,
            snapshot_creator: None,
            observability: None,
            da_client_config: self.sample(rng),
            da_dispatcher_config: None,
            protective_reads_writer_config: None,
            basic_witness_input_producer_config: None,
            commitment_generator: None,
            snapshot_recovery: None,
            pruning: None,
            core_object_store: None,
            base_token_adjuster: None,
            external_price_api_client_config: None,
            consensus_config: self.sample(rng),
            external_proof_integration_api_config: None,
            experimental_vm_config: None,
            prover_job_monitor_config: None,
            timestamp_asserter_config: None,
        }
    }
}
