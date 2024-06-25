use anyhow::Context as _;
use zksync_config::configs::GeneralConfig;
use zksync_protobuf::ProtoRepr;

use crate::{proto::general as proto, read_optional_repr};

impl ProtoRepr for proto::GeneralConfig {
    type Type = GeneralConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            postgres_config: read_optional_repr(&self.postgres).context("postgres")?,
            contract_verifier: read_optional_repr(&self.contract_verifier).context("postgres")?,
            circuit_breaker_config: read_optional_repr(&self.circuit_breaker)
                .context("circuit_breaker")?,
            mempool_config: read_optional_repr(&self.mempool).context("mempool")?,
            operations_manager_config: read_optional_repr(&self.operations_manager)
                .context("operations_manager")?,
            state_keeper_config: read_optional_repr(&self.state_keeper).context("state_keeper")?,
            house_keeper_config: read_optional_repr(&self.house_keeper).context("house_keeper")?,
            proof_compressor_config: read_optional_repr(&self.proof_compressor)
                .context("proof_compressor_config")?,
            prover_config: read_optional_repr(&self.prover).context("prover_config")?,
            prover_gateway: read_optional_repr(&self.prover_gateway).context("prover_gateway")?,
            witness_vector_generator: read_optional_repr(&self.witness_vector_generator)
                .context("witness_vector_generator")?,
            prover_group_config: read_optional_repr(&self.prover_group)
                .context("prover_group_config")?,
            prometheus_config: read_optional_repr(&self.prometheus).context("prometheus")?,
            proof_data_handler_config: read_optional_repr(&self.data_handler)
                .context("proof_data_handler")?,
            witness_generator: read_optional_repr(&self.witness_generator)
                .context("witness_generator")?,
            api_config: read_optional_repr(&self.api).context("api")?,
            db_config: read_optional_repr(&self.db).context("db")?,
            eth: read_optional_repr(&self.eth).context("eth")?,
            snapshot_creator: read_optional_repr(&self.snapshot_creator)
                .context("snapshot_creator")?,
            observability: read_optional_repr(&self.observability).context("observability")?,
            protective_reads_writer_config: read_optional_repr(&self.protective_reads_writer)
                .context("protective_reads_writer")?,
            core_object_store: read_optional_repr(&self.core_object_store)
                .context("core_object_store")?,
            base_token_adjuster: read_optional_repr(&self.base_token_adjuster)
                .context("base_token_adjuster")?,
            commitment_generator: read_optional_repr(&self.commitment_generator)
                .context("commitment_generator")?,
            pruning: read_optional_repr(&self.pruning).context("pruning")?,
            snapshot_recovery: read_optional_repr(&self.snapshot_recovery)
                .context("snapshot_recovery")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            postgres: this.postgres_config.as_ref().map(ProtoRepr::build),
            circuit_breaker: this.circuit_breaker_config.as_ref().map(ProtoRepr::build),
            mempool: this.mempool_config.as_ref().map(ProtoRepr::build),
            contract_verifier: this.contract_verifier.as_ref().map(ProtoRepr::build),
            operations_manager: this
                .operations_manager_config
                .as_ref()
                .map(ProtoRepr::build),
            state_keeper: this.state_keeper_config.as_ref().map(ProtoRepr::build),
            house_keeper: this.house_keeper_config.as_ref().map(ProtoRepr::build),
            proof_compressor: this.proof_compressor_config.as_ref().map(ProtoRepr::build),
            prover: this.prover_config.as_ref().map(ProtoRepr::build),
            prover_group: this.prover_group_config.as_ref().map(ProtoRepr::build),
            witness_generator: this.witness_generator.as_ref().map(ProtoRepr::build),
            prover_gateway: this.prover_gateway.as_ref().map(ProtoRepr::build),
            witness_vector_generator: this.witness_vector_generator.as_ref().map(ProtoRepr::build),
            prometheus: this.prometheus_config.as_ref().map(ProtoRepr::build),
            data_handler: this
                .proof_data_handler_config
                .as_ref()
                .map(ProtoRepr::build),
            api: this.api_config.as_ref().map(ProtoRepr::build),
            db: this.db_config.as_ref().map(ProtoRepr::build),
            eth: this.eth.as_ref().map(ProtoRepr::build),
            snapshot_creator: this.snapshot_creator.as_ref().map(ProtoRepr::build),
            observability: this.observability.as_ref().map(ProtoRepr::build),
            protective_reads_writer: this
                .protective_reads_writer_config
                .as_ref()
                .map(ProtoRepr::build),
            commitment_generator: this.commitment_generator.as_ref().map(ProtoRepr::build),
            snapshot_recovery: this.snapshot_recovery.as_ref().map(ProtoRepr::build),
            pruning: this.pruning.as_ref().map(ProtoRepr::build),
            core_object_store: this.core_object_store.as_ref().map(ProtoRepr::build),
            base_token_adjuster: this.base_token_adjuster.as_ref().map(ProtoRepr::build),
        }
    }
}
