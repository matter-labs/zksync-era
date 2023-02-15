use crate::StorageProcessor;
use crate::{events_web3_dal::EventsWeb3Dal, models::storage_fee_monitor::StorageBlockGasData};
use zksync_config::constants::ERC20_TRANSFER_TOPIC;
use zksync_types::{
    api::{self, GetLogsFilter},
    Address, L1BatchNumber, L2_ETH_TOKEN_ADDRESS, U256,
};
use zksync_utils::address_to_h256;

// Dev note: these structure is not fundamental and exists for auxiliary
// monitoring purposes, it's use cases are limited and will normally appear
// together with calls to `FeeMonitorDal` methods, thus it doesn't really
// makes sense to extract it to the `types` crate.

#[derive(Debug)]
pub struct GasConsumptionData {
    pub consumed_gas: u64,
    pub base_gas_price: u64,
    pub priority_gas_price: u64,
}

impl GasConsumptionData {
    pub fn wei_spent(&self) -> u128 {
        (self.base_gas_price + self.priority_gas_price) as u128 * self.consumed_gas as u128
    }
}

#[derive(Debug)]
pub struct BlockGasConsumptionData {
    pub block_number: L1BatchNumber,
    pub commit: GasConsumptionData,
    pub prove: GasConsumptionData,
    pub execute: GasConsumptionData,
}

impl BlockGasConsumptionData {
    pub fn wei_spent(&self) -> u128 {
        self.commit.wei_spent() + self.prove.wei_spent() + self.execute.wei_spent()
    }
}

#[derive(Debug)]
pub struct FeeMonitorDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl FeeMonitorDal<'_, '_> {
    /// Returns data related to the gas consumption and gas costs for certain block.
    /// In case of any unexpected situation (i.e. some data is not present in the database)
    /// will return an error.
    pub fn get_block_gas_consumption(
        &mut self,
        block_number: L1BatchNumber,
    ) -> Result<BlockGasConsumptionData, anyhow::Error> {
        async_std::task::block_on(async {
            let res: StorageBlockGasData = sqlx::query_as!(
                StorageBlockGasData,
                r#"
                    SELECT
                        l1_batches.number,
                        commit_tx_data.gas_used as "commit_gas?",
                        commit_tx.base_fee_per_gas as "commit_base_gas_price?",
                        commit_tx.priority_fee_per_gas as "commit_priority_gas_price?",
                        prove_tx_data.gas_used as "prove_gas?",
                        prove_tx.base_fee_per_gas as "prove_base_gas_price?",
                        prove_tx.priority_fee_per_gas as "prove_priority_gas_price?",
                        execute_tx_data.gas_used as "execute_gas?",
                        execute_tx.base_fee_per_gas as "execute_base_gas_price?",
                        execute_tx.priority_fee_per_gas as "execute_priority_gas_price?"
                    FROM l1_batches
                    LEFT JOIN eth_txs_history as commit_tx
                        ON (l1_batches.eth_commit_tx_id = commit_tx.eth_tx_id AND commit_tx.confirmed_at IS NOT NULL)
                    LEFT JOIN eth_txs as commit_tx_data
                        ON (l1_batches.eth_commit_tx_id = commit_tx_data.id)
                    LEFT JOIN eth_txs_history as prove_tx
                        ON (l1_batches.eth_prove_tx_id = prove_tx.eth_tx_id AND prove_tx.confirmed_at IS NOT NULL)
                    LEFT JOIN eth_txs as prove_tx_data
                        ON (l1_batches.eth_prove_tx_id = prove_tx_data.id)
                    LEFT JOIN eth_txs_history as execute_tx
                        ON (l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id AND execute_tx.confirmed_at IS NOT NULL)
                    LEFT JOIN eth_txs as execute_tx_data
                        ON (l1_batches.eth_execute_tx_id = execute_tx_data.id)
                    WHERE l1_batches.number = $1
                "#,
                block_number.0 as i64
            )
            .fetch_optional(self.storage.conn())
            .await?
            .ok_or_else(|| anyhow::format_err!("No block details for requested block {block_number}"))?;

            // Closure extracting `u64` out of `Option<i64>`.
            // Normally we expect data to be present, but if for any reason it isn't we'll just return an error:
            // it's tracking module, so no big deal.
            let extract = |opt: Option<i64>| {
                opt.map(|val| val as u64).ok_or_else(|| {
                    anyhow::format_err!("Some field was `None` for block {block_number}. Data from database: {res:?}")
                })
            };

            Ok(BlockGasConsumptionData {
                block_number,
                commit: GasConsumptionData {
                    consumed_gas: extract(res.commit_gas)?,
                    base_gas_price: extract(res.commit_base_gas_price)?,
                    priority_gas_price: extract(res.commit_priority_gas_price)?,
                },
                prove: GasConsumptionData {
                    consumed_gas: extract(res.prove_gas)?,
                    base_gas_price: extract(res.prove_base_gas_price)?,
                    priority_gas_price: extract(res.prove_priority_gas_price)?,
                },
                execute: GasConsumptionData {
                    consumed_gas: extract(res.execute_gas)?,
                    base_gas_price: extract(res.execute_base_gas_price)?,
                    priority_gas_price: extract(res.execute_priority_gas_price)?,
                },
            })
        })
    }

    /// Fetches ETH ERC-20 transfers to a certain account for a certain block.
    /// Returns the vector of transfer amounts.
    pub fn fetch_erc20_transfers(
        &mut self,
        block_number: L1BatchNumber,
        account: Address,
    ) -> Result<Vec<U256>, anyhow::Error> {
        // We expect one log per transaction, thus limitiing is not really important.
        const MAX_LOGS_PER_BLOCK: usize = 100_000;

        // Event signature: `Transfer(address from, address to, uint256 value)`.
        // We're filtering by the 1st (signature hash) and 3rd (receiver).
        let topics = vec![
            (1, vec![ERC20_TRANSFER_TOPIC]),
            (3, vec![address_to_h256(&account)]),
        ];
        let miniblocks_range = match self
            .storage
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(block_number)
        {
            Some(range) => range,
            None => return Ok(Vec::new()),
        };

        let logs = {
            let mut events_web3_dal = EventsWeb3Dal {
                storage: self.storage,
            };
            events_web3_dal.get_logs(
                GetLogsFilter {
                    from_block: miniblocks_range.0,
                    to_block: Some(api::BlockNumber::Number(miniblocks_range.1 .0.into())),
                    addresses: vec![L2_ETH_TOKEN_ADDRESS],
                    topics,
                },
                MAX_LOGS_PER_BLOCK,
            )?
        };

        // Now collect the transfer amounts from retrieved logs.
        let balances: Vec<_> = logs
            .into_iter()
            .map(|log| U256::from_big_endian(&log.data.0))
            .collect();

        Ok(balances)
    }
}
