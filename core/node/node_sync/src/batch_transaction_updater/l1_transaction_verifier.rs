use anyhow::Context as _;
use zksync_dal::{blocks_dal::L1BatchWithOptionalMetadata, ConnectionPool, Core, CoreDal};
use zksync_types::{
    commitment::L1BatchMetadata, ethabi, web3::TransactionReceipt, Address, L1BatchNumber,
    ProtocolVersionId, H256, U64,
};

const FIRST_VALIDATED_PROTOCOL_VERSION_ID: ProtocolVersionId = if cfg!(test) {
    ProtocolVersionId::latest() // necessary for tests
} else {
    ProtocolVersionId::Version29
};

/// Verifies the L1 transaction against the database and the SL.
#[derive(Debug)]
pub struct L1TransactionVerifier {
    diamond_proxy_addr: Address,
    /// ABI of the ZKsync contract
    contract: ethabi::Contract,
    pool: ConnectionPool<Core>,
}

#[derive(Debug, thiserror::Error)]
pub enum TransactionValidationError {
    #[error("Batch {batch_number} is not found in the database")]
    BatchNotFound { batch_number: L1BatchNumber },
    #[error("Batch transaction {tx_hash} failed on SL")]
    TransactionFailed { tx_hash: H256 },
    #[error("Database error")]
    DatabaseError(#[from] zksync_dal::DalError),
    #[error("Batch transaction invalid: {reason}")]
    BatchTransactionInvalid { reason: String },
    #[error("Other validation error")]
    OtherValidationError(#[from] anyhow::Error),
}

impl TransactionValidationError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, TransactionValidationError::BatchNotFound { .. })
    }
}

pub fn get_param(log_params: &[ethabi::LogParam], name: &str) -> Option<ethabi::Token> {
    log_params
        .iter()
        .find(|param| param.name == name)
        .map(|param| param.value.clone())
}

impl L1TransactionVerifier {
    pub fn new(diamond_proxy_addr: Address, pool: ConnectionPool<Core>) -> Self {
        Self {
            diamond_proxy_addr,
            contract: zksync_contracts::hyperchain_contract(),
            pool,
        }
    }

    async fn get_db_batch_metadata(
        &self,
        batch_number: L1BatchNumber,
    ) -> Result<L1BatchMetadata, TransactionValidationError> {
        match self
            .pool
            .connection_tagged("sync_layer")
            .await?
            .blocks_dal()
            .get_optional_l1_batch_metadata(batch_number)
            .await?
        {
            Some(L1BatchWithOptionalMetadata {
                header: _,
                metadata: Ok(batch),
            }) => Ok(batch),
            _ => {
                tracing::debug!(
                    "Metadata for batch {} is not found in the database. Cannot verify transaction right now",
                    batch_number
                );
                Err(TransactionValidationError::BatchNotFound { batch_number })
            }
        }
    }

    async fn should_perform_logs_validation(
        &self,
        batch_number: L1BatchNumber,
    ) -> Result<bool, TransactionValidationError> {
        match self
            .pool
            .connection_tagged("sync_layer")
            .await?
            .blocks_dal()
            .get_batch_protocol_version_id(batch_number)
            .await?
        {
            Some(version) => Ok(version >= FIRST_VALIDATED_PROTOCOL_VERSION_ID),
            None => {
                tracing::debug!(
                    "Batch {} is not found in the database. Cannot verify transaction right now",
                    batch_number
                );
                Err(TransactionValidationError::BatchNotFound { batch_number })
            }
        }
    }

