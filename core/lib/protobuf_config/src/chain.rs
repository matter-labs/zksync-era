use anyhow::Context as _;
use zksync_basic_types::network::Network;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::{parse_h160, parse_h256, proto::chain as proto};

impl proto::Network {
    fn new(n: &Network) -> Self {
        match n {
            Network::Mainnet => Self::Mainnet,
            Network::Rinkeby => Self::Rinkeby,
            Network::Ropsten => Self::Ropsten,
            Network::Goerli => Self::Goerli,
            Network::Sepolia => Self::Sepolia,
            Network::Localhost => Self::Localhost,
            Network::Unknown => Self::Unknown,
            Network::Test => Self::Test,
        }
    }

    fn parse(&self) -> Network {
        match self {
            Self::Mainnet => Network::Mainnet,
            Self::Rinkeby => Network::Rinkeby,
            Self::Ropsten => Network::Ropsten,
            Self::Goerli => Network::Goerli,
            Self::Sepolia => Network::Sepolia,
            Self::Localhost => Network::Localhost,
            Self::Unknown => Network::Unknown,
            Self::Test => Network::Test,
        }
    }
}

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

impl ProtoRepr for proto::EthNetwork {
    type Type = configs::chain::NetworkConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            network: required(&self.network)
                .and_then(|x| Ok(proto::Network::try_from(*x)?))
                .context("network")?
                .parse(),
            zksync_network: required(&self.zksync_network)
                .context("zksync_network")?
                .clone(),
            zksync_network_id: required(&self.zksync_network_id)
                .and_then(|x| (*x).try_into().map_err(anyhow::Error::msg))
                .context("zksync_network_id")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            network: Some(proto::Network::new(&this.network).into()),
            zksync_network: Some(this.zksync_network.clone()),
            zksync_network_id: Some(this.zksync_network_id.as_u64()),
        }
    }
}

impl ProtoRepr for proto::StateKeeper {
    type Type = configs::chain::StateKeeperConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            transaction_slots: required(&self.transaction_slots)
                .and_then(|x| Ok((*x).try_into()?))
                .context("transaction_slots")?,
            block_commit_deadline_ms: *required(&self.block_commit_deadline_ms)
                .context("block_commit_deadline_ms")?,
            miniblock_commit_deadline_ms: *required(&self.miniblock_commit_deadline_ms)
                .context("miniblock_commit_deadline_ms")?,
            miniblock_seal_queue_capacity: required(&self.miniblock_seal_queue_capacity)
                .and_then(|x| Ok((*x).try_into()?))
                .context("miniblock_seal_queue_capacity")?,
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
            fee_account_addr: required(&self.fee_account_addr)
                .and_then(|a| parse_h160(a))
                .context("fee_account_addr")?,
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
            virtual_blocks_interval: *required(&self.virtual_blocks_interval)
                .context("virtual_blocks_interval")?,
            virtual_blocks_per_miniblock: *required(&self.virtual_blocks_per_miniblock)
                .context("virtual_blocks_per_miniblock")?,
            upload_witness_inputs_to_gcs: *required(&self.upload_witness_inputs_to_gcs)
                .context("upload_witness_inputs_to_gcs")?,
            enum_index_migration_chunk_size: self
                .enum_index_migration_chunk_size
                .map(|x| x.try_into())
                .transpose()
                .context("enum_index_migration_chunk_size")?,
            bootloader_hash: self
                .bootloader_hash
                .as_ref()
                .map(|a| parse_h256(a))
                .transpose()
                .context("bootloader_hash")?,
            default_aa_hash: self
                .default_aa_hash
                .as_ref()
                .map(|a| parse_h256(a))
                .transpose()
                .context("default_aa_hash")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            transaction_slots: Some(this.transaction_slots.try_into().unwrap()),
            block_commit_deadline_ms: Some(this.block_commit_deadline_ms),
            miniblock_commit_deadline_ms: Some(this.miniblock_commit_deadline_ms),
            miniblock_seal_queue_capacity: Some(
                this.miniblock_seal_queue_capacity.try_into().unwrap(),
            ),
            max_single_tx_gas: Some(this.max_single_tx_gas),
            max_allowed_l2_tx_gas_limit: Some(this.max_allowed_l2_tx_gas_limit),
            reject_tx_at_geometry_percentage: Some(this.reject_tx_at_geometry_percentage),
            reject_tx_at_eth_params_percentage: Some(this.reject_tx_at_eth_params_percentage),
            reject_tx_at_gas_percentage: Some(this.reject_tx_at_gas_percentage),
            close_block_at_geometry_percentage: Some(this.close_block_at_geometry_percentage),
            close_block_at_eth_params_percentage: Some(this.close_block_at_eth_params_percentage),
            close_block_at_gas_percentage: Some(this.close_block_at_gas_percentage),
            fee_account_addr: Some(this.fee_account_addr.as_bytes().into()),
            minimal_l2_gas_price: Some(this.minimal_l2_gas_price),
            compute_overhead_part: Some(this.compute_overhead_part),
            pubdata_overhead_part: Some(this.pubdata_overhead_part),
            batch_overhead_l1_gas: Some(this.batch_overhead_l1_gas),
            max_gas_per_batch: Some(this.max_gas_per_batch),
            max_pubdata_per_batch: Some(this.max_pubdata_per_batch),
            fee_model_version: Some(proto::FeeModelVersion::new(&this.fee_model_version).into()),
            validation_computational_gas_limit: Some(this.validation_computational_gas_limit),
            save_call_traces: Some(this.save_call_traces),
            virtual_blocks_interval: Some(this.virtual_blocks_interval),
            virtual_blocks_per_miniblock: Some(this.virtual_blocks_per_miniblock),
            upload_witness_inputs_to_gcs: Some(this.upload_witness_inputs_to_gcs),
            enum_index_migration_chunk_size: this
                .enum_index_migration_chunk_size
                .as_ref()
                .map(|x| (*x).try_into().unwrap()),
            bootloader_hash: this.bootloader_hash.map(|a| a.as_bytes().into()),
            default_aa_hash: this.default_aa_hash.map(|a| a.as_bytes().into()),
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
        }
    }
}

impl ProtoRepr for proto::CircuitBreaker {
    type Type = configs::chain::CircuitBreakerConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            sync_interval_ms: *required(&self.sync_interval_ms).context("sync_interval_ms")?,
            http_req_max_retry_number: required(&self.http_req_max_retry_number)
                .and_then(|x| Ok((*x).try_into()?))
                .context("http_req_max_retry_number")?,
            http_req_retry_interval_sec: required(&self.http_req_retry_interval_sec)
                .and_then(|x| Ok((*x).try_into()?))
                .context("http_req_retry_interval_sec")?,
            replication_lag_limit_sec: self.replication_lag_limit_sec,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            sync_interval_ms: Some(this.sync_interval_ms),
            http_req_max_retry_number: Some(this.http_req_max_retry_number.try_into().unwrap()),
            http_req_retry_interval_sec: Some(this.http_req_retry_interval_sec.into()),
            replication_lag_limit_sec: this.replication_lag_limit_sec,
        }
    }
}
