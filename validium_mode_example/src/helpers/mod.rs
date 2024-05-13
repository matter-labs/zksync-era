mod batch_data;
mod l1_tx_data;
mod tx_kind;
mod tx_type;

use std::{str::FromStr, sync::Arc, time::Duration};

pub use batch_data::BatchData;
use colored::Colorize;
use ethers::{
    abi::{Abi, Address, Hash},
    core::k256::ecdsa::SigningKey,
    providers::Http,
    types::TransactionReceipt,
    utils::parse_units,
};
pub use l1_tx_data::L1TxData;
use loadnext::config::LoadtestConfig;
use tokio::{sync::Mutex, time::sleep};
pub use tx_kind::TxKind;
pub use tx_type::TxType;
use zksync_types::{api::TransactionDetails, web3::futures::future::join_all, H160, H256, U256};
use zksync_web3_decl::{jsonrpsee::http_client::HttpClientBuilder, namespaces::ZksNamespaceClient};
use zksync_web3_rs::{
    eip712::Eip712TransactionRequest,
    providers::{Middleware, Provider},
    signers::{LocalWallet, Signer},
    zks_provider::ZKSProvider,
    zks_wallet::{DeployRequest, DepositRequest, TransferRequest, WithdrawRequest},
    ZKSWallet,
};

use crate::constants;

pub fn l1_provider() -> Arc<Provider<Http>> {
    Arc::new(
        Provider::<Http>::try_from(constants::L1_URL).expect("Could not instantiate L1 Provider"),
    )
}

pub fn l2_provider() -> Arc<Provider<Http>> {
    Arc::new(
        Provider::try_from(constants::ERA_PROVIDER_URL).expect("Could not instantiate L2 Provider"),
    )
}

pub async fn zks_wallet(
    l1_provider: &Provider<Http>,
    l2_provider: &Provider<Http>,
) -> ZKSWallet<Provider<Http>, SigningKey> {
    let chain_id = l2_provider.get_chainid().await.unwrap();
    let l2_wallet = LocalWallet::from_str(constants::PRIVATE_KEY)
        .unwrap()
        .with_chain_id(chain_id.as_u64());
    ZKSWallet::new(
        l2_wallet,
        None,
        Some(l2_provider.clone()),
        Some(l1_provider.clone()),
    )
    .unwrap()
}

pub async fn deposit_eth(
    from: Arc<ZKSWallet<Provider<Http>, SigningKey>>,
    to: Arc<ZKSWallet<Provider<Http>, SigningKey>>,
) {
    println!(
        "{}",
        format!("Depositing in {:?}", to.l2_address()).bright_yellow()
    );
    let amount = parse_units("11", "ether").unwrap();
    let mut request = DepositRequest::new(amount.into());
    request.to = Some(to.l2_address());
    from.deposit(&request)
        .await
        .expect("Failed to perform deposit transaction");
}

pub async fn deploy_erc20(
    zks_wallet: Arc<ZKSWallet<Provider<Http>, SigningKey>>,
) -> TransactionReceipt {
    // Read both files from disk:
    let abi = Abi::load(constants::ERC20_ABI.as_bytes()).unwrap();
    let contract_bin = hex::decode(constants::ERC20_BIN).unwrap().to_vec();

    // DeployRequest sets the parameters for the constructor call and the deployment transaction.
    let request = DeployRequest::with(
        abi,
        contract_bin,
        vec!["ToniToken".to_owned(), "teth".to_owned()],
    )
    .from(zks_wallet.l2_address());

    let eip712_request: Eip712TransactionRequest = request.clone().try_into().unwrap();

    let l2_deploy_tx_receipt = zks_wallet
        .get_era_provider()
        .unwrap()
        .clone()
        .send_transaction_eip712(&zks_wallet.l2_wallet, eip712_request)
        .await
        .unwrap()
        .await
        .unwrap()
        .unwrap();

    l2_deploy_tx_receipt
}

pub async fn mint_erc20(
    zks_wallet: Arc<ZKSWallet<Provider<Http>, SigningKey>>,
    erc20_address: H160,
) -> TransactionReceipt {
    zks_wallet
        .get_era_provider()
        .unwrap()
        .clone()
        .send_eip712(
            &zks_wallet.l2_wallet,
            erc20_address,
            "_mint(address, uint256)",
            Some(
                [
                    "CD2a3d9F938E13CD947Ec05AbC7FE734Df8DD826".into(),
                    "10000000000".into(),
                ]
                .into(),
            ),
            None,
        )
        .await
        .unwrap()
        .await
        .unwrap()
        .unwrap()
}

pub async fn transfer_eth(
    from: Arc<ZKSWallet<Provider<Http>, SigningKey>>,
    to: Arc<ZKSWallet<Provider<Http>, SigningKey>>,
) {
    println!(
        "{}",
        format!("Transfer eth in {:?}", to.l2_address()).bright_yellow()
    );
    let amount = parse_units("11", "ether").unwrap();
    let mut request = TransferRequest::new(amount.into());
    request.to = to.l2_address();

    from.transfer(&request, None)
        .await
        .expect("Failed to perform transfer transaction");
}

