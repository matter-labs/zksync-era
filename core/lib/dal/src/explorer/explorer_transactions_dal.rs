use std::collections::HashMap;

use itertools::Itertools;
use once_cell::sync::Lazy;
use sqlx::Row;

use zksync_config::constants::ERC20_TRANSFER_TOPIC;
use zksync_types::api::Log;
use zksync_types::explorer_api::{
    BalanceChangeInfo, BalanceChangeType, Erc20TransferInfo, ExplorerTokenInfo,
    PaginationDirection, PaginationQuery, TransactionDetails, TransactionResponse,
    TransactionsResponse, TxPosition,
};
use zksync_types::{
    tokens::ETHEREUM_ADDRESS, tx::Execute, Address, L1BatchNumber, MiniblockNumber, H256,
    L2_ETH_TOKEN_ADDRESS, U256, U64,
};

use crate::models::storage_event::StorageWeb3Log;
use crate::models::storage_transaction::{
    transaction_details_from_storage, StorageTransactionDetails,
};
use crate::SqlxError;
use crate::StorageProcessor;

#[derive(Debug)]
pub struct ExplorerTransactionsDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl ExplorerTransactionsDal<'_, '_> {
    pub fn get_transactions_count_after(
        &mut self,
        block_number: MiniblockNumber,
    ) -> Result<usize, SqlxError> {
        async_std::task::block_on(async {
            let tx_count = sqlx::query!(
                r#"SELECT COUNT(*) as "count!" FROM transactions
                WHERE miniblock_number > $1 AND miniblock_number IS NOT NULL"#,
                block_number.0 as i64
            )
            .fetch_one(self.storage.conn())
            .await?
            .count as usize;
            Ok(tx_count)
        })
    }

