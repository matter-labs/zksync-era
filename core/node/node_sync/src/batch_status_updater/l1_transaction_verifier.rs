use anyhow::Context as _;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_client::EthInterface;
use zksync_types::{ethabi, Address, L1BatchNumber, SLChainId, H256, U64};

/// Verifies the L1 transaction against the database and the SL.
#[derive(Debug)]
pub struct L1TransactionVerifier {
    sl_client: Box<dyn EthInterface>,
    diamond_proxy_addr: Address,
    /// ABI of the ZKsync contract
    contract: ethabi::Contract,
    pool: ConnectionPool<Core>,
    pub sl_chain_id: SLChainId,
}

#[derive(Debug, thiserror::Error)]
pub enum TransactionValidationError {
    #[error("Batch {batch_number} is not found in the database")]
    BatchNotFound { batch_number: L1BatchNumber },
    #[error("Batch transaction {tx_hash} failed on SL")]
    TransactionFailed { tx_hash: H256 },
    #[error("Database error")]
    DatabaseError(#[from] zksync_dal::DalError),
    #[error("SL client error")]
    SlClientError(#[from] zksync_eth_client::EnrichedClientError),
    #[error("Batch tranasction invalid: {reason}")]
    BatchTransactionInvalid { reason: String },
    #[error("Other validaion error")]
    OtherValidationError(#[from] anyhow::Error),
}

impl TransactionValidationError {
    pub fn is_retriable(&self) -> bool {
        match self {
            TransactionValidationError::BatchNotFound { .. } => true,
            TransactionValidationError::TransactionFailed { .. } => false,
            TransactionValidationError::DatabaseError(_) => false,
            TransactionValidationError::SlClientError(_) => false,
            TransactionValidationError::BatchTransactionInvalid { .. } => false,
            TransactionValidationError::OtherValidationError(_) => false,
        }
    }
}

pub fn get_param(log_params: &[ethabi::LogParam], name: &str) -> Option<ethabi::Token> {
    log_params
        .iter()
        .find(|param| param.name == name)
        .map(|param| param.value.clone())
}

impl L1TransactionVerifier {
    pub fn new(
        sl_client: Box<dyn EthInterface>,
        diamond_proxy_addr: Address,
        pool: ConnectionPool<Core>,
        sl_chain_id: SLChainId,
    ) -> Self {
        Self {
            sl_client,
            diamond_proxy_addr,
            contract: zksync_contracts::hyperchain_contract(),
            pool,
            sl_chain_id,
        }
    }

    pub async fn validate_commit_tx_against_db(
        &self,
        commit_tx_hash: H256,
        batch_number: L1BatchNumber,
    ) -> Result<(), TransactionValidationError> {
        let db_batch = match self
            .pool
            .connection_tagged("sync_layer")
            .await?
            .blocks_dal()
            .get_l1_batch_metadata(batch_number)
            .await?
        {
            Some(batch) => batch,
            None => {
                tracing::debug!("Batch {} is not found in the database. Cannot verify commit transaction right now", batch_number);
                return Err(TransactionValidationError::BatchNotFound { batch_number });
            }
        };

        let receipt = self
            .sl_client
            .tx_receipt(commit_tx_hash)
            .await?
            .context("Failed to fetch commit transaction receipt from SL")?;
        if receipt.status != Some(U64::one()) {
            return Err(TransactionValidationError::TransactionFailed {
                tx_hash: commit_tx_hash,
            });
        }

        let event = self
            .contract
            .event("BlockCommit")
            .context("`BlockCommit` event not found for ZKsync L1 contract")?;

        let commited_batch_info: Option<(H256, H256)> =
            receipt.logs.into_iter().filter_map(|log| {
                if log.address != self.diamond_proxy_addr {
                    tracing::debug!(
                        "Log address {} does not match diamond proxy address {}, skipping",
                        log.address,
                        self.diamond_proxy_addr
                    );
                    return None;
                }
                let parsed_log = event
                                        .parse_log_whole(log.into())

                    .ok()?; // Skip logs that are of different event type

                let block_number_from_log = get_param(&parsed_log.params, "batchNumber")
                        .and_then(ethabi::Token::into_uint)
                        .and_then(|x| u32::try_from(x).ok())
                        .map(L1BatchNumber)
                        .expect("Missing expected `batchNumber` parameter in `BlockCommit` event log");

                if block_number_from_log != batch_number {
                    tracing::warn!(
                        "Commit transaction {commit_tx_hash:?} has `BlockCommit` event log with batchNumber={block_number_from_log}, \
                        but we are checking for batchNumber={batch_number}"
                    );
                    return None;
                }

                let batch_hash = get_param(&parsed_log.params, "batchHash")
                        .and_then(ethabi::Token::into_fixed_bytes).map(|bytes| H256::from_slice(&bytes))
                        .expect("Missing expected `batchHash` parameter in `BlockCommit` event log");

                let commitment = get_param(&parsed_log.params, "commitment")
                        .and_then(ethabi::Token::into_fixed_bytes).map(|bytes| H256::from_slice(&bytes))
                        .expect("Missing expected `commitment` parameter in `BlockCommit` event log");

                Some((batch_hash, commitment))
            }).next();

        if let Some((batch_hash, commitment)) = commited_batch_info {
            if db_batch.metadata.commitment != commitment {
                return Err(TransactionValidationError::BatchTransactionInvalid { reason: format!("Commit transaction {commit_tx_hash} for batch {} has different commitment: expected {:?}, got {:?}",
                    batch_number,
                    db_batch.metadata.commitment,
                    commitment) });
            }
            if db_batch.metadata.root_hash != batch_hash {
                return Err(TransactionValidationError::BatchTransactionInvalid { reason: format!("Commit transaction {commit_tx_hash} for batch {} has different root hash: expected {:?}, got {:?}",
                    batch_number,
                    db_batch.metadata.root_hash,
                    batch_hash)});
            }
            // OK verified successfully the commit transaction.
            tracing::debug!(
                "Commit transaction {commit_tx_hash} for batch {} verified successfully",
                batch_number
            );
            Ok(())
        } else {
            Err(TransactionValidationError::BatchTransactionInvalid { reason: format!("Commit transaction {commit_tx_hash} for batch {} does not have `BlockCommit` event log", batch_number) })
        }
    }

    pub async fn validate_prove_tx(
        &self,
        prove_tx_hash: H256,
        batch_number: L1BatchNumber,
    ) -> Result<(), TransactionValidationError> {
        let receipt = self
            .sl_client
            .tx_receipt(prove_tx_hash)
            .await?
            .context("Failed to fetch prove transaction receipt from SL")?;
        if receipt.status != Some(U64::one()) {
            return Err(TransactionValidationError::TransactionFailed {
                tx_hash: prove_tx_hash,
            });
        }

        let event = self
            .contract
            .event("BlocksVerification")
            .context("`BlocksVerification` event not found for ZKsync L1 contract")?;

        let proved_from_to: Option<(u32, u32)> =
            receipt.logs.into_iter().filter_map(|log| {
                if log.address != self.diamond_proxy_addr {
                    tracing::debug!(
                        "Log address {} does not match diamond proxy address {}, skipping",
                        log.address,
                        self.diamond_proxy_addr
                    );
                    return None;
                }
                let parsed_log = event
                    .parse_log_whole(log.into())
                    .ok()?; // Skip logs that are of different event type

                let block_number_from = get_param(&parsed_log.params, "previousLastVerifiedBatch")
                        .and_then(ethabi::Token::into_uint)
                        .and_then(|batch_number_from_log| {
                            u32::try_from(batch_number_from_log).ok()
                        })
                        .expect("Missing expected `previousLastVerifiedBatch` parameter in `BlocksVerification` event log");
                let block_number_to = get_param(&parsed_log.params, "currentLastVerifiedBatch")
                        .and_then(ethabi::Token::into_uint)
                        .and_then(|batch_number_to_log| {
                            u32::try_from(batch_number_to_log).ok()
                        })
                        .expect("Missing expected `currentLastVerifiedBatch` parameter in `BlocksVerification` event log");
                Some((
                    block_number_from,
                    block_number_to,
                ))
            }).next();
        if let Some((from, to)) = proved_from_to {
            if from >= batch_number.0 {
                return Err(TransactionValidationError::BatchTransactionInvalid {
                    reason: format!(
                        "Prove transaction {prove_tx_hash} for batch {} has invalid `from` value: expected < {}, got {}",
                        batch_number,
                        batch_number.0,
                        from
                    )
                });
            }
            if to < batch_number.0 {
                return Err(TransactionValidationError::BatchTransactionInvalid {
                    reason: format!(
                        "Prove transaction {prove_tx_hash} for batch {} has invalid `to` value: expected >= {}, got {}",
                        batch_number,
                        batch_number.0,
                        to
                    )
                });
            }
            // OK verified successfully the prove transaction.
            tracing::debug!(
                "Prove transaction {prove_tx_hash} for batch {} verified successfully",
                batch_number
            );
            Ok(())
        } else {
            Err(TransactionValidationError::BatchTransactionInvalid {
                reason: format!(
                    "Prove transaction {prove_tx_hash} for batch {} does not have `BlocksVerification` event log",
                    batch_number
                )
            })
        }
    }

    /// Validates the execute transaction against the database.
    pub async fn validate_execute_tx(
        &self,
        execute_tx_hash: H256,
        batch_number: L1BatchNumber,
    ) -> Result<(), TransactionValidationError> {
        let db_batch = match self
            .pool
            .connection_tagged("sync_layer")
            .await?
            .blocks_dal()
            .get_l1_batch_metadata(batch_number)
            .await?
        {
            Some(batch) => batch,
            None => {
                return Err(TransactionValidationError::BatchNotFound { batch_number });
            }
        };

        let receipt = self
            .sl_client
            .tx_receipt(execute_tx_hash)
            .await?
            .context("Failed to fetch execute transaction receipt from SL")?;
        if receipt.status != Some(U64::one()) {
            return Err(TransactionValidationError::TransactionFailed {
                tx_hash: execute_tx_hash,
            });
        }

        let event = self
            .contract
            .event("BlockExecution")
            .context("`BlockExecution` event not found for ZKsync L1 contract")?;

        let commited_batch_info: Option<(H256, H256)> =
            receipt.logs.into_iter().filter_map(|log| {
                if log.address != self.diamond_proxy_addr {
                    tracing::debug!(
                        "Log address {} does not match diamond proxy address {}, skipping",
                        log.address,
                        self.diamond_proxy_addr
                    );
                    return None;
                }
                let parsed_log = event
                                        .parse_log_whole(log.into())

                    .ok()?; // Skip logs that are of different event type

                let block_number_from_log = get_param(&parsed_log.params, "batchNumber")
                        .and_then(ethabi::Token::into_uint)
                        .and_then(|batch_number_from_log| {
                            u32::try_from(batch_number_from_log)
                                    .ok()
                                    .map(L1BatchNumber)
                        })
                        .expect("Missing expected `batchNumber` parameter in `BlockExecution` event log");

                if block_number_from_log != batch_number {
                    tracing::debug!(
                        "Skipping event log batchNumber={block_number_from_log} for commit transaction {execute_tx_hash:?}. \
                        We are checking for batchNumber={batch_number}"
                    );
                    return None;
                }

                let batch_hash = get_param(&parsed_log.params, "batchHash")
                        .and_then(ethabi::Token::into_fixed_bytes)
                        .map(|bytes| H256::from_slice(&bytes))
                        .expect("Missing expected `batchHash` parameter in `BlockExecution` event log");

                let commitment = get_param(&parsed_log.params, "commitment")
                        .and_then(ethabi::Token::into_fixed_bytes)
                        .map(|bytes| H256::from_slice(&bytes))
                        .expect("Missing expected `commitment` parameter in `BlockExecution` event log");

                Some((batch_hash, commitment))
            }).next();

        if let Some((batch_hash, commitment)) = commited_batch_info {
            if db_batch.metadata.commitment != commitment {
                return Err(TransactionValidationError::BatchTransactionInvalid {
                    reason: format!(
                        "Execute transaction {execute_tx_hash} for batch {} has different commitment: expected {:?}, got {:?}",
                        batch_number,
                        db_batch.metadata.commitment,
                        commitment
                    )
                });
            }
            if db_batch.metadata.root_hash != batch_hash {
                return Err(TransactionValidationError::BatchTransactionInvalid {
                    reason: format!(
                        "Execute transaction {execute_tx_hash} for batch {} has different root hash: expected {:?}, got {:?}",
                        batch_number,
                        db_batch.metadata.root_hash,
                        batch_hash
                    )
                });
            }
            // OK verified successfully the execute transaction.
            tracing::debug!(
                "Execute transaction {execute_tx_hash} for batch {} verified successfully",
                batch_number
            );
            Ok(())
        } else {
            Err(TransactionValidationError::BatchTransactionInvalid {
                reason: format!(
                    "Execute transaction {execute_tx_hash} for batch {} does not have the corresponding `BlockExecution` event log",
                    batch_number
                )
            })
        }
    }
}