pub async fn transfer_erc20(
    zks_wallet: Arc<ZKSWallet<Provider<Http>, SigningKey>>,
    erc20_address: H160,
) -> TransactionReceipt {
    zks_wallet
        .get_era_provider()
        .unwrap()
        .clone()
        .send_eip712(
            &zks_wallet.l2_wallet,
            erc20_address,
            "_transfer(address, address, uint256)",
            Some(
                [
                    "CD2a3d9F938E13CD947Ec05AbC7FE734Df8DD826".into(),
                    "bBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB".into(),
                    "100".into(),
                ]
                .into(),
            ),
            None,
        )
        .await
        .unwrap()
        .await
        .unwrap()
        .unwrap()
}

pub async fn wait_for_l2_tx_details(tx_hash: H256) -> TransactionDetails {
    let config = LoadtestConfig::from_env()
        .expect("Config parameters should be loaded from env or from default values");

    let client = HttpClientBuilder::default()
        .build(config.l2_rpc_address)
        .unwrap();
    loop {
        let details = client
            .get_transaction_details(tx_hash)
            .await
            .unwrap()
            .unwrap();

        if details.eth_commit_tx_hash.is_some()
            && details.eth_prove_tx_hash.is_some()
            && details.eth_execute_tx_hash.is_some()
        {
            break details;
        }

        sleep(Duration::from_secs(1)).await;
    }
}

pub async fn withdraw_eth(
    from: Arc<ZKSWallet<Provider<Http>, SigningKey>>,
    to: Arc<ZKSWallet<Provider<Http>, SigningKey>>,
) {
    println!(
        "{}",
        format!("Withdrawing in {:?}", to.l2_address()).bright_yellow()
    );
    let amount = parse_units("11", "ether").unwrap();
    let mut request = WithdrawRequest::new(amount.into());
    request.to = to.l2_address();

    from.withdraw(&request)
        .await
        .expect("Failed to perform withdraw transaction");
}

pub async fn wait_for_tx_receipt(client: Arc<Provider<Http>>, tx_hash: H256) -> TransactionReceipt {
    loop {
        let receipt = client
            .get_transaction_receipt(tx_hash.clone())
            .await
            .unwrap();
        if receipt.is_some() {
            break receipt.unwrap();
        }
        sleep(Duration::from_secs(1)).await;
    }
}

pub async fn tx_gas_used(client: Arc<Provider<Http>>, tx_hash: H256) -> U256 {
    let receipt = wait_for_tx_receipt(client, tx_hash).await;
    receipt.gas_used.unwrap()
}

pub async fn create_account(
    l1_provider: &Provider<Http>,
    l2_provider: &Provider<Http>,
) -> Arc<ZKSWallet<Provider<Http>, SigningKey>> {
    let chain_id = l2_provider.get_chainid().await.unwrap().as_u64();
    Arc::new(
        ZKSWallet::new(
            LocalWallet::new(&mut ethers::core::rand::thread_rng()).with_chain_id(chain_id),
            None,
            Some(l2_provider.clone()),
            Some(l1_provider.clone()),
        )
        .unwrap(),
    )
}

pub async fn create_accounts(
    count: usize,
    l1_provider: &Provider<Http>,
    l2_provider: &Provider<Http>,
) -> Vec<Arc<ZKSWallet<Provider<Http>, SigningKey>>> {
    let mut accounts = Vec::new();
    for _ in 0..count {
        accounts.push(create_account(l1_provider, l2_provider).await)
    }
    accounts
}

pub async fn create_funded_account(
    l1_provider: &Provider<Http>,
    l2_provider: &Provider<Http>,
    main_wallet: Arc<ZKSWallet<Provider<Http>, SigningKey>>,
) -> Arc<ZKSWallet<Provider<Http>, SigningKey>> {
    let to = create_account(l1_provider, l2_provider).await;
    let _ = deposit_eth(main_wallet, to.clone()).await;
    to
}

pub async fn create_funded_accounts(
    count: usize,
    l1_provider: &Provider<Http>,
    l2_provider: &Provider<Http>,
    main_wallet: Arc<ZKSWallet<Provider<Http>, SigningKey>>,
) -> Vec<Arc<ZKSWallet<Provider<Http>, SigningKey>>> {
    let mut accounts = Vec::new();
    // Fund accounts
    for _ in 0..count {
        let from = main_wallet.clone();
        let to = create_account(l1_provider, l2_provider).await;
        accounts.push(to.clone());
        let _ = deposit_eth(from, to).await;
    }
    accounts
}

pub async fn send_transactions(
    kind: TxKind,
    erc20_address: Address,
    accounts: Vec<Arc<ZKSWallet<Provider<Http>, SigningKey>>>,
    txs_per_account: usize,
) -> Vec<Hash> {
    let l2_txs_receipts = Arc::new(Mutex::new(Vec::new()));

    let mut l2_txs_handles = Vec::new();
    for account in accounts.into_iter() {
        let l2_txs_receipts_clone = l2_txs_receipts.clone();
        l2_txs_handles.push(tokio::spawn(async move {
            for tx_counter_i in 0..txs_per_account {
                let tx_receipt = kind
                    .run(
                        account.clone(),
                        erc20_address,
                        tx_counter_i + 1,
                        txs_per_account,
                    )
                    .await;
                l2_txs_receipts_clone.lock().await.push(tx_receipt);
            }
        }));
    }

    join_all(l2_txs_handles)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    l2_txs_receipts
        .clone()
        .lock()
        .await
        .clone()
        .into_iter()
        .map(|tx| tx.transaction_hash)
        .collect()
}
