use anyhow::Context as _;
use zksync_types::{
    block::L2BlockHeader, commitment::L1BatchMetadata, ethabi, web3::TransactionReceipt, Address,
    L1BatchNumber, L2BlockNumber, H256, U64,
};

/// Verifies the L1 transaction against the database and the SL.
#[derive(Debug)]
pub struct L1TransactionVerifier {
    diamond_proxy_addr: Address,
    /// ABI of the ZKsync contract
    contract: ethabi::Contract,
}

#[derive(Debug, thiserror::Error)]
pub enum TransactionValidationError {
    #[error("Batch transaction {tx_hash} invalid: {reason}")]
    BatchTransactionInvalid { tx_hash: H256, reason: String },
    #[error("Precommit transaction {tx_hash} invalid: {reason}")]
    PrecommitTransactionInvalid { tx_hash: H256, reason: String },
    #[error(transparent)]
    OtherValidationError(#[from] anyhow::Error),
}

pub fn get_param(log_params: &[ethabi::LogParam], name: &str) -> Option<ethabi::Token> {
    log_params
        .iter()
        .find(|param| param.name == name)
        .map(|param| param.value.clone())
}

impl L1TransactionVerifier {
    pub fn new(diamond_proxy_addr: Address) -> Self {
        Self {
            diamond_proxy_addr,
            contract: zksync_contracts::hyperchain_contract(),
        }
    }

