use anyhow::Context as _;
use zksync_consensus_engine::{BlockStoreState, Last};
use zksync_consensus_roles::validator;
use zksync_db_connection::{
    connection::Connection,
    error::{DalError, DalResult, SqlxContext},
    instrument::{InstrumentExt, Instrumented},
};
use zksync_types::{L1BatchNumber, L2BlockNumber};

pub use crate::consensus::{proto, BlockMetadata, GlobalConfig, Payload};
use crate::{consensus::BlockCertificate, Core, CoreDal};

#[cfg(test)]
mod tests;

/// Verifies that the transition from `old` to `new` is admissible.
pub fn verify_config_transition(old: &GlobalConfig, new: &GlobalConfig) -> anyhow::Result<()> {
    anyhow::ensure!(
        old.genesis.chain_id == new.genesis.chain_id,
        "changing chain_id is not allowed: old = {:?}, new = {:?}",
        old.genesis.chain_id,
        new.genesis.chain_id,
    );
    // Note that it may happen that the fork number didn't change,
    // in case the binary was updated to support more fields in genesis struct.
    // In such a case, the old binary was not able to connect to the consensus network,
    // because of the genesis hash mismatch.
    // TODO: Perhaps it would be better to deny unknown fields in the genesis instead.
    // It would require embedding the genesis either as a json string or protobuf bytes within
    // the global config, so that the global config can be parsed with
    // `deny_unknown_fields:false` while genesis would be parsed with
    // `deny_unknown_fields:true`.
    anyhow::ensure!(
        old.genesis.fork_number <= new.genesis.fork_number,
        "transition to a past fork is not allowed: old = {:?}, new = {:?}",
        old.genesis.fork_number,
        new.genesis.fork_number,
    );

    // This is a temporary hack until the `consensus_genesis()` RPC is disabled.
    if new
        == (&GlobalConfig {
            genesis: old.genesis.clone(),
            registry_address: None,
            seed_peers: [].into(),
        })
    {
        anyhow::bail!("new config is equal to truncated old config, which means that it was sourced from the wrong endpoint");
    }

    Ok(())
}

/// Storage access methods for `zksync_core::consensus` module.
#[derive(Debug)]
pub struct ConsensusDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

