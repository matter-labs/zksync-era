use anyhow::Context as _;
use zksync_basic_types::url::SensitiveUrl;
use zksync_config::configs;
use zksync_protobuf::{
    repr::{read_required_repr, ProtoRepr},
    required,
};

use crate::proto::database as proto;

impl proto::MerkleTreeMode {
    fn new(x: &configs::database::MerkleTreeMode) -> Self {
        use configs::database::MerkleTreeMode as From;
        match x {
            From::Full => Self::Full,
            From::Lightweight => Self::Lightweight,
        }
    }

    fn parse(&self) -> configs::database::MerkleTreeMode {
        use configs::database::MerkleTreeMode as To;
        match self {
            Self::Full => To::Full,
            Self::Lightweight => To::Lightweight,
        }
    }
}

impl ProtoRepr for proto::MerkleTree {
    type Type = configs::database::MerkleTreeConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            path: required(&self.path).context("path")?.clone(),
            mode: required(&self.mode)
                .and_then(|x| Ok(proto::MerkleTreeMode::try_from(*x)?))
                .context("mode")?
                .parse(),
            multi_get_chunk_size: required(&self.multi_get_chunk_size)
                .and_then(|x| Ok((*x).try_into()?))
                .context("multi_get_chunk_size")?,
            block_cache_size_mb: required(&self.block_cache_size_mb)
                .and_then(|x| Ok((*x).try_into()?))
                .context("block_cache_size_mb")?,
            memtable_capacity_mb: required(&self.memtable_capacity_mb)
                .and_then(|x| Ok((*x).try_into()?))
                .context("memtable_capacity_mb")?,
            stalled_writes_timeout_sec: *required(&self.stalled_writes_timeout_sec)
                .context("stalled_writes_timeout_sec")?,
            max_l1_batches_per_iter: required(&self.max_l1_batches_per_iter)
                .and_then(|x| Ok((*x).try_into()?))
                .context("max_l1_batches_per_iter")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            path: Some(this.path.clone()),
            mode: Some(proto::MerkleTreeMode::new(&this.mode).into()),
            multi_get_chunk_size: Some(this.multi_get_chunk_size.try_into().unwrap()),
            block_cache_size_mb: Some(this.block_cache_size_mb.try_into().unwrap()),
            memtable_capacity_mb: Some(this.memtable_capacity_mb.try_into().unwrap()),
            stalled_writes_timeout_sec: Some(this.stalled_writes_timeout_sec),
            max_l1_batches_per_iter: Some(this.max_l1_batches_per_iter.try_into().unwrap()),
        }
    }
}

impl ProtoRepr for proto::Db {
    type Type = configs::database::DBConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            state_keeper_db_path: required(&self.state_keeper_db_path)
                .context("state_keeper_db_path")?
                .clone(),
            merkle_tree: read_required_repr(&self.merkle_tree).context("merkle_tree")?,
            experimental: self
                .experimental
                .as_ref()
                .map(ProtoRepr::read)
                .transpose()
                .context("experimental")?
                .unwrap_or_default(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            state_keeper_db_path: Some(this.state_keeper_db_path.clone()),
            merkle_tree: Some(ProtoRepr::build(&this.merkle_tree)),
            experimental: Some(ProtoRepr::build(&this.experimental)),
        }
    }
}

impl ProtoRepr for proto::Postgres {
    type Type = configs::database::PostgresConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        let (test_server_url, test_prover_url) = self
            .test
            .as_ref()
            .map(|x| (x.server_url.clone(), x.prover_url.clone()))
            .unwrap_or_default();

        let master_url = self
            .server_url
            .as_deref()
            .map(str::parse::<SensitiveUrl>)
            .transpose()
            .context("master_url")?;
        let mut replica_url = self
            .server_replica_url
            .as_deref()
            .map(str::parse::<SensitiveUrl>)
            .transpose()
            .context("replica_url")?;
        if replica_url.is_none() {
            replica_url = master_url.clone();
        }
        let prover_url = self
            .prover_url
            .as_deref()
            .map(str::parse::<SensitiveUrl>)
            .transpose()
            .context("prover_url")?;

        Ok(Self::Type {
            master_url,
            replica_url,
            prover_url,
            max_connections: self.max_connections,
            max_connections_master: self.max_connections_master,
            acquire_timeout_sec: self.acquire_timeout_sec,
            statement_timeout_sec: self.statement_timeout_sec,
            long_connection_threshold_ms: self.long_connection_threshold_ms,
            slow_query_threshold_ms: self.slow_query_threshold_ms,
            test_server_url,
            test_prover_url,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            server_url: this
                .master_url
                .as_ref()
                .map(|url| url.expose_str().to_owned()),
            server_replica_url: this
                .replica_url
                .as_ref()
                .map(|url| url.expose_str().to_owned()),
            prover_url: this
                .prover_url
                .as_ref()
                .map(|url| url.expose_str().to_owned()),
            max_connections: this.max_connections,
            max_connections_master: this.max_connections_master,
            acquire_timeout_sec: this.acquire_timeout_sec,
            statement_timeout_sec: this.statement_timeout_sec,
            long_connection_threshold_ms: this.long_connection_threshold_ms,
            slow_query_threshold_ms: this.slow_query_threshold_ms,
            test: Some(proto::TestDatabase {
                server_url: this.test_server_url.clone(),
                prover_url: this.test_prover_url.clone(),
            }),
        }
    }
}
