use anyhow::Context as _;
use zksync_config::{configs, configs::chain::Finality};
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::{parse_h160, proto::chain as proto, read_optional_repr};

impl proto::FeeModelVersion {
    fn new(n: &configs::chain::FeeModelVersion) -> Self {
        use configs::chain::FeeModelVersion as From;
        match n {
            From::V1 => Self::V1,
            From::V2 => Self::V2,
        }
    }

    fn parse(&self) -> configs::chain::FeeModelVersion {
        use configs::chain::FeeModelVersion as To;
        match self {
            Self::V1 => To::V1,
            Self::V2 => To::V2,
        }
    }
}

impl ProtoRepr for proto::StateKeeper {
    type Type = configs::chain::StateKeeperConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        #[allow(deprecated)]
        Ok(Self::Type {
            transaction_slots: required(&self.transaction_slots)
                .and_then(|x| Ok((*x).try_into()?))
                .context("transaction_slots")?,
            block_commit_deadline_ms: *required(&self.block_commit_deadline_ms)
                .context("block_commit_deadline_ms")?,
            l2_block_commit_deadline_ms: *required(&self.miniblock_commit_deadline_ms)
                .context("miniblock_commit_deadline_ms")?,
            l2_block_seal_queue_capacity: required(&self.miniblock_seal_queue_capacity)
                .and_then(|x| Ok((*x).try_into()?))
                .context("miniblock_seal_queue_capacity")?,
            l2_block_max_payload_size: required(&self.miniblock_max_payload_size)
                .and_then(|x| Ok((*x).try_into()?))
                .context("miniblock_max_payload_size")?,
            max_single_tx_gas: *required(&self.max_single_tx_gas).context("max_single_tx_gas")?,
            max_allowed_l2_tx_gas_limit: *required(&self.max_allowed_l2_tx_gas_limit)
                .context("max_allowed_l2_tx_gas_limit")?,
            reject_tx_at_geometry_percentage: *required(&self.reject_tx_at_geometry_percentage)
                .context("reject_tx_at_geometry_percentage")?,
            reject_tx_at_eth_params_percentage: *required(&self.reject_tx_at_eth_params_percentage)
                .context("reject_tx_at_eth_params_percentage")?,
            reject_tx_at_gas_percentage: *required(&self.reject_tx_at_gas_percentage)
                .context("reject_tx_at_gas_percentage")?,
            close_block_at_geometry_percentage: *required(&self.close_block_at_geometry_percentage)
                .context("close_block_at_geometry_percentage")?,
            close_block_at_eth_params_percentage: *required(
                &self.close_block_at_eth_params_percentage,
            )
            .context("close_block_at_eth_params_percentage")?,
            close_block_at_gas_percentage: *required(&self.close_block_at_gas_percentage)
                .context("close_block_at_gas_percentage")?,
            minimal_l2_gas_price: *required(&self.minimal_l2_gas_price)
                .context("minimal_l2_gas_price")?,
            compute_overhead_part: *required(&self.compute_overhead_part)
                .context("compute_overhead_part")?,
            pubdata_overhead_part: *required(&self.pubdata_overhead_part)
                .context("pubdata_overhead_part")?,
            batch_overhead_l1_gas: *required(&self.batch_overhead_l1_gas)
                .context("batch_overhead_l1_gas")?,
            max_gas_per_batch: *required(&self.max_gas_per_batch).context("max_gas_per_batch")?,
            max_pubdata_per_batch: *required(&self.max_pubdata_per_batch)
                .context("max_pubdata_per_batch")?,
            fee_model_version: required(&self.fee_model_version)
                .and_then(|x| Ok(proto::FeeModelVersion::try_from(*x)?))
                .context("fee_model_version")?
                .parse(),
            validation_computational_gas_limit: *required(&self.validation_computational_gas_limit)
                .context("validation_computational_gas_limit")?,
            save_call_traces: *required(&self.save_call_traces).context("save_call_traces")?,
            max_circuits_per_batch: required(&self.max_circuits_per_batch)
                .and_then(|x| Ok((*x).try_into()?))
                .context("max_circuits_per_batch")?,
            protective_reads_persistence_enabled: self
                .protective_reads_persistence_enabled
                .unwrap_or_default(),

            // We need these values only for instantiating configs from environmental variables, so it's not
            // needed during the initialization from files
            bootloader_hash: None,
            default_aa_hash: None,
            evm_emulator_hash: None,
            fee_account_addr: None,
            l1_batch_commit_data_generator_mode: Default::default(),
            deployment_allowlist: read_optional_repr(&self.deployment_allowlist),
            // TODO add it to real config
            finality: Finality::RollingTxHash(10),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            transaction_slots: Some(this.transaction_slots.try_into().unwrap()),
            block_commit_deadline_ms: Some(this.block_commit_deadline_ms),
            miniblock_commit_deadline_ms: Some(this.l2_block_commit_deadline_ms),
            miniblock_seal_queue_capacity: Some(
                this.l2_block_seal_queue_capacity.try_into().unwrap(),
            ),
            miniblock_max_payload_size: Some(this.l2_block_max_payload_size.try_into().unwrap()),
            max_single_tx_gas: Some(this.max_single_tx_gas),
            max_allowed_l2_tx_gas_limit: Some(this.max_allowed_l2_tx_gas_limit),
            reject_tx_at_geometry_percentage: Some(this.reject_tx_at_geometry_percentage),
            reject_tx_at_eth_params_percentage: Some(this.reject_tx_at_eth_params_percentage),
            reject_tx_at_gas_percentage: Some(this.reject_tx_at_gas_percentage),
            close_block_at_geometry_percentage: Some(this.close_block_at_geometry_percentage),
            close_block_at_eth_params_percentage: Some(this.close_block_at_eth_params_percentage),
            close_block_at_gas_percentage: Some(this.close_block_at_gas_percentage),
            minimal_l2_gas_price: Some(this.minimal_l2_gas_price),
            compute_overhead_part: Some(this.compute_overhead_part),
            pubdata_overhead_part: Some(this.pubdata_overhead_part),
            batch_overhead_l1_gas: Some(this.batch_overhead_l1_gas),
            max_gas_per_batch: Some(this.max_gas_per_batch),
            max_pubdata_per_batch: Some(this.max_pubdata_per_batch),
            fee_model_version: Some(proto::FeeModelVersion::new(&this.fee_model_version).into()),
            validation_computational_gas_limit: Some(this.validation_computational_gas_limit),
            save_call_traces: Some(this.save_call_traces),
            max_circuits_per_batch: Some(this.max_circuits_per_batch.try_into().unwrap()),
            protective_reads_persistence_enabled: Some(this.protective_reads_persistence_enabled),
            deployment_allowlist: this
                .deployment_allowlist
                .as_ref()
                .map(proto::DeploymentAllowlist::build),
        }
    }
}