/// Error returned by `ConsensusDal::insert_certificate()`.
#[derive(thiserror::Error, Debug)]
pub enum InsertCertificateError {
    #[error("corresponding payload is missing")]
    MissingPayload,
    #[error("certificate doesn't match the payload, payload = {0:?}")]
    PayloadMismatch(Payload),
    #[error(transparent)]
    Dal(#[from] DalError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl ConsensusDal<'_, '_> {
    /// Fetch consensus global config.
    pub async fn global_config(&mut self) -> anyhow::Result<Option<GlobalConfig>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                global_config
            FROM
                consensus_replica_state
            WHERE
                fake_key
            "#
        )
        .instrument("global_config")
        .fetch_optional(self.storage)
        .await?
        else {
            return Ok(None);
        };
        let d = zksync_protobuf::serde::Deserialize {
            deny_unknown_fields: true,
        };
        if let Some(global_config) = row.global_config {
            return Ok(Some(d.proto_fmt(&global_config).context("global_config")?));
        }
        Ok(None)
    }

    /// Attempts to update the global config.
    /// Fails if the new genesis is invalid.
    /// Fails if the new genesis has different `chain_id`.
    /// Fails if the storage contains a newer genesis (higher fork number).
    /// Noop if the new global config is the same as the current one.
    /// Resets the stored consensus state otherwise and purges all certificates.
    pub async fn try_update_global_config(&mut self, want: &GlobalConfig) -> anyhow::Result<()> {
        let mut txn = self.storage.start_transaction().await?;
        let got = txn.consensus_dal().global_config().await?;
        if let Some(got) = &got {
            // Exit if the global config didn't change.
            if got == want {
                return Ok(());
            }
            verify_config_transition(got, want)?;

            // If genesis didn't change, just update the config.
            if got.genesis == want.genesis {
                let s = zksync_protobuf::serde::Serialize;
                let global_config = s.proto_fmt(want, serde_json::value::Serializer).unwrap();
                sqlx::query!(
                    r#"
                    UPDATE consensus_replica_state
                    SET
                        global_config = $1
                    "#,
                    global_config,
                )
                .instrument("try_update_global_config#UPDATE consensus_replica_state")
                .execute(&mut txn)
                .await?;
                txn.commit().await?;
                return Ok(());
            }
        }

        // Reset the consensus state.
        let s = zksync_protobuf::serde::Serialize;
        let global_config = s.proto_fmt(want, serde_json::value::Serializer).unwrap();
        let state = s
            .proto_fmt(&validator::ReplicaState::default(), serde_json::value::Serializer)
            .unwrap();
        sqlx::query!(
            r#"
            DELETE FROM l1_batches_consensus
            "#
        )
        .instrument("try_update_genesis#DELETE FROM l1_batches_consensus")
        .execute(&mut txn)
        .await?;
        sqlx::query!(
            r#"
            DELETE FROM miniblocks_consensus
            "#
        )
        .instrument("try_update_genesis#DELETE FROM miniblock_consensus")
        .execute(&mut txn)
        .await?;
        sqlx::query!(
            r#"
            DELETE FROM consensus_replica_state
            "#
        )
        .instrument("try_update_genesis#DELETE FROM consensus_replica_state")
        .execute(&mut txn)
        .await?;
        sqlx::query!(
            r#"
            INSERT INTO
            consensus_replica_state (fake_key, global_config, state)
            VALUES
            (TRUE, $1, $2)
            "#,
            global_config,
            state,
        )
        .instrument("try_update_global_config#INSERT INTO consensus_replica_state")
        .execute(&mut txn)
        .await?;
        txn.commit().await?;
        Ok(())
    }

    /// [Main node only] creates a new consensus fork starting at
    /// the last sealed L2 block. Resets the state of the consensus
    /// by calling `try_update_genesis()`.
    pub async fn fork(&mut self) -> anyhow::Result<()> {
        let mut txn = self
            .storage
            .start_transaction()
            .await
            .context("start_transaction")?;
        let Some(old) = txn
            .consensus_dal()
            .global_config()
            .await
            .context("global_config()")?
        else {
            return Ok(());
        };
        let new = GlobalConfig {
            genesis: validator::GenesisRaw {
                chain_id: old.genesis.chain_id,
                fork_number: old.genesis.fork_number.next(),
                first_block: txn
                    .consensus_dal()
                    .next_block()
                    .await
                    .context("next_block()")?,

                protocol_version: old.genesis.protocol_version,
                validators_schedule: old.genesis.validators_schedule.clone(),
            }
            .with_hash(),
            registry_address: old.registry_address,
            seed_peers: old.seed_peers,
        };
        txn.consensus_dal().try_update_global_config(&new).await?;
        txn.commit().await?;
        Ok(())
    }

    /// Fetches the current BFT replica state.
    pub async fn replica_state(&mut self) -> DalResult<validator::ReplicaState> {
        sqlx::query!(
            r#"
            SELECT
                state AS "state!"
            FROM
                consensus_replica_state
            WHERE
                fake_key
            "#
        )
        .try_map(|row| {
            zksync_protobuf::serde::Deserialize {
                deny_unknown_fields: true,
            }
            .proto_fmt(row.state)
            .decode_column("state")
        })
        .instrument("replica_state")
        .fetch_one(self.storage)
        .await
    }

    /// Sets the current BFT replica state.
    pub async fn set_replica_state(&mut self, state: &validator::ReplicaState) -> DalResult<()> {
        let state_json = zksync_protobuf::serde::Serialize
            .proto_fmt(state, serde_json::value::Serializer)
            .unwrap();
        sqlx::query!(
            r#"
            UPDATE consensus_replica_state
            SET
                state = $1
            WHERE
                fake_key
            "#,
            state_json
        )
        .instrument("set_replica_state")
        .report_latency()
        .with_arg("state.view", &state.view)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    /// First block that should be in storage.
    async fn first_block(&mut self) -> anyhow::Result<validator::BlockNumber> {
        let info = self
            .storage
            .pruning_dal()
            .get_pruning_info()
            .await
            .context("get_pruning_info()")?;
        Ok(match info.last_soft_pruned {
            // It is guaranteed that pruning info values are set for storage recovered from
            // snapshot, even if pruning was not enabled.
            Some(last_pruned) => validator::BlockNumber(last_pruned.l2_block.0.into()) + 1,
            // No snapshot and no pruning:
            None => validator::BlockNumber(0),
        })
    }

    /// Next block that should be inserted to storage.
    pub async fn next_block(&mut self) -> anyhow::Result<validator::BlockNumber> {
        if let Some(last) = self
            .storage
            .blocks_dal()
            .get_sealed_l2_block_number()
            .await
            .context("get_sealed_l2_block_number()")?
        {
            return Ok(validator::BlockNumber(last.0.into()) + 1);
        }
        let next = self
            .storage
            .consensus_dal()
            .first_block()
            .await
            .context("first_block()")?;
        Ok(next)
    }

    /// Fetches the block store state.
    /// The blocks that are available to consensus are either pre-genesis or
    /// have a consensus certificate.
    /// Currently, certificates are NOT generated synchronously with L2 blocks,
    /// so the `BlockStoreState.last` might be different than the last block in storage.
    pub async fn block_store_state(&mut self) -> anyhow::Result<BlockStoreState> {
        let first = self.first_block().await.context("first_block()")?;
        let cfg = self
            .global_config()
            .await
            .context("global_config()")?
            .context("global config is missing")?;

        // If there is a cert in storage, then the block range visible to consensus
        // is [first block, block of last cert].
        // Also tries to fetch the last cert from the old column
        // This is a temporary solution to support the transition to the new column.
        if let Some(row) = sqlx::query!(
            r#"
            SELECT
                certificate, versioned_certificate
            FROM
                miniblocks_consensus
            ORDER BY
                number DESC
            LIMIT
                1
            "#,
        )
        .instrument("block_certificate_range")
        .report_latency()
        .fetch_optional(self.storage)
        .await?
        {
            let d = zksync_protobuf::serde::Deserialize {
                deny_unknown_fields: true,
            };

            // First try to use versioned_certificate
            let cert: Option<BlockCertificate> = row
                .versioned_certificate
                .as_ref()
                .map(|cert| d.proto_fmt(cert))
                .transpose()?;

            if let Some(cert) = cert {
                return Ok(BlockStoreState {
                    first,
                    last: Some(cert.into()),
                });
            }

            // If versioned_certificate is None, try to use certificate
            // This is for backward compatibility
            let qc = row
                .certificate
                .as_ref()
                .map(|cert| d.proto_fmt(cert))
                .transpose()?;

            if let Some(qc) = qc {
                return Ok(BlockStoreState {
                    first,
                    last: Some(Last::FinalV1(qc)),
                });
            }
        }

        // Otherwise it is [first block, min(genesis.first_block-1,last block)].
        let next = self
            .next_block()
            .await
            .context("next_block()")?
            .min(cfg.genesis.first_block);

        Ok(BlockStoreState {
            first,
            // unwrap is ok, because `next > first >= 0`.
            last: if next > first {
                Some(Last::PreGenesis(next.prev().unwrap()))
            } else {
                None
            },
        })
    }

    /// Fetches the consensus certificate for the L2 block with the given `block_number`.
    pub async fn block_certificate(
        &mut self,
        block_number: validator::BlockNumber,
    ) -> anyhow::Result<Option<BlockCertificate>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                certificate, versioned_certificate
            FROM
                miniblocks_consensus
            WHERE
                number = $1
            "#,
            i64::try_from(block_number.0)?
        )
        .instrument("block_certificate")
        .report_latency()
        .fetch_optional(self.storage)
        .await?
        else {
            return Ok(None);
        };

        let d = zksync_protobuf::serde::Deserialize {
            deny_unknown_fields: true,
        };

        // First try to use versioned_certificate
        let cert = row
            .versioned_certificate
            .as_ref()
            .map(|cert| d.proto_fmt(cert))
            .transpose()?;

        if cert.is_some() {
            return Ok(cert);
        }

        // If versioned_certificate is None, try to use certificate
        // This is for backward compatibility
        let qc = row
            .certificate
            .as_ref()
            .map(|cert| d.proto_fmt(cert))
            .transpose()?;

        if qc.is_some() {
            return Ok(qc);
        }

        Ok(None)
    }

    /// Gets the number of the last L2 block that was certified. It might have gaps before it,
    /// depending on the order in which certificates have been collected.
    pub async fn last_block_certificate_number(
        &mut self,
    ) -> anyhow::Result<Option<validator::BlockNumber>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                number
            FROM
                miniblocks_consensus
            ORDER BY
                number DESC
            LIMIT
                1
            "#
        )
        .instrument("last_block_certificate_number")
        .report_latency()
        .fetch_optional(self.storage)
        .await?
        else {
            return Ok(None);
        };

        Ok(Some(validator::BlockNumber(
            row.number.try_into().context("overflow")?,
        )))
    }

    /// Inserts a certificate for the L2 block `cert.header().number`.
    /// Fails if certificate doesn't match the stored block.
    pub async fn insert_block_certificate(
        &mut self,
        cert: &BlockCertificate,
    ) -> Result<(), InsertCertificateError> {
        use InsertCertificateError as E;

        // Extract block number and payload hash based on certificate variant
        let block_number = cert.number();
        let payload_hash = cert.payload_hash();

        let want_payload = self
            .block_payload(block_number)
            .await?
            .ok_or(E::MissingPayload)?;

        if payload_hash != want_payload.encode().hash() {
            return Err(E::PayloadMismatch(want_payload));
        }

        sqlx::query!(
            r#"
            INSERT INTO
            miniblocks_consensus (number, versioned_certificate)
            VALUES
            ($1, $2)
            "#,
            i64::try_from(block_number.0).context("overflow")?,
            zksync_protobuf::serde::Serialize
                .proto_fmt(cert, serde_json::value::Serializer)
                .unwrap(),
        )
        .instrument("insert_block_certificate")
        .report_latency()
        .execute(self.storage)
        .await?;

        Ok(())
    }

    /// Fetches a range of L2 blocks from storage and converts them to `Payload`s.
    pub async fn block_payloads(
        &mut self,
        numbers: std::ops::Range<validator::BlockNumber>,
    ) -> DalResult<Vec<Payload>> {
        let numbers = (|| {
            anyhow::Ok(std::ops::Range {
                start: L2BlockNumber(numbers.start.0.try_into().context("start")?),
                end: L2BlockNumber(numbers.end.0.try_into().context("end")?),
            })
        })()
        .map_err(|err| {
            Instrumented::new("block_payloads")
                .with_arg("numbers", &numbers)
                .arg_error("numbers", err)
        })?;

        let blocks = self
            .storage
            .sync_dal()
            .sync_blocks_inner(numbers.clone())
            .await?;
        let mut transactions = self
            .storage
            .transactions_web3_dal()
            .get_raw_l2_blocks_transactions(numbers)
            .await?;
        Ok(blocks
            .into_iter()
            .map(|b| {
                let txs = transactions.remove(&b.number).unwrap_or_default();
                b.into_payload(txs)
            })
            .collect())
    }

    /// Fetches an L2 block from storage and converts it to `Payload`. `Payload` is an
    /// opaque format for the L2 block that consensus understands and generates a
    /// certificate for it.
    pub async fn block_payload(
        &mut self,
        number: validator::BlockNumber,
    ) -> DalResult<Option<Payload>> {
        Ok(self
            .block_payloads(number..number + 1)
            .await?
            .into_iter()
            .next())
    }

    /// Fetches L2 block metadata for the given block number.
    pub async fn block_metadata(
        &mut self,
        n: validator::BlockNumber,
    ) -> anyhow::Result<Option<BlockMetadata>> {
        let Some(b) = self.block_payload(n).await.context("block_payload()")? else {
            return Ok(None);
        };
        Ok(Some(BlockMetadata {
            payload_hash: b.encode().hash(),
        }))
    }

    /// Checks if the L1 batch and metadata is stored in the database.
    pub async fn is_batch_stored(&mut self, number: L1BatchNumber) -> anyhow::Result<bool> {
        Ok(self
            .storage
            .blocks_dal()
            .get_l1_batch_header(number)
            .await
            .context("get_l1_batch_header()")?
            .is_some())
    }
}