    pub fn get_transaction_details(
        &mut self,
        hash: H256,
        l2_erc20_bridge_addr: Address,
    ) -> Result<Option<TransactionResponse>, SqlxError> {
        async_std::task::block_on(async {
            let tx_details: Option<StorageTransactionDetails> = sqlx::query_as!(
                StorageTransactionDetails,
                r#"
                    SELECT transactions.*, miniblocks.hash as "block_hash?",
                        commit_tx.tx_hash as "eth_commit_tx_hash?",
                        prove_tx.tx_hash as "eth_prove_tx_hash?",
                        execute_tx.tx_hash as "eth_execute_tx_hash?"
                    FROM transactions
                    LEFT JOIN miniblocks ON miniblocks.number = transactions.miniblock_number
                    LEFT JOIN l1_batches ON l1_batches.number = miniblocks.l1_batch_number
                    LEFT JOIN eth_txs_history as commit_tx ON (l1_batches.eth_commit_tx_id = commit_tx.eth_tx_id AND commit_tx.confirmed_at IS NOT NULL)
                    LEFT JOIN eth_txs_history as prove_tx ON (l1_batches.eth_prove_tx_id = prove_tx.eth_tx_id AND prove_tx.confirmed_at IS NOT NULL)
                    LEFT JOIN eth_txs_history as execute_tx ON (l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id AND execute_tx.confirmed_at IS NOT NULL)
                    WHERE transactions.hash = $1
                "#,
                hash.as_bytes()
            )
            .fetch_optional(self.storage.conn())
            .await?;
            let tx = if let Some(tx_details) = tx_details {
                let list = self
                    .storage_tx_list_to_tx_details_list(vec![tx_details], l2_erc20_bridge_addr)?;
                let tx = list[0].clone();
                let logs: Vec<Log> = sqlx::query_as!(
                    StorageWeb3Log,
                    r#"
                    SELECT
                        address, topic1, topic2, topic3, topic4, value,
                        Null::bytea as "block_hash", Null::bigint as "l1_batch_number?",
                        miniblock_number, tx_hash, tx_index_in_block,
                        event_index_in_block, event_index_in_tx
                    FROM events
                    WHERE tx_hash = $1
                    ORDER BY miniblock_number ASC, event_index_in_block ASC
                    "#,
                    hash.as_bytes()
                )
                .fetch_all(self.storage.conn())
                .await?
                .into_iter()
                .map(|storage_log: StorageWeb3Log| {
                    let mut log = Log::from(storage_log);
                    log.block_hash = tx.block_hash;
                    log.l1_batch_number = tx.l1_batch_number.map(|n| U64::from(n.0));
                    log
                })
                .collect();
                Some(TransactionResponse { tx, logs })
            } else {
                None
            };
            Ok(tx)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn get_transactions_page(
        &mut self,
        from_tx_location: Option<TxPosition>,
        block_number: Option<MiniblockNumber>,
        l1_batch_number: Option<L1BatchNumber>,
        contract_address: Option<Address>,
        pagination: PaginationQuery,
        max_total: usize,
        l2_erc20_bridge_addr: Address,
    ) -> Result<TransactionsResponse, SqlxError> {
        async_std::task::block_on(async {
            let (cmp_sign, order_str) = match pagination.direction {
                PaginationDirection::Older => ("<", "DESC"),
                PaginationDirection::Newer => (">", "ASC"),
            };
            let mut filters = vec!["transactions.miniblock_number IS NOT NULL".to_string()];
            if let Some(from_tx_location) = from_tx_location {
                if let Some(tx_index) = from_tx_location.tx_index {
                    filters.push(format!(
                        "(transactions.miniblock_number, transactions.index_in_block) {} ({}, {})",
                        cmp_sign, from_tx_location.block_number, tx_index
                    ));
                } else {
                    filters.push(format!(
                        "transactions.miniblock_number {} {}",
                        cmp_sign, from_tx_location.block_number
                    ));
                }
            }
            if let Some(address) = contract_address {
                filters.push(format!(
                    "(transactions.contract_address = '\\x{0}' OR transactions.initiator_address = '\\x{0}')",
                    hex::encode(address)
                ));
            }
            if let Some(number) = block_number {
                filters.push(format!("transactions.miniblock_number = {}", number.0));
            }
            if let Some(number) = l1_batch_number {
                filters.push(format!("transactions.l1_batch_number = {}", number.0));
            }
            let filters: String = if !filters.is_empty() {
                format!("WHERE {}", filters.join(" AND "))
            } else {
                "".to_string()
            };
            let ordering = format!(
                "transactions.miniblock_number {0}, transactions.index_in_block {0}",
                order_str
            );

            let sql_query_list_str = format!(
                r#"
                SELECT transactions.*, miniblocks.hash as "block_hash",
                    commit_tx.tx_hash as eth_commit_tx_hash,
                    prove_tx.tx_hash as eth_prove_tx_hash,
                    execute_tx.tx_hash as eth_execute_tx_hash
                FROM (
                    SELECT * FROM transactions
                    {0}
                    ORDER BY {1}
                    LIMIT {2}
                    OFFSET {3}
                ) as transactions
                LEFT JOIN miniblocks ON miniblocks.number = transactions.miniblock_number
                LEFT JOIN l1_batches ON l1_batches.number = miniblocks.l1_batch_number
                LEFT JOIN eth_txs_history as commit_tx ON (l1_batches.eth_commit_tx_id = commit_tx.eth_tx_id AND commit_tx.confirmed_at IS NOT NULL)
                LEFT JOIN eth_txs_history as prove_tx ON (l1_batches.eth_prove_tx_id = prove_tx.eth_tx_id AND prove_tx.confirmed_at IS NOT NULL)
                LEFT JOIN eth_txs_history as execute_tx ON (l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id AND execute_tx.confirmed_at IS NOT NULL)
                ORDER BY {1}
                "#,
                filters, ordering, pagination.limit, pagination.offset
            );
            let storage_txs: Vec<StorageTransactionDetails> = sqlx::query_as(&sql_query_list_str)
                .fetch_all(self.storage.conn())
                .await?;
            let list =
                self.storage_tx_list_to_tx_details_list(storage_txs, l2_erc20_bridge_addr)?;

            let sql_query_total_str = format!(
                r#"
                SELECT COUNT(*) as "count" FROM (
                    SELECT true FROM transactions
                    {}
                    LIMIT {}
                ) as c
                "#,
                filters, max_total
            );
            let total = sqlx::query(&sql_query_total_str)
                .fetch_one(self.storage.conn())
                .await?
                .get::<i64, &str>("count") as usize;

            Ok(TransactionsResponse { list, total })
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn get_account_transactions_page(
        &mut self,
        account_address: Address,
        from_tx_location: Option<TxPosition>,
        block_number: Option<MiniblockNumber>,
        pagination: PaginationQuery,
        max_total: usize,
        l2_erc20_bridge_addr: Address,
    ) -> Result<TransactionsResponse, SqlxError> {
        async_std::task::block_on(async {
            let order_str = match pagination.direction {
                PaginationDirection::Older => "DESC",
                PaginationDirection::Newer => "ASC",
            };

            let (hashes, total) = self.get_account_transactions_hashes_page(
                account_address,
                from_tx_location,
                block_number,
                pagination,
                max_total,
            )?;
            let sql_query_str = format!(
                r#"
                SELECT transactions.*, miniblocks.hash as "block_hash",
                    commit_tx.tx_hash as eth_commit_tx_hash,
                    prove_tx.tx_hash as eth_prove_tx_hash,
                    execute_tx.tx_hash as eth_execute_tx_hash
                FROM transactions
                LEFT JOIN miniblocks ON miniblocks.number = transactions.miniblock_number
                LEFT JOIN l1_batches ON l1_batches.number = miniblocks.l1_batch_number
                LEFT JOIN eth_txs_history as commit_tx ON (l1_batches.eth_commit_tx_id = commit_tx.eth_tx_id AND commit_tx.confirmed_at IS NOT NULL)
                LEFT JOIN eth_txs_history as prove_tx ON (l1_batches.eth_prove_tx_id = prove_tx.eth_tx_id AND prove_tx.confirmed_at IS NOT NULL)
                LEFT JOIN eth_txs_history as execute_tx ON (l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id AND execute_tx.confirmed_at IS NOT NULL)
                WHERE transactions.hash = ANY($1)
                ORDER BY transactions.miniblock_number {}, transactions.index_in_block {}
                "#,
                order_str, order_str
            );

            let sql_query = sqlx::query_as(&sql_query_str).bind(hashes);
            let storage_txs: Vec<StorageTransactionDetails> =
                sql_query.fetch_all(self.storage.conn()).await?;
            let list =
                self.storage_tx_list_to_tx_details_list(storage_txs, l2_erc20_bridge_addr)?;

            Ok(TransactionsResponse { list, total })
        })
    }

    fn get_account_transactions_hashes_page(
        &mut self,
        account_address: Address,
        from_tx_location: Option<TxPosition>,
        block_number: Option<MiniblockNumber>,
        pagination: PaginationQuery,
        max_total: usize,
    ) -> Result<(Vec<Vec<u8>>, usize), SqlxError> {
        async_std::task::block_on(async {
            let (cmp_sign, order_str) = match pagination.direction {
                PaginationDirection::Older => ("<", "DESC"),
                PaginationDirection::Newer => (">", "ASC"),
            };
            let mut optional_filters = String::new();
            if let Some(block_number) = block_number {
                optional_filters += format!("AND miniblock_number = {}\n", block_number.0).as_str();
            }
            if let Some(from_tx_location) = from_tx_location {
                if let Some(from_tx_index) = from_tx_location.tx_index {
                    optional_filters += format!(
                        "AND (miniblock_number, tx_index_in_block) {} ({}, {})\n",
                        cmp_sign, from_tx_location.block_number.0, from_tx_index
                    )
                    .as_str();
                } else {
                    optional_filters += format!(
                        "AND miniblock_number {} {}\n",
                        cmp_sign, from_tx_location.block_number.0
                    )
                    .as_str();
                }
            }

            let mut padded_address = [0u8; 12].to_vec();
            padded_address.extend_from_slice(account_address.as_bytes());

            let sql_query_str = format!(
                "
                    SELECT tx_hash FROM (
                        SELECT tx_hash, lag(tx_hash) OVER (ORDER BY miniblock_number {0}, tx_index_in_block {0}) as prev_hash,
                            miniblock_number, tx_index_in_block
                        FROM events
                        WHERE
                        (
                            (
                                (
                                    topic2 = $1
                                    OR
                                    topic3 = $1
                                )
                                AND topic1 = $2
                                AND (address IN (SELECT l2_address FROM tokens) OR address = $3)
                            )
                            OR events.tx_initiator_address = $4
                        )
                        {1}
                    ) AS h
                    WHERE prev_hash IS NULL OR tx_hash != prev_hash
                    ORDER BY miniblock_number {0}, tx_index_in_block {0}
                    LIMIT {2} OFFSET {3}
                ",
                order_str, optional_filters, pagination.limit, pagination.offset
            );
            let sql_query = sqlx::query(&sql_query_str)
                .bind(padded_address.clone())
                .bind(ERC20_TRANSFER_TOPIC.as_bytes().to_vec())
                .bind(L2_ETH_TOKEN_ADDRESS.as_bytes().to_vec())
                .bind(account_address.as_bytes().to_vec());
            let hashes: Vec<Vec<u8>> = sql_query
                .fetch_all(self.storage.conn())
                .await?
                .into_iter()
                .map(|row| row.get::<Vec<u8>, &str>("tx_hash"))
                .collect();

            let sql_count_query_str = format!(
                r#"
                SELECT COUNT(*) as "count" FROM (
                    SELECT true FROM (
                        SELECT tx_hash, lag(tx_hash) OVER (ORDER BY miniblock_number {0}, tx_index_in_block {0}) as prev_hash,
                            miniblock_number, tx_index_in_block
                        FROM events
                        WHERE
                        (
                            (
                                (
                                    topic2 = $1
                                    OR
                                    topic3 = $1
                                )
                                AND topic1 = $2
                                AND (address IN (SELECT l2_address FROM tokens) OR address = $3)
                            )
                            OR events.tx_initiator_address = $4
                        )
                        {1}
                    ) AS h
                    WHERE prev_hash IS NULL OR tx_hash != prev_hash
                    ORDER BY miniblock_number {0}, tx_index_in_block {0}
                    LIMIT {2}
                ) AS c
                "#,
                order_str, optional_filters, max_total
            );
            let sql_count_query = sqlx::query(&sql_count_query_str)
                .bind(padded_address)
                .bind(ERC20_TRANSFER_TOPIC.as_bytes().to_vec())
                .bind(L2_ETH_TOKEN_ADDRESS.as_bytes().to_vec())
                .bind(account_address.as_bytes().to_vec());
            let total = sql_count_query
                .fetch_one(self.storage.conn())
                .await?
                .get::<i64, &str>("count");
            Ok((hashes, total as usize))
        })
    }

    fn get_erc20_transfers(
        &mut self,
        hashes: Vec<Vec<u8>>,
    ) -> Result<HashMap<H256, Vec<Erc20TransferInfo>>, SqlxError> {
        async_std::task::block_on(async {
            let transfers = sqlx::query!(
                r#"
                SELECT tx_hash, topic2 as "topic2!", topic3 as "topic3!", value as "value!",
                    tokens.l1_address as "l1_address!", tokens.l2_address as "l2_address!",
                    tokens.symbol as "symbol!", tokens.name as "name!", tokens.decimals as "decimals!", tokens.usd_price as "usd_price?"
                FROM events
                INNER JOIN tokens ON
                    tokens.l2_address = events.address OR (events.address = $3 AND tokens.l2_address = $4)
                WHERE tx_hash = ANY($1) AND topic1 = $2
                ORDER BY tx_hash, miniblock_number ASC, event_index_in_block ASC
                "#,
                &hashes,
                ERC20_TRANSFER_TOPIC.as_bytes(),
                L2_ETH_TOKEN_ADDRESS.as_bytes(),
                ETHEREUM_ADDRESS.as_bytes(),
            )
            .fetch_all(self.storage.conn())
            .await?
            .into_iter()
            .group_by(|row| row.tx_hash.clone())
            .into_iter()
            .map(|(hash, group)| (H256::from_slice(&hash), group.map(|row| {
                let token_info = ExplorerTokenInfo {
                    l1_address: Address::from_slice(&row.l1_address),
                    l2_address: Address::from_slice(&row.l2_address),
                    address: Address::from_slice(&row.l2_address),
                    symbol: row.symbol,
                    name: row.name,
                    decimals: row.decimals as u8,
                    usd_price: row.usd_price,
                };
                let from = Self::erc20_decode_address_from_topic(H256::from_slice(&row.topic2));
                let to = Self::erc20_decode_address_from_topic(H256::from_slice(&row.topic3));
                let amount = U256::from_big_endian(&row.value);
                Erc20TransferInfo {
                    token_info,
                    from,
                    to,
                    amount,
                }
            }).collect::<Vec<Erc20TransferInfo>>()))
            .collect();
            Ok(transfers)
        })
    }

    fn get_withdrawals(
        &mut self,
        hashes: Vec<Vec<u8>>,
        l2_erc20_bridge_addr: Address,
    ) -> Result<HashMap<H256, Vec<BalanceChangeInfo>>, SqlxError> {
        async_std::task::block_on(async {
            static ERC20_WITHDRAW_EVENT_SIGNATURE: Lazy<H256> = Lazy::new(|| {
                zksync_contracts::l2_bridge_contract()
                    .event("WithdrawalInitiated")
                    .unwrap()
                    .signature()
            });

            let erc20_withdrawals: HashMap<H256, Vec<BalanceChangeInfo>> = sqlx::query!(
                r#"
                SELECT tx_hash, topic2 as "topic2!", topic3 as "topic3!", value as "value!",
                    tokens.l1_address as "l1_address!", tokens.l2_address as "l2_address!",
                    tokens.symbol as "symbol!", tokens.name as "name!", tokens.decimals as "decimals!", tokens.usd_price as "usd_price?"
                FROM events
                INNER JOIN tokens ON
                    events.topic4 = ('\x000000000000000000000000'::bytea || tokens.l2_address)
                WHERE tx_hash = ANY($1) AND events.topic1 = $2 AND events.address = $3
                ORDER BY tx_hash, miniblock_number ASC, event_index_in_block ASC
                "#,
                &hashes,
                ERC20_WITHDRAW_EVENT_SIGNATURE.as_bytes(),
                l2_erc20_bridge_addr.as_bytes()
            )
                .fetch_all(self.storage.conn())
                .await?
                .into_iter()
                .group_by(|row| row.tx_hash.clone())
                .into_iter()
                .map(|(hash, group)| (H256::from_slice(&hash), group.map(|row| {
                    let token_info = ExplorerTokenInfo {
                        l1_address: Address::from_slice(&row.l1_address),
                        l2_address: Address::from_slice(&row.l2_address),
                        address: Address::from_slice(&row.l2_address),
                        symbol: row.symbol,
                        name: row.name,
                        decimals: row.decimals as u8,
                        usd_price: row.usd_price,
                    };
                    let l2_sender = Self::erc20_decode_address_from_topic(H256::from_slice(&row.topic2));
                    let l1_receiver = Self::erc20_decode_address_from_topic(H256::from_slice(&row.topic3));
                    let amount = U256::from_big_endian(&row.value);
                    BalanceChangeInfo {
                        token_info,
                        from: l2_sender,
                        to: l1_receiver,
                        amount,
                        r#type: BalanceChangeType::Withdrawal
                    }
                }).collect::<Vec<BalanceChangeInfo>>()))
                .collect();

            static ETH_WITHDRAW_EVENT_SIGNATURE: Lazy<H256> = Lazy::new(|| {
                zksync_contracts::eth_contract()
                    .event("Withdrawal")
                    .unwrap()
                    .signature()
            });

            let eth_withdrawals: HashMap<H256, Vec<BalanceChangeInfo>> = sqlx::query!(
                r#"
                SELECT tx_hash, topic2 as "topic2!", topic3 as "topic3!", value as "value!",
                    tokens.l1_address as "l1_address!", tokens.l2_address as "l2_address!",
                    tokens.symbol as "symbol!", tokens.name as "name!", tokens.decimals as "decimals!", tokens.usd_price as "usd_price?"
                FROM events
                INNER JOIN tokens ON tokens.l2_address = '\x0000000000000000000000000000000000000000'
                WHERE tx_hash = ANY($1) AND events.topic1 = $2 AND events.address = $3
                ORDER BY tx_hash, miniblock_number ASC, event_index_in_block ASC
                "#,
                &hashes,
                ETH_WITHDRAW_EVENT_SIGNATURE.as_bytes(),
                L2_ETH_TOKEN_ADDRESS.as_bytes(),
            )
                .fetch_all(self.storage.conn())
                .await?
                .into_iter()
                .group_by(|row| row.tx_hash.clone())
                .into_iter()
                .map(|(hash, group)| (H256::from_slice(&hash), group.map(|row| {
                    let token_info = ExplorerTokenInfo {
                        l1_address: Address::from_slice(&row.l1_address),
                        l2_address: Address::from_slice(&row.l2_address),
                        address: Address::from_slice(&row.l2_address),
                        symbol: row.symbol,
                        name: row.name,
                        decimals: row.decimals as u8,
                        usd_price: row.usd_price,
                    };
                    let l2_sender = Self::erc20_decode_address_from_topic(H256::from_slice(&row.topic2));
                    let l1_receiver = Self::erc20_decode_address_from_topic(H256::from_slice(&row.topic3));
                    let amount = U256::from_big_endian(&row.value);
                    BalanceChangeInfo {
                        token_info,
                        from: l2_sender,
                        to: l1_receiver,
                        amount,
                        r#type: BalanceChangeType::Withdrawal
                    }
                }).collect::<Vec<BalanceChangeInfo>>()))
                .collect();

            let mut withdrawals = erc20_withdrawals;
            for (hash, mut items) in eth_withdrawals {
                withdrawals.entry(hash).or_default().append(&mut items);
            }

            Ok(withdrawals)
        })
    }

    /// Returns hashmap with transactions that are deposits.
    fn get_deposits(
        &mut self,
        hashes: Vec<Vec<u8>>,
        l2_erc20_bridge_addr: Address,
    ) -> Result<HashMap<H256, Vec<BalanceChangeInfo>>, SqlxError> {
        async_std::task::block_on(async {
            static ERC20_DEPOSIT_EVENT_SIGNATURE: Lazy<H256> = Lazy::new(|| {
                zksync_contracts::l2_bridge_contract()
                    .event("FinalizeDeposit")
                    .unwrap()
                    .signature()
            });
            let erc20_deposits: HashMap<H256, Vec<BalanceChangeInfo>> = sqlx::query!(
                r#"
                SELECT tx_hash, topic2 as "topic2!", topic3 as "topic3!", value as "value!",
                    tokens.l1_address as "l1_address!", tokens.l2_address as "l2_address!",
                    tokens.symbol as "symbol!", tokens.name as "name!", tokens.decimals as "decimals!", tokens.usd_price as "usd_price?"
                FROM events
                INNER JOIN tokens ON
                    events.topic4 = ('\x000000000000000000000000'::bytea || tokens.l2_address)
                WHERE tx_hash = ANY($1) AND events.topic1 = $2 AND events.address = $3
                "#,
                &hashes,
                ERC20_DEPOSIT_EVENT_SIGNATURE.as_bytes(),
                l2_erc20_bridge_addr.as_bytes()
            )
            .fetch_all(self.storage.conn())
            .await?
            .into_iter()
            .map(|row| {
                let token_info = ExplorerTokenInfo {
                    l1_address: Address::from_slice(&row.l1_address),
                    l2_address: Address::from_slice(&row.l2_address),
                    address: Address::from_slice(&row.l2_address),
                    symbol: row.symbol,
                    name: row.name,
                    decimals: row.decimals as u8,
                    usd_price: row.usd_price,
                };
                let l1_sender = Self::erc20_decode_address_from_topic(H256::from_slice(&row.topic2));
                let l2_receiver = Self::erc20_decode_address_from_topic(H256::from_slice(&row.topic3));
                let amount = U256::from_big_endian(&row.value);
                let deposit_info = BalanceChangeInfo {
                    token_info,
                    from: l1_sender,
                    to: l2_receiver,
                    amount,
                    r#type: BalanceChangeType::Deposit
                };
                (H256::from_slice(&row.tx_hash), vec![deposit_info])
            })
            .collect();

            static ETH_MINT_EVENT_SIGNATURE: Lazy<H256> = Lazy::new(|| {
                zksync_contracts::eth_contract()
                    .event("Mint")
                    .unwrap()
                    .signature()
            });
            let eth_deposits: HashMap<H256, Vec<BalanceChangeInfo>> = sqlx::query!(
                r#"
                SELECT events.tx_hash, transactions.initiator_address as "l1_sender!", events.topic2 as "topic2!", events.value as "value!",
                    tokens.l1_address as "l1_address!", tokens.l2_address as "l2_address!",
                    tokens.symbol as "symbol!", tokens.name as "name!", tokens.decimals as "decimals!", tokens.usd_price as "usd_price?"
                FROM events
                INNER JOIN tokens ON tokens.l2_address = '\x0000000000000000000000000000000000000000'
                INNER JOIN transactions ON transactions.hash = events.tx_hash
                WHERE tx_hash = ANY($1) AND events.topic1 = $2 AND events.address = $3
                ORDER BY tx_hash, events.miniblock_number ASC, event_index_in_block ASC
                "#,
                &hashes,
                ETH_MINT_EVENT_SIGNATURE.as_bytes(),
                L2_ETH_TOKEN_ADDRESS.as_bytes(),
            )
                .fetch_all(self.storage.conn())
                .await?
                .into_iter()
                .group_by(|row| row.tx_hash.clone())
                .into_iter()
                .map(|(hash, group)| (H256::from_slice(&hash), group.map(|row| {
                    let token_info = ExplorerTokenInfo {
                        l1_address: Address::from_slice(&row.l1_address),
                        l2_address: Address::from_slice(&row.l2_address),
                        address: Address::from_slice(&row.l2_address),
                        symbol: row.symbol,
                        name: row.name,
                        decimals: row.decimals as u8,
                        usd_price: row.usd_price,
                    };
                    let l1_sender = Address::from_slice(&row.l1_sender);
                    let l2_receiver = Self::erc20_decode_address_from_topic(H256::from_slice(&row.topic2));
                    let amount = U256::from_big_endian(&row.value);
                    BalanceChangeInfo {
                        token_info,
                        from: l1_sender,
                        to: l2_receiver,
                        amount,
                        r#type: BalanceChangeType::Deposit
                    }
                }).collect::<Vec<BalanceChangeInfo>>()))
                .collect();

            let mut deposits = erc20_deposits;
            for (hash, mut items) in eth_deposits {
                deposits.entry(hash).or_default().append(&mut items);
            }

            Ok(deposits)
        })
    }

    /// Returns hashmap with transactions that are ERC20 transfers.
    fn filter_erc20_transfers(
        &mut self,
        txs: &[StorageTransactionDetails],
    ) -> Result<HashMap<H256, Erc20TransferInfo>, SqlxError> {
        async_std::task::block_on(async {
            let hashes: Vec<Vec<u8>> = txs.iter().map(|tx| tx.hash.clone()).collect();
            // For transaction to be ERC20 transfer 2 conditions should be met
            // 1) It is an execute transaction and contract address is an ERC20 token.
            let filtered_by_contract_address: HashMap<H256, ExplorerTokenInfo> = sqlx::query!(
                r#"
                SELECT hash as "hash!",
                    tokens.l1_address as "l1_address!", tokens.l2_address as "l2_address!",
                    tokens.symbol as "symbol!", tokens.name as "name!", tokens.decimals as "decimals!", tokens.usd_price as "usd_price?"
                FROM transactions
                INNER JOIN tokens
                    ON tokens.l2_address = transactions.contract_address OR (transactions.contract_address = $2 AND tokens.l2_address = $3)
                WHERE hash = ANY($1)
                "#,
                &hashes,
                L2_ETH_TOKEN_ADDRESS.as_bytes(),
                ETHEREUM_ADDRESS.as_bytes(),
            )
            .fetch_all(self.storage.conn())
            .await?
            .into_iter()
            .map(|row| {
                let token_info = ExplorerTokenInfo {
                    l1_address: Address::from_slice(&row.l1_address),
                    l2_address: Address::from_slice(&row.l2_address),
                    address: Address::from_slice(&row.l2_address),
                    symbol: row.symbol,
                    name: row.name,
                    decimals: row.decimals as u8,
                    usd_price: row.usd_price,
                };
                (H256::from_slice(&row.hash), token_info)
            })
            .collect();

            // 2) Calldata is a valid ERC20 `transfer` calldata
            let erc20_transfers_iter = txs.iter().filter_map(|tx| {
                let hash = H256::from_slice(&tx.hash);
                if let Some(token_info) = filtered_by_contract_address.get(&hash).cloned() {
                    let execute = serde_json::from_value::<Execute>(tx.data.clone()).unwrap();
                    let calldata = execute.calldata();
                    Self::parse_erc20_transfer_calldata(calldata).map(|(to, amount)| {
                        let from = Address::from_slice(&tx.initiator_address);
                        (
                            hash,
                            Erc20TransferInfo {
                                from,
                                to,
                                amount,
                                token_info,
                            },
                        )
                    })
                } else {
                    None
                }
            });

            // Also include ETH transfers
            let eth_token_info = self
                .storage
                .explorer()
                .misc_dal()
                .get_token_details(Address::zero())?
                .expect("Info about ETH should be present in DB");
            let eth_transfers_iter = txs.iter().filter_map(|tx| {
                let hash = H256::from_slice(&tx.hash);
                let execute = serde_json::from_value::<Execute>(tx.data.clone()).unwrap();
                // All transactions with an empty calldata are considered to be called "transfers".
                if execute.calldata().is_empty() {
                    let from = Address::from_slice(&tx.initiator_address);
                    let to = execute.contract_address;
                    let amount = execute.value;

                    Some((
                        hash,
                        Erc20TransferInfo {
                            from,
                            to,
                            amount,
                            token_info: eth_token_info.clone(),
                        },
                    ))
                } else {
                    None
                }
            });

            let result = erc20_transfers_iter.chain(eth_transfers_iter).collect();
            Ok(result)
        })
    }

    fn storage_tx_list_to_tx_details_list(
        &mut self,
        txs: Vec<StorageTransactionDetails>,
        l2_erc20_bridge_addr: Address,
    ) -> Result<Vec<TransactionDetails>, SqlxError> {
        let hashes: Vec<Vec<u8>> = txs.iter().map(|tx| tx.hash.clone()).collect();
        let erc20_transfers_map = self.get_erc20_transfers(hashes.clone())?;
        let withdrawals_map = self.get_withdrawals(hashes.clone(), l2_erc20_bridge_addr)?;
        let erc20_transfers_filtered = self.filter_erc20_transfers(&txs)?;
        let deposits_map = self.get_deposits(hashes, l2_erc20_bridge_addr)?;
        let txs = txs
            .into_iter()
            .map(|tx_details| {
                Self::build_transaction_details(
                    &erc20_transfers_map,
                    &withdrawals_map,
                    &erc20_transfers_filtered,
                    &deposits_map,
                    tx_details,
                )
            })
            .collect();
        Ok(txs)
    }

    fn build_transaction_details(
        erc20_transfers_map: &HashMap<H256, Vec<Erc20TransferInfo>>,
        withdrawals_map: &HashMap<H256, Vec<BalanceChangeInfo>>,
        filtered_transfers: &HashMap<H256, Erc20TransferInfo>,
        deposits_map: &HashMap<H256, Vec<BalanceChangeInfo>>,
        tx_details: StorageTransactionDetails,
    ) -> TransactionDetails {
        let hash = H256::from_slice(&tx_details.hash);
        let erc20_transfers = erc20_transfers_map.get(&hash).cloned().unwrap_or_default();
        let withdrawals = withdrawals_map.get(&hash).cloned().unwrap_or_default();
        let transfer = filtered_transfers.get(&hash).cloned();
        let deposits = deposits_map.get(&hash).cloned().unwrap_or_default();
        transaction_details_from_storage(
            tx_details,
            erc20_transfers,
            withdrawals,
            transfer,
            deposits,
        )
    }

    /// Checks if calldata is erc20 `transfer` calldata and parses (to, amount) from it
    fn parse_erc20_transfer_calldata(calldata: Vec<u8>) -> Option<(Address, U256)> {
        // Check calldata length
        if calldata.len() != 68 {
            return None;
        }
        // Check signature match
        if calldata[0..4].to_vec() != vec![0xa9, 0x05, 0x9c, 0xbb] {
            return None;
        }
        let to = Address::from_slice(&calldata[16..36]);
        let amount = U256::from_big_endian(&calldata[36..68]);
        Some((to, amount))
    }

    fn erc20_decode_address_from_topic(topic: H256) -> Address {
        Address::from_slice(&topic.as_bytes()[12..])
    }
}
