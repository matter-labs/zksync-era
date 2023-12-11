use std::time::Duration;

use zksync_contracts::PRE_BOOJUM_COMMIT_FUNCTION;
use zksync_dal::ConnectionPool;
use zksync_types::{
    web3::{error, ethabi, transports::Http, types::TransactionId, Web3},
    L1BatchNumber,
};

use crate::metrics::{CheckerComponent, EN_METRICS};

#[derive(Debug)]
pub struct ConsistencyChecker {
    // ABI of the zkSync contract
    contract: ethabi::Contract,
    // How many past batches to check when starting
    max_batches_to_recheck: u32,
    web3: Web3<Http>,
    db: ConnectionPool,
}

const SLEEP_DELAY: Duration = Duration::from_secs(5);

impl ConsistencyChecker {
    pub fn new(web3_url: &str, max_batches_to_recheck: u32, db: ConnectionPool) -> Self {
        let web3 = Web3::new(Http::new(web3_url).unwrap());
        let contract = zksync_contracts::zksync_contract();
        Self {
            web3,
            contract,
            max_batches_to_recheck,
            db,
        }
    }

    async fn check_commitments(&self, batch_number: L1BatchNumber) -> Result<bool, error::Error> {
        let mut storage = self.db.access_storage().await.unwrap();

        let storage_l1_batch = storage
            .blocks_dal()
            .get_storage_l1_batch(batch_number)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("L1 batch #{} not found in the database", batch_number));

        let commit_tx_id = storage_l1_batch
            .eth_commit_tx_id
            .unwrap_or_else(|| panic!("Commit tx not found for L1 batch #{}", batch_number))
            as u32;

        let block_metadata = storage
            .blocks_dal()
            .get_l1_batch_with_metadata(storage_l1_batch)
            .await
            .unwrap()
            .unwrap_or_else(|| {
                panic!(
                    "Metadata for L1 batch #{} not found in the database",
                    batch_number
                )
            });

        let commit_tx_hash = storage
            .eth_sender_dal()
            .get_confirmed_tx_hash_by_eth_tx_id(commit_tx_id)
            .await
            .unwrap()
            .unwrap_or_else(|| {
                panic!(
                    "Commit tx hash not found in the database. Commit tx id: {}",
                    commit_tx_id
                )
            });

        tracing::info!(
            "Checking commit tx {} for batch {}",
            commit_tx_hash,
            batch_number.0
        );

        // we can't get tx calldata from db because it can be fake
        let commit_tx = self
            .web3
            .eth()
            .transaction(TransactionId::Hash(commit_tx_hash))
            .await?
            .expect("Commit tx not found on L1");

        let commit_tx_status = self
            .web3
            .eth()
            .transaction_receipt(commit_tx_hash)
            .await?
            .expect("Commit tx receipt not found on L1")
            .status;

        assert_eq!(
            commit_tx_status,
            Some(1.into()),
            "Main node gave us a failed commit tx"
        );

        let commit_function = if block_metadata
            .header
            .protocol_version
            .unwrap()
            .is_pre_boojum()
        {
            PRE_BOOJUM_COMMIT_FUNCTION.clone()
        } else {
            self.contract.function("commitBatches").unwrap().clone()
        };

        let commitments = commit_function
            .decode_input(&commit_tx.input.0[4..])
            .unwrap()
            .pop()
            .unwrap()
            .into_array()
            .unwrap();

        // Commit transactions usually publish multiple commitments at once, so we need to find
        // the one that corresponds to the batch we're checking.
        let first_batch_number = match &commitments[0] {
            ethabi::Token::Tuple(tuple) => tuple[0].clone().into_uint().unwrap().as_usize(),
            _ => panic!("ABI does not match the expected one"),
        };
        let commitment = &commitments[batch_number.0 as usize - first_batch_number];

        Ok(commitment == &block_metadata.l1_commit_data())
    }

    async fn last_committed_batch(&self) -> L1BatchNumber {
        self.db
            .access_storage()
            .await
            .unwrap()
            .blocks_dal()
            .get_number_of_last_l1_batch_committed_on_eth()
            .await
            .unwrap()
            .unwrap_or(L1BatchNumber(0))
    }

    pub async fn run(
        self,
        stop_receiver: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let mut batch_number: L1BatchNumber = self
            .last_committed_batch()
            .await
            .0
            .saturating_sub(self.max_batches_to_recheck)
            .max(1)
            .into();

        tracing::info!("Starting consistency checker from batch {}", batch_number.0);

        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, consistency_checker is shutting down");
                break;
            }

            let metadata = self
                .db
                .access_storage()
                .await
                .unwrap()
                .blocks_dal()
                .get_l1_batch_metadata(batch_number)
                .await
                .unwrap();
            let batch_has_metadata = metadata
                .map(|m| {
                    m.metadata.bootloader_initial_content_commitment.is_some()
                        && m.metadata.events_queue_commitment.is_some()
                })
                .unwrap_or(false);

            // The batch might be already committed but not yet processed by the external node's tree
            // OR the batch might be processed by the external node's tree but not yet committed.
            // We need both.
            if !batch_has_metadata || self.last_committed_batch().await < batch_number {
                tokio::time::sleep(SLEEP_DELAY).await;
                continue;
            }

            match self.check_commitments(batch_number).await {
                Ok(true) => {
                    tracing::info!("Batch {} is consistent with L1", batch_number.0);
                    EN_METRICS.last_correct_batch[&CheckerComponent::ConsistencyChecker]
                        .set(batch_number.0.into());
                    batch_number.0 += 1;
                }
                Ok(false) => {
                    anyhow::bail!("Batch {} is inconsistent with L1", batch_number.0);
                }
                Err(e) => {
                    tracing::warn!("Consistency checker error: {}", e);
                    tokio::time::sleep(SLEEP_DELAY).await;
                }
            }
        }
        Ok(())
    }
}