    pub async fn validate_commit_tx(
        &self,
        receipt: &TransactionReceipt,
        batch_number: L1BatchNumber,
    ) -> Result<(), TransactionValidationError> {
        if !self.should_perform_logs_validation(batch_number).await? {
            return Ok(());
        }

        let db_batch = self.get_db_batch_metadata(batch_number).await?;

        if receipt.status != Some(U64::one()) {
            return Err(TransactionValidationError::TransactionFailed {
                tx_hash: receipt.transaction_hash,
            });
        }

        let event = self
            .contract
            .event("BlockCommit")
            .context("`BlockCommit` event not found for ZKsync L1 contract")?;

        let committed_batch_info: Option<(H256, H256)> =
            receipt.logs.iter().find_map(|log| {
                if log.address != self.diamond_proxy_addr {
                    tracing::trace!(
                        "Log address {} does not match diamond proxy address {}, skipping",
                        log.address,
                        self.diamond_proxy_addr
                    );
                    return None;
                }
                let parsed_log = event.parse_log_whole(log.clone().into()).ok()?; // Skip logs that are of different event type

                let batch_number_from_log = get_param(&parsed_log.params, "batchNumber")
                        .and_then(ethabi::Token::into_uint)
                        .and_then(|x| u32::try_from(x).ok())
                        .map(L1BatchNumber)
                        .expect("Missing expected `batchNumber` parameter in `BlockCommit` event log");

                if batch_number_from_log != batch_number {
                    tracing::warn!(
                        "Commit transaction {0:?} has `BlockCommit` event log with batchNumber={batch_number_from_log}, \
                        but we are checking for batchNumber={batch_number}", receipt.transaction_hash
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
            });

        if let Some((batch_hash, commitment)) = committed_batch_info {
            if db_batch.commitment != commitment {
                return Err(TransactionValidationError::BatchTransactionInvalid {
                    reason: format!(
                        "Commit transaction {} for batch {} has different commitment: expected {:?}, got {:?}",
                        receipt.transaction_hash,
                        batch_number,
                        db_batch.commitment,
                        commitment
                    )
                });
            }
            if db_batch.root_hash != batch_hash {
                return Err(TransactionValidationError::BatchTransactionInvalid {
                    reason: format!(
                        "Commit transaction {} for batch {} has different root hash: expected {:?}, got {:?}",
                        receipt.transaction_hash,
                        batch_number,
                        db_batch.root_hash,
                        batch_hash
                    )
                });
            }
            // OK verified successfully the commit transaction.
            tracing::debug!(
                "Commit transaction {} for batch {} verified successfully",
                receipt.transaction_hash,
                batch_number
            );
            Ok(())
        } else {
            Err(TransactionValidationError::BatchTransactionInvalid {
                reason: format!(
                    "Commit transaction {} for batch {} does not have `BlockCommit` event log",
                    receipt.transaction_hash, batch_number
                ),
            })
        }
    }

    pub async fn validate_prove_tx(
        &self,
        receipt: &TransactionReceipt,
        batch_number: L1BatchNumber,
    ) -> Result<(), TransactionValidationError> {
        if !(self.should_perform_logs_validation(batch_number).await?) {
            return Ok(());
        }

        if receipt.status != Some(U64::one()) {
            return Err(TransactionValidationError::TransactionFailed {
                tx_hash: receipt.transaction_hash,
            });
        }

        let event = self
            .contract
            .event("BlocksVerification")
            .context("`BlocksVerification` event not found for ZKsync L1 contract")?;

        let proved_from_to: Option<(u32, u32)> =
            receipt.logs.iter().find_map(|log| {
                if log.address != self.diamond_proxy_addr {
                    tracing::debug!(
                        "Log address {} does not match diamond proxy address {}, skipping",
                        log.address,
                        self.diamond_proxy_addr
                    );
                    return None;
                }
                let parsed_log = event
                    .parse_log_whole(log.clone().into())
                    .ok()?; // Skip logs that are of different event type

                let batch_number_from = get_param(&parsed_log.params, "previousLastVerifiedBatch")
                        .and_then(ethabi::Token::into_uint)
                        .and_then(|batch_number_from_log| {
                            u32::try_from(batch_number_from_log).ok()
                        })
                        .expect("Missing expected `previousLastVerifiedBatch` parameter in `BlocksVerification` event log");
                let batch_number_to = get_param(&parsed_log.params, "currentLastVerifiedBatch")
                        .and_then(ethabi::Token::into_uint)
                        .and_then(|batch_number_to_log| {
                            u32::try_from(batch_number_to_log).ok()
                        })
                        .expect("Missing expected `currentLastVerifiedBatch` parameter in `BlocksVerification` event log");
                Some((
                    batch_number_from,
                    batch_number_to,
                ))
            });

        if let Some((from, to)) = proved_from_to {
            if from >= batch_number.0 {
                return Err(TransactionValidationError::BatchTransactionInvalid {
                    reason: format!(
                        "Prove transaction {} for batch {} has invalid `from` value: expected < {}, got {}",
                        receipt.transaction_hash,
                        batch_number,
                        batch_number.0,
                        from
                    )
                });
            }
            if to < batch_number.0 {
                return Err(TransactionValidationError::BatchTransactionInvalid {
                    reason: format!(
                        "Prove transaction {} for batch {} has invalid `to` value: expected >= {}, got {}",
                        receipt.transaction_hash,
                        batch_number,
                        batch_number.0,
                        to
                    )
                });
            }
            // OK verified successfully the prove transaction.
            tracing::debug!(
                "Prove transaction {} for batch {} verified successfully",
                receipt.transaction_hash,
                batch_number
            );
            Ok(())
        } else {
            Err(TransactionValidationError::BatchTransactionInvalid {
                reason: format!(
                    "Prove transaction {} for batch {} does not have `BlocksVerification` event log",
                    receipt.transaction_hash,
                    batch_number
                )
            })
        }
    }

    /// Validates the execute transaction against the database.
    pub async fn validate_execute_tx(
        &self,
        receipt: &TransactionReceipt,
        batch_number: L1BatchNumber,
    ) -> Result<(), TransactionValidationError> {
        if !(self.should_perform_logs_validation(batch_number).await?) {
            return Ok(());
        }

        let db_batch = self.get_db_batch_metadata(batch_number).await?;

        if receipt.status != Some(U64::one()) {
            return Err(TransactionValidationError::TransactionFailed {
                tx_hash: receipt.transaction_hash,
            });
        }

        let event = self
            .contract
            .event("BlockExecution")
            .context("`BlockExecution` event not found for ZKsync L1 contract")?;

        let executed_batch_info: Option<(H256, H256)> =
            receipt.logs.iter().find_map(|log| {
                if log.address != self.diamond_proxy_addr {
                    tracing::debug!(
                        "Log address {} does not match diamond proxy address {}, skipping",
                        log.address,
                        self.diamond_proxy_addr
                    );
                    return None;
                }
                let parsed_log = event
                    .parse_log_whole(log.clone().into())
                    .ok()?; // Skip logs that are of different event type

                let batch_number_from_log = get_param(&parsed_log.params, "batchNumber")
                        .and_then(ethabi::Token::into_uint)
                        .and_then(|batch_number_from_log| {
                            u32::try_from(batch_number_from_log)
                                    .ok()
                                    .map(L1BatchNumber)
                        })
                        .expect("Missing expected `batchNumber` parameter in `BlockExecution` event log");

                if batch_number_from_log != batch_number {
                    tracing::debug!(
                        "Skipping event log batchNumber={batch_number_from_log} for execute transaction {}. \
                        We are checking for batchNumber={batch_number}", receipt.transaction_hash,
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
            });

        if let Some((batch_hash, commitment)) = executed_batch_info {
            if db_batch.commitment != commitment {
                return Err(TransactionValidationError::BatchTransactionInvalid {
                    reason: format!(
                        "Execute transaction {} for batch {} has different commitment: expected {:?}, got {:?}",
                        receipt.transaction_hash,
                        batch_number,
                        db_batch.commitment,
                        commitment
                    )
                });
            }
            if db_batch.root_hash != batch_hash {
                return Err(TransactionValidationError::BatchTransactionInvalid {
                    reason: format!(
                        "Execute transaction {} for batch {} has different root hash: expected {:?}, got {:?}",
                        receipt.transaction_hash,batch_number,
                        db_batch.root_hash,
                        batch_hash
                    )
                });
            }
            // OK verified successfully the execute transaction.
            tracing::debug!(
                "Execute transaction {} for batch {} verified successfully",
                receipt.transaction_hash,
                batch_number
            );
            Ok(())
        } else {
            Err(TransactionValidationError::BatchTransactionInvalid {
                reason: format!(
                    "Execute transaction {} for batch {} does not have the corresponding `BlockExecution` event log",receipt.transaction_hash,
                    batch_number
                )
            })
        }
    }
}
