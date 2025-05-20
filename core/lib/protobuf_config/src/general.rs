use zksync_config::configs::GeneralConfig;
use zksync_protobuf::ProtoRepr;

use crate::{proto::general as proto, read_optional_repr};

impl ProtoRepr for proto::GeneralConfig {
    type Type = GeneralConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            postgres_config: read_optional_repr(&self.postgres),
            contract_verifier: read_optional_repr(&self.contract_verifier),
            circuit_breaker_config: read_optional_repr(&self.circuit_breaker),
            mempool_config: read_optional_repr(&self.mempool),
            operations_manager_config: read_optional_repr(&self.operations_manager),
            state_keeper_config: read_optional_repr(&self.state_keeper),
            house_keeper_config: read_optional_repr(&self.house_keeper),
            proof_compressor_config: read_optional_repr(&self.proof_compressor),
            prover_config: read_optional_repr(&self.prover),
            prover_gateway: read_optional_repr(&self.prover_gateway),
            prometheus_config: read_optional_repr(&self.prometheus),
            proof_data_handler_config: read_optional_repr(&self.data_handler),
            tee_proof_data_handler_config: read_optional_repr(&self.tee_data_handler),
            witness_generator_config: read_optional_repr(&self.witness_generator),
            api_config: read_optional_repr(&self.api),
            db_config: read_optional_repr(&self.db),
            eth: read_optional_repr(&self.eth),
            snapshot_creator: read_optional_repr(&self.snapshot_creator),
            observability: read_optional_repr(&self.observability),
            da_client_config: read_optional_repr(&self.da_client),
            da_dispatcher_config: read_optional_repr(&self.da_dispatcher),
            protective_reads_writer_config: read_optional_repr(&self.protective_reads_writer),
            basic_witness_input_producer_config: read_optional_repr(
                &self.basic_witness_input_producer,
            ),
            core_object_store: read_optional_repr(&self.core_object_store),
            base_token_adjuster: read_optional_repr(&self.base_token_adjuster),
            commitment_generator: read_optional_repr(&self.commitment_generator),
            pruning: read_optional_repr(&self.pruning),
            snapshot_recovery: read_optional_repr(&self.snapshot_recovery),
            external_price_api_client_config: read_optional_repr(&self.external_price_api_client),
            consensus_config: read_optional_repr(&self.consensus),
            external_proof_integration_api_config: read_optional_repr(
                &self.external_proof_integration_api,
            ),
            experimental_vm_config: read_optional_repr(&self.experimental_vm),
            prover_job_monitor_config: read_optional_repr(&self.prover_job_monitor),
            timestamp_asserter_config: read_optional_repr(&self.timestamp_asserter),
            gateway_migrator_config: read_optional_repr(&self.gateway_migrator).unwrap_or_default(),
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
            witness_generator: this.witness_generator_config.as_ref().map(ProtoRepr::build),
            prover_gateway: this.prover_gateway.as_ref().map(ProtoRepr::build),
            prometheus: this.prometheus_config.as_ref().map(ProtoRepr::build),
            data_handler: this
                .proof_data_handler_config
                .as_ref()
                .map(ProtoRepr::build),
            tee_data_handler: this
                .tee_proof_data_handler_config
                .as_ref()
                .map(ProtoRepr::build),
            api: this.api_config.as_ref().map(ProtoRepr::build),
            db: this.db_config.as_ref().map(ProtoRepr::build),
            eth: this.eth.as_ref().map(ProtoRepr::build),
            snapshot_creator: this.snapshot_creator.as_ref().map(ProtoRepr::build),
            observability: this.observability.as_ref().map(ProtoRepr::build),
            da_client: this.da_client_config.as_ref().map(ProtoRepr::build),
            da_dispatcher: this.da_dispatcher_config.as_ref().map(ProtoRepr::build),
            protective_reads_writer: this
                .protective_reads_writer_config
                .as_ref()
                .map(ProtoRepr::build),
            basic_witness_input_producer: this
                .basic_witness_input_producer_config
                .as_ref()
                .map(ProtoRepr::build),
            commitment_generator: this.commitment_generator.as_ref().map(ProtoRepr::build),
            snapshot_recovery: this.snapshot_recovery.as_ref().map(ProtoRepr::build),
            pruning: this.pruning.as_ref().map(ProtoRepr::build),
            core_object_store: this.core_object_store.as_ref().map(ProtoRepr::build),
            base_token_adjuster: this.base_token_adjuster.as_ref().map(ProtoRepr::build),
            external_price_api_client: this
                .external_price_api_client_config
                .as_ref()
                .map(ProtoRepr::build),
            consensus: this.consensus_config.as_ref().map(ProtoRepr::build),
            external_proof_integration_api: this
                .external_proof_integration_api_config
                .as_ref()
                .map(ProtoRepr::build),
            experimental_vm: this.experimental_vm_config.as_ref().map(ProtoRepr::build),
            prover_job_monitor: this
                .prover_job_monitor_config
                .as_ref()
                .map(ProtoRepr::build),
            timestamp_asserter: this
                .timestamp_asserter_config
                .as_ref()
                .map(ProtoRepr::build),
            gateway_migrator: Some(ProtoRepr::build(&this.gateway_migrator_config)),
        }
    }
}