impl ProtoRepr for proto::OperationsManager {
    type Type = configs::chain::OperationsManagerConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            delay_interval: *required(&self.delay_interval).context("delay_interval")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            delay_interval: Some(this.delay_interval),
        }
    }
}

impl ProtoRepr for proto::Mempool {
    type Type = configs::chain::MempoolConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            sync_interval_ms: *required(&self.sync_interval_ms).context("sync_interval_ms")?,
            sync_batch_size: required(&self.sync_batch_size)
                .and_then(|x| Ok((*x).try_into()?))
                .context("sync_batch_size")?,
            capacity: *required(&self.capacity).context("capacity")?,
            stuck_tx_timeout: *required(&self.stuck_tx_timeout).context("stuck_tx_timeout")?,
            remove_stuck_txs: *required(&self.remove_stuck_txs).context("remove_stuck_txs")?,
            delay_interval: *required(&self.delay_interval).context("delay_interval")?,
            skip_unsafe_deposit_checks: self.skip_unsafe_deposit_checks.unwrap_or_default(),
            l1_to_l2_txs_paused: self.l1_to_l2_txs_paused.unwrap_or_default(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            sync_interval_ms: Some(this.sync_interval_ms),
            sync_batch_size: Some(this.sync_batch_size.try_into().unwrap()),
            capacity: Some(this.capacity),
            stuck_tx_timeout: Some(this.stuck_tx_timeout),
            remove_stuck_txs: Some(this.remove_stuck_txs),
            delay_interval: Some(this.delay_interval),
            skip_unsafe_deposit_checks: Some(this.skip_unsafe_deposit_checks),
            l1_to_l2_txs_paused: Some(this.l1_to_l2_txs_paused),
        }
    }
}

impl ProtoRepr for proto::DeploymentAllowlist {
    type Type = configs::chain::DeploymentAllowlist;

    fn read(&self) -> anyhow::Result<Self::Type> {
        let res = match required(&self.allow_list).context("DeploymentAllowlist")? {
            proto::deployment_allowlist::AllowList::Dynamic(list) => {
                Self::Type::Dynamic(configs::chain::DeploymentAllowlistDynamic::new(
                    list.http_file_url.clone(),
                    list.refresh_interval_secs,
                ))
            }
            proto::deployment_allowlist::AllowList::Static(list) => {
                let res: Result<Vec<_>, _> =
                    list.addresses.iter().map(|item| parse_h160(item)).collect();
                Self::Type::Static(res?)
            }
        };
        Ok(res)
    }

    fn build(this: &Self::Type) -> Self {
        let allow_list = match this {
            configs::chain::DeploymentAllowlist::Dynamic(list) => {
                proto::deployment_allowlist::AllowList::Dynamic(
                    proto::deployment_allowlist::Dynamic {
                        http_file_url: list.http_file_url().map(String::from),
                        refresh_interval_secs: Some(list.refresh_interval().as_secs()),
                    },
                )
            }
            configs::chain::DeploymentAllowlist::Static(list) => {
                proto::deployment_allowlist::AllowList::Static(
                    proto::deployment_allowlist::Static {
                        addresses: list.iter().map(|item| format!("{:?}", item)).collect(),
                    },
                )
            }
        };
        Self {
            allow_list: Some(allow_list),
        }
    }
}
