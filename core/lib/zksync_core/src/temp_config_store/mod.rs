use anyhow::Context as _;
use zksync_config::{
    configs::{
        api::{HealthCheckConfig, MerkleTreeApiConfig, Web3JsonRpcConfig},
        chain::{
            CircuitBreakerConfig, MempoolConfig, NetworkConfig, OperationsManagerConfig,
            StateKeeperConfig,
        },
        fri_prover_group::FriProverGroupConfig,
        house_keeper::HouseKeeperConfig,
        native_token_fetcher::NativeTokenFetcherConfig,
        FriProofCompressorConfig, FriProverConfig, FriWitnessGeneratorConfig, PrometheusConfig,
        ProofDataHandlerConfig, WitnessGeneratorConfig,
    },
    ApiConfig, ContractsConfig, DBConfig, ETHClientConfig, ETHSenderConfig, ETHWatchConfig,
    GasAdjusterConfig, ObjectStoreConfig, PostgresConfig,
};
use zksync_protobuf::{read_optional, repr::ProtoRepr, ProtoFmt};

use crate::proto;

fn read_optional_repr<P: ProtoRepr>(field: &Option<P>) -> anyhow::Result<Option<P::Type>> {
    field.as_ref().map(|x| x.read()).transpose()
}

use crate::consensus;

#[cfg(test)]
mod tests;

/// Decodes a proto message from json for arbitrary `ProtoFmt`.
pub fn decode_json<T: ProtoFmt>(json: &str) -> anyhow::Result<T> {
    let mut d = serde_json::Deserializer::from_str(json);
    let p: T = zksync_protobuf::serde::deserialize(&mut d)?;
    d.end()?;
    Ok(p)
}

pub fn decode_yaml<T: ProtoFmt>(yaml: &str) -> anyhow::Result<T> {
    let d = serde_yaml::Deserializer::from_str(yaml);
    let this: T = zksync_protobuf::serde::deserialize(d)?;
    Ok(this)
}

// TODO (QIT-22): This structure is going to be removed when components will be responsible for their own configs.
/// A temporary config store allowing to pass deserialized configs from `zksync_server` to `zksync_core`.
/// All the configs are optional, since for some component combination it is not needed to pass all the configs.
#[derive(Debug, PartialEq)]
pub struct TempConfigStore {
    pub postgres_config: Option<PostgresConfig>,
    pub health_check_config: Option<HealthCheckConfig>,
    pub merkle_tree_api_config: Option<MerkleTreeApiConfig>,
    pub web3_json_rpc_config: Option<Web3JsonRpcConfig>,
    pub circuit_breaker_config: Option<CircuitBreakerConfig>,
    pub mempool_config: Option<MempoolConfig>,
    pub network_config: Option<NetworkConfig>,
    pub operations_manager_config: Option<OperationsManagerConfig>,
    pub state_keeper_config: Option<StateKeeperConfig>,
    pub house_keeper_config: Option<HouseKeeperConfig>,
    pub fri_proof_compressor_config: Option<FriProofCompressorConfig>,
    pub fri_prover_config: Option<FriProverConfig>,
    pub fri_prover_group_config: Option<FriProverGroupConfig>,
    pub fri_witness_generator_config: Option<FriWitnessGeneratorConfig>,
    pub prometheus_config: Option<PrometheusConfig>,
    pub proof_data_handler_config: Option<ProofDataHandlerConfig>,
    pub witness_generator_config: Option<WitnessGeneratorConfig>,
    pub api_config: Option<ApiConfig>,
    pub contracts_config: Option<ContractsConfig>,
    pub db_config: Option<DBConfig>,
    pub eth_client_config: Option<ETHClientConfig>,
    pub eth_sender_config: Option<ETHSenderConfig>,
    pub eth_watch_config: Option<ETHWatchConfig>,
    pub gas_adjuster_config: Option<GasAdjusterConfig>,
    pub object_store_config: Option<ObjectStoreConfig>,
    pub consensus_config: Option<consensus::Config>,
    pub native_token_fetcher_config: Option<NativeTokenFetcherConfig>,
}

