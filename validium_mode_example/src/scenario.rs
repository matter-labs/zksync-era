use std::{collections::HashMap, sync::Arc};

use colored::Colorize;
use ethers::{
    abi::Hash,
    providers::{Http, Provider},
};
use zksync_types::U64;
use zksync_web3_rs::zks_provider::ZKSProvider;

use crate::helpers::{self, BatchData, L1TxData, TxType};

pub struct ScenarioData(HashMap<U64, BatchData>);

impl ScenarioData {
    pub async fn collect(
        l2_txs_hashes: Vec<Hash>,
        l1_provider: Arc<Provider<Http>>,
        l2_provider: Arc<Provider<Http>>,
        signer_middleware: Arc<impl ZKSProvider>,
    ) -> Self {
        let mut batches_data: HashMap<U64, BatchData> = HashMap::new();
        for l2_tx_hash in l2_txs_hashes.clone().into_iter() {
            while helpers::wait_for_tx_receipt(l2_provider.clone(), l2_tx_hash)
                .await
                .other
                .get_deserialized::<U64>("l1BatchNumber")
                .unwrap()
                .is_err()
            {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }

            let batch_number: U64 = helpers::wait_for_tx_receipt(l2_provider.clone(), l2_tx_hash)
                .await
                .other
                .get_deserialized::<U64>("l1BatchNumber")
                .unwrap()
                .unwrap();

            let batch_details = {
                while signer_middleware
                    .get_l1_batch_details(batch_number.as_u64())
                    .await
                    .is_err()
                {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
                signer_middleware
                    .get_l1_batch_details(batch_number.as_u64())
                    .await
                    .unwrap()
            };

            batches_data
                .entry(batch_number)
                .and_modify(|e| {
                    e.l2_txs_hashes.push(l2_tx_hash);
                    e.l2_txs += 1;
                })
                .or_insert(BatchData {
                    batch_number,
                    l2_txs_hashes: vec![l2_tx_hash],
                    l2_txs: 1,
                    commit_tx_data: L1TxData {
                        tx_type: TxType::Commit,
                        hash: batch_details.commit_tx_hash,
                        gas_used: helpers::tx_gas_used(
                            l1_provider.clone(),
                            batch_details.commit_tx_hash,
                        )
                        .await,
                    },
                    prove_tx_data: L1TxData {
                        tx_type: TxType::Prove,
                        hash: batch_details.prove_tx_hash,
                        gas_used: helpers::tx_gas_used(
                            l1_provider.clone(),
                            batch_details.prove_tx_hash,
                        )
                        .await,
                    },
                    execute_tx_data: L1TxData {
                        tx_type: TxType::Execute,
                        hash: batch_details.execute_tx_hash,
                        gas_used: helpers::tx_gas_used(
                            l1_provider.clone(),
                            batch_details.execute_tx_hash,
                        )
                        .await,
                    },
                });
        }

        Self(batches_data)
    }
}

pub async fn run(accounts_count: usize, txs_per_account: usize, txs_kind: helpers::TxKind) {
    print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
    println!(
        "{}",
        format!("Running scenario: {accounts_count} account/s making {txs_per_account} {txs_kind} tx/s\n")
            .bright_red()
            .bold()
    );

    let l1_provider = helpers::l1_provider();
    let l2_provider = helpers::l2_provider();
    let main_wallet = Arc::new(helpers::zks_wallet(&l1_provider, &l2_provider).await);

    let accounts = helpers::create_funded_accounts(
        accounts_count,
        &l1_provider,
        &l2_provider,
        main_wallet.clone(),
    )
    .await;

    // In case that the txs_type is not Deploy we need the contract deployed.
    println!("{}", "Initial deploy".bright_yellow());
    let erc20_address = helpers::deploy(main_wallet.clone())
        .await
        .contract_address
        .unwrap();

    let l2_txs_hashes =
        helpers::send_transactions(txs_kind, erc20_address, accounts, txs_per_account).await;

    let data = ScenarioData::collect(
        l2_txs_hashes,
        l1_provider,
        l2_provider,
        main_wallet.get_era_provider().unwrap(),
    )
    .await;

    for (_batch_number, batch_data) in data.0.iter() {
        println!("{batch_data}");
    }
}

pub async fn basic() {
    println!(
        "{}",
        format!("Running basic scenario\n").bright_red().bold()
    );

    let l1_provider = helpers::l1_provider();
    let l2_provider = helpers::l2_provider();
    let main_wallet = Arc::new(helpers::zks_wallet(&l1_provider, &l2_provider).await);

    let account =
        helpers::create_funded_account(&l1_provider, &l2_provider, main_wallet.clone()).await;

    let deploy_receipt = helpers::deploy(account.clone()).await;
    let erc20_address = deploy_receipt.contract_address.unwrap();
    let mint_receipt = helpers::mint(account.clone(), erc20_address).await;
    let transfer_receipt = helpers::transfer(account.clone(), erc20_address).await;

    println!(
        "{}",
        format!(
            "Deploy L2 tx gas used: {}",
            deploy_receipt.gas_used.unwrap()
        )
        .bright_yellow()
    );
    println!(
        "{}",
        format!("Mint L2 tx gas used: {}", mint_receipt.gas_used.unwrap()).bright_yellow()
    );
    println!(
        "{}",
        format!(
            "Transfer L2 tx gas used: {}",
            transfer_receipt.gas_used.unwrap()
        )
        .bright_yellow()
    );
    print!("");

    let l2_txs_hashes = vec![
        deploy_receipt.transaction_hash,
        mint_receipt.transaction_hash,
        transfer_receipt.transaction_hash,
    ];

    let data = ScenarioData::collect(
        l2_txs_hashes,
        l1_provider,
        l2_provider,
        account.get_era_provider().unwrap(),
    )
    .await;

    for (_batch_number, batch_data) in data.0.iter() {
        println!("{batch_data}");
    }
}