    pub fn validate_commit_tx(
        &self,
        receipt: &TransactionReceipt,
        db_batch: L1BatchMetadata,
        batch_number: L1BatchNumber,
    ) -> Result<(), TransactionValidationError> {
        if receipt.status != Some(U64::one()) {
            return Err(TransactionValidationError::BatchTransactionInvalid {
                tx_hash: receipt.transaction_hash,
                reason: "transaction reverted".to_string(),
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

        let (batch_hash, commitment) = committed_batch_info.ok_or_else(|| {
            TransactionValidationError::BatchTransactionInvalid {
                tx_hash: receipt.transaction_hash,
                reason: format!(
                    "does not have `BlockCommit` event log for batch {}",
                    batch_number
                ),
            }
        })?;

        if db_batch.commitment != commitment {
            return Err(TransactionValidationError::BatchTransactionInvalid {
                tx_hash: receipt.transaction_hash,
                reason: format!(
                    "batch {} has different commitment: batch: {:?}, transaction log: {:?}",
                    batch_number, db_batch.commitment, commitment
                ),
            });
        }

        if db_batch.root_hash != batch_hash {
            return Err(TransactionValidationError::BatchTransactionInvalid {
                tx_hash: receipt.transaction_hash,
                reason: format!(
                    "batch {} has different root hash: batch: {:?}, transaction log: {:?}",
                    batch_number, db_batch.root_hash, batch_hash
                ),
            });
        }

        // OK verified successfully the commit transaction.
        Ok(())
    }

    pub fn validate_prove_tx(
        &self,
        receipt: &TransactionReceipt,
        _batch_metadata: L1BatchMetadata,
        batch_number: L1BatchNumber,
    ) -> Result<(), TransactionValidationError> {
        if receipt.status != Some(U64::one()) {
            return Err(TransactionValidationError::BatchTransactionInvalid {
                tx_hash: receipt.transaction_hash,
                reason: "transaction reverted".to_string(),
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

        let (from, to) =
            proved_from_to.ok_or_else(|| TransactionValidationError::BatchTransactionInvalid {
                tx_hash: receipt.transaction_hash,
                reason: "does not have `BlocksVerification` event log".to_string(),
            })?;

        if from >= batch_number.0 {
            return Err(TransactionValidationError::BatchTransactionInvalid {
                tx_hash: receipt.transaction_hash,
                reason: format!("has invalid `from` value for batch {}", batch_number),
            });
        }
        if to < batch_number.0 {
            return Err(TransactionValidationError::BatchTransactionInvalid {
                tx_hash: receipt.transaction_hash,
                reason: format!("has invalid `to` value for batch {}", batch_number),
            });
        }
        // OK verified successfully the prove transaction.
        Ok(())
    }

    /// Validates the execute transaction against the database.
    pub fn validate_execute_tx(
        &self,
        receipt: &TransactionReceipt,
        db_batch: L1BatchMetadata,
        batch_number: L1BatchNumber,
    ) -> Result<(), TransactionValidationError> {
        if receipt.status != Some(U64::one()) {
            return Err(TransactionValidationError::BatchTransactionInvalid {
                tx_hash: receipt.transaction_hash,
                reason: "transaction reverted".to_string(),
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

        let (batch_hash, commitment) = executed_batch_info.ok_or_else(|| {
            TransactionValidationError::BatchTransactionInvalid {
                tx_hash: receipt.transaction_hash,
                reason: format!(
                    "does not have `BlockExecution` event log for batch {}",
                    batch_number,
                ),
            }
        })?;
        if db_batch.commitment != commitment {
            return Err(TransactionValidationError::BatchTransactionInvalid {
                tx_hash: receipt.transaction_hash,
                reason: format!(
                    "has different commitment: batch {:?}, transaction log {:?}",
                    db_batch.commitment, commitment,
                ),
            });
        }
        if db_batch.root_hash != batch_hash {
            return Err(TransactionValidationError::BatchTransactionInvalid {
                tx_hash: receipt.transaction_hash,
                reason: format!(
                    "has different root hash: batch {:?}, transaction log {:?}",
                    db_batch.root_hash, batch_hash,
                ),
            });
        }
        // OK verified successfully the execute transaction.
        Ok(())
    }

    /// validates precommit transaction against miniblock header
    pub fn validate_precommit_tx(
        &self,
        receipt: &TransactionReceipt,
        miniblock_header: L2BlockHeader,
    ) -> Result<(), TransactionValidationError> {
        if receipt.status != Some(U64::one()) {
            return Err(TransactionValidationError::BatchTransactionInvalid {
                tx_hash: receipt.transaction_hash,
                reason: "transaction reverted".to_string(),
            });
        }

        let event = self
            .contract
            .event("BatchPrecommitmentSet")
            .context("`BatchPrecommitmentSet` event not found for ZKsync L1 contract")?;

        let precommitment: Option<H256> = receipt.logs.iter().find_map(|log| {
            if log.address != self.diamond_proxy_addr {
                tracing::debug!(
                    "Log address {} does not match diamond proxy address {}, skipping",
                    log.address,
                    self.diamond_proxy_addr
                );
                return None;
            }
            let parsed_log = event.parse_log_whole(log.clone().into()).ok()?; // Skip logs that are of different event type

            // we don't verify batchNumber, because verifying precommitment is enough.
            // And we don't have convenient access to that value.

            let l2_block_number_hint = get_param(&parsed_log.params, "untrustedLastL2BlockNumberHint")
                .and_then(ethabi::Token::into_uint)
                .and_then(|l2_block_number_hint| {
                    u32::try_from(l2_block_number_hint).ok().map(L2BlockNumber)
                })
                .expect(
                    "Missing expected `untrustedLastL2BlockNumberHint` parameter in `BatchPrecommitmentSet` event log",
                );

            if l2_block_number_hint != miniblock_header.number {
                tracing::debug!(
                    "Skipping event log miniblockNumber={l2_block_number_hint} for precommit transaction {}. \
                    We are checking for miniblockNumber={}",
                    receipt.transaction_hash,
                    miniblock_header.number.0,
                );
                return None;
            }

            let precommitment = get_param(&parsed_log.params, "precommitment")
                .and_then(ethabi::Token::into_fixed_bytes)
                .map(|bytes| H256::from_slice(&bytes))
                .expect(
                    "Missing expected `precommitment` parameter in `BatchPrecommitmentSet` event log",
                );

            Some(precommitment)
        });

        let precommitment =
            precommitment.ok_or_else(|| TransactionValidationError::BatchTransactionInvalid {
                tx_hash: receipt.transaction_hash,
                reason: "does not have `BatchPrecommitmentSet` event log".to_string(),
            })?;

        let Some(rolling_txs_hash) = miniblock_header.rolling_txs_hash else {
            return Err(TransactionValidationError::PrecommitTransactionInvalid {
                tx_hash: receipt.transaction_hash,
                reason: format!(
                    "we do not have the rolling_txs_hash value for miniblock {}. This is unexpected.",
                    miniblock_header.number
                ),
            });
        };

        if precommitment != rolling_txs_hash {
            return Err(TransactionValidationError::PrecommitTransactionInvalid {
                tx_hash: receipt.transaction_hash,
                reason: format!(
                    "has different precommitment: miniblock {:?}, transaction log {:?}",
                    rolling_txs_hash, precommitment,
                ),
            });
        }

        Ok(())
    }
}