impl ProtoFmt for TempConfigStore {
    type Proto = proto::TempConfigStore;
    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        Ok(Self {
            postgres_config: read_optional_repr(&r.postgres).context("postgres")?,
            health_check_config: read_optional_repr(&r.health_check).context("health_check")?,
            merkle_tree_api_config: read_optional_repr(&r.merkle_tree_api)
                .context("merkle_tree_api")?,
            web3_json_rpc_config: read_optional_repr(&r.web3_json_rpc).context("web3_json_rpc")?,
            circuit_breaker_config: read_optional_repr(&r.circuit_breaker)
                .context("circuit_breaker")?,
            mempool_config: read_optional_repr(&r.mempool).context("mempool")?,
            network_config: read_optional_repr(&r.network).context("network")?,
            operations_manager_config: read_optional_repr(&r.operations_manager)
                .context("operations_manager")?,
            state_keeper_config: read_optional_repr(&r.state_keeper).context("state_keeper")?,
            house_keeper_config: read_optional_repr(&r.house_keeper).context("house_keeper")?,
            fri_proof_compressor_config: read_optional_repr(&r.fri_proof_compressor)
                .context("fri_proof_compressor")?,
            fri_prover_config: read_optional_repr(&r.fri_prover).context("fri_prover")?,
            fri_prover_group_config: read_optional_repr(&r.fri_prover_group)
                .context("fri_prover_group")?,
            fri_witness_generator_config: read_optional_repr(&r.fri_witness_generator)
                .context("fri_witness_generator")?,
            prometheus_config: read_optional_repr(&r.prometheus).context("prometheus")?,
            proof_data_handler_config: read_optional_repr(&r.proof_data_handler)
                .context("proof_data_handler")?,
            witness_generator_config: read_optional_repr(&r.witness_generator)
                .context("witness_generator")?,
            api_config: read_optional_repr(&r.api).context("api")?,
            contracts_config: read_optional_repr(&r.contracts).context("contracts")?,
            db_config: read_optional_repr(&r.db).context("db")?,
            eth_client_config: read_optional_repr(&r.eth_client).context("eth_client")?,
            eth_sender_config: read_optional_repr(&r.eth_sender).context("eth_sender")?,
            eth_watch_config: read_optional_repr(&r.eth_watch).context("eth_watch")?,
            gas_adjuster_config: read_optional_repr(&r.gas_adjuster).context("gas_adjuster")?,
            object_store_config: read_optional_repr(&r.object_store).context("object_store")?,
            consensus_config: read_optional(&r.consensus).context("consensus")?,
            native_token_fetcher_config: read_optional_repr(&r.native_token_fetcher)
                .context("native_token_fetcher")?,
        })
    }

    fn build(&self) -> Self::Proto {
        Self::Proto {
            postgres: self.postgres_config.as_ref().map(ProtoRepr::build),
            health_check: self.health_check_config.as_ref().map(ProtoRepr::build),
            merkle_tree_api: self.merkle_tree_api_config.as_ref().map(ProtoRepr::build),
            web3_json_rpc: self.web3_json_rpc_config.as_ref().map(ProtoRepr::build),
            circuit_breaker: self.circuit_breaker_config.as_ref().map(ProtoRepr::build),
            mempool: self.mempool_config.as_ref().map(ProtoRepr::build),
            network: self.network_config.as_ref().map(ProtoRepr::build),
            operations_manager: self
                .operations_manager_config
                .as_ref()
                .map(ProtoRepr::build),
            state_keeper: self.state_keeper_config.as_ref().map(ProtoRepr::build),
            house_keeper: self.house_keeper_config.as_ref().map(ProtoRepr::build),
            fri_proof_compressor: self
                .fri_proof_compressor_config
                .as_ref()
                .map(ProtoRepr::build),
            fri_prover: self.fri_prover_config.as_ref().map(ProtoRepr::build),
            fri_prover_group: self.fri_prover_group_config.as_ref().map(ProtoRepr::build),
            fri_witness_generator: self
                .fri_witness_generator_config
                .as_ref()
                .map(ProtoRepr::build),
            prometheus: self.prometheus_config.as_ref().map(ProtoRepr::build),
            proof_data_handler: self
                .proof_data_handler_config
                .as_ref()
                .map(ProtoRepr::build),
            witness_generator: self.witness_generator_config.as_ref().map(ProtoRepr::build),
            api: self.api_config.as_ref().map(ProtoRepr::build),
            contracts: self.contracts_config.as_ref().map(ProtoRepr::build),
            db: self.db_config.as_ref().map(ProtoRepr::build),
            eth_client: self.eth_client_config.as_ref().map(ProtoRepr::build),
            eth_sender: self.eth_sender_config.as_ref().map(ProtoRepr::build),
            eth_watch: self.eth_watch_config.as_ref().map(ProtoRepr::build),
            gas_adjuster: self.gas_adjuster_config.as_ref().map(ProtoRepr::build),
            object_store: self.object_store_config.as_ref().map(ProtoRepr::build),
            consensus: self.consensus_config.as_ref().map(ProtoFmt::build),
            native_token_fetcher: self
                .native_token_fetcher_config
                .as_ref()
                .map(ProtoRepr::build),
        }
    }
}

#[derive(Debug)]
pub struct Secrets {
    pub consensus: Option<consensus::Secrets>,
}

impl ProtoFmt for Secrets {
    type Proto = proto::Secrets;
    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        Ok(Self {
            consensus: read_optional(&r.consensus).context("consensus")?,
        })
    }

    fn build(&self) -> Self::Proto {
        Self::Proto {
            consensus: self.consensus.as_ref().map(|x| x.build()),
        }
    }
}
