use std::{str::FromStr, time::Duration};

use colored::Colorize;
use ethers::{abi::Abi, providers::Http, utils::parse_units};
use loadnext::config::LoadtestConfig;
use tokio::time::sleep;
use zksync_web3_decl::{
    jsonrpsee::http_client::HttpClientBuilder,
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};
use zksync_web3_rs::{
    eip712::Eip712TransactionRequest,
    providers::{Middleware, Provider},
    signers::{LocalWallet, Signer},
    zks_provider::ZKSProvider,
    zks_wallet::{DeployRequest, DepositRequest},
    ZKSWallet,
};

static ERA_PROVIDER_URL: &str = "http://127.0.0.1:3050";
static PRIVATE_KEY: &str = "7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110";

static CONTRACT_BIN: &str = include_str!("../ERC20.bin");
static CONTRACT_ABI: &str = include_str!("../ERC20.abi");

static L1_URL: &str = "http://localhost:8545";

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let l1_provider =
        Provider::<Http>::try_from(L1_URL).expect("Could not instantiate L1 Provider");
    let zk_wallet = {
        let era_provider = Provider::try_from(ERA_PROVIDER_URL).unwrap();

        let chain_id = era_provider.get_chainid().await.unwrap();
        let l2_wallet = LocalWallet::from_str(PRIVATE_KEY)
            .unwrap()
            .with_chain_id(chain_id.as_u64());
        ZKSWallet::new(
            l2_wallet,
            None,
            Some(era_provider.clone()),
            Some(l1_provider.clone()),
        )
        .unwrap()
    };

    let config = LoadtestConfig::from_env()
        .expect("Config parameters should be loaded from env or from default values");

    let l1_rpc_client = HttpClientBuilder::default()
        .build(config.l1_rpc_address)
        .unwrap();
    let l2_rpc_client = HttpClientBuilder::default()
        .build(config.l2_rpc_address)
        .unwrap();

    let deposit_transaction_hash = {
        let amount = parse_units("11", "ether").unwrap();
        let request = DepositRequest::new(amount.into());
        zk_wallet
            .deposit(&request)
            .await
            .expect("Failed to perform deposit transaction")
    };
    println!();
    println!("Deposit transaction hash: {:?}", deposit_transaction_hash);

    // Deploy contract:
    let contract_address = {
        // Read both files from disk:
        let abi = Abi::load(CONTRACT_ABI.as_bytes()).unwrap();
        let contract_bin = hex::decode(CONTRACT_BIN).unwrap().to_vec();

        // DeployRequest sets the parameters for the constructor call and the deployment transaction.
        let request = DeployRequest::with(
            abi,
            contract_bin,
            vec!["ToniToken".to_owned(), "teth".to_owned()],
        )
        .from(zk_wallet.l2_address());

        let eip712_request: Eip712TransactionRequest = request.clone().try_into().unwrap();
        println!("{}", "Deploy".bright_magenta());

        let transaction_receipt = zk_wallet
            .get_era_provider()
            .unwrap()
            .clone()
            .send_transaction_eip712(&zk_wallet.l2_wallet, eip712_request)
            .await
            .unwrap()
            .await
            .unwrap()
            .unwrap();

        let address = transaction_receipt.contract_address.unwrap();
        println!("Contract address: {:#?}", address);

        let transaction_hash_deploy = transaction_receipt.transaction_hash;
        let transaction_hash_formatted_deploy =
            format!("{:#?}", transaction_receipt.transaction_hash);
        println!("Transaction hash {}", transaction_hash_formatted_deploy);
        let transaction_gas_used_formatted_deploy =
            format!("{:#?}", transaction_receipt.gas_used.unwrap());
        println!(
            "Transaction gas used {}",
            transaction_gas_used_formatted_deploy.cyan()
        );
        let l2_transaction_deploy = {
            loop {
                let l2_transaction = l2_rpc_client
                    .get_transaction_details(transaction_hash_deploy)
                    .await
                    .unwrap()
                    .unwrap();

                if l2_transaction.eth_commit_tx_hash.is_some() {
                    break l2_transaction.clone();
                }

                sleep(Duration::from_secs(1)).await;
            }
        };

        let l2_tx_fee_formatted_deploy = format!("{:#?}", l2_transaction_deploy.fee);
        println!("L2 fee: {}", l2_tx_fee_formatted_deploy.green());

        address
    };
    println!();

    println!("{}", "Mint".bright_magenta());
    let receipt_mint = zk_wallet
        .get_era_provider()
        .unwrap()
        .clone()
        .send_eip712(
            &zk_wallet.l2_wallet,
            contract_address,
            "_mint(address, uint256)",
            Some(
                [
                    "CD2a3d9F938E13CD947Ec05AbC7FE734Df8DD826".into(),
                    "100000".into(),
                ]
                .into(),
            ),
            None,
        )
        .await
        .unwrap()
        .await
        .unwrap()
        .unwrap();

    let transaction_hash_mint = receipt_mint.transaction_hash;
    let transaction_hash_formatted_mint = format!("{:#?}", receipt_mint.transaction_hash);
    println!("Transaction hash {}", transaction_hash_formatted_mint);
    let transaction_gas_used_formatted_mint = format!("{:#?}", receipt_mint.gas_used.unwrap());
    println!(
        "Transaction gas used {}",
        transaction_gas_used_formatted_mint.cyan()
    );
    let l2_transaction_mint = {
        loop {
            let l2_transaction = l2_rpc_client
                .get_transaction_details(transaction_hash_mint)
                .await
                .unwrap()
                .unwrap();

            if l2_transaction.eth_commit_tx_hash.is_some() {
                break l2_transaction.clone();
            }

            sleep(Duration::from_secs(1)).await; // Adjust the duration as needed
        }
    };

    let l2_tx_fee_formatted_mint = format!("{:#?}", l2_transaction_mint.fee);
    println!("L2 fee: {}", l2_tx_fee_formatted_mint.green());

    let l1_transaction_transfer = l1_rpc_client
        .get_transaction_by_hash(l2_transaction_mint.eth_commit_tx_hash.unwrap())
        .await
        .unwrap()
        .unwrap();

    let l1_max_fee_per_gas_mint = l1_transaction_transfer.max_fee_per_gas.unwrap();
    let l1_max_fee_per_gas_formatted_mint = format!("{:#?}", l1_max_fee_per_gas_mint);
    println!(
        "L1 max fee per gas: {}",
        l1_max_fee_per_gas_formatted_mint.cyan()
    );
    println!();

    let values: Vec<&str> = vec!["1000"];

    for &value in &values {
        println!("Transfer {}", value.bright_magenta());
        let receipt_transfer = zk_wallet
            .get_era_provider()
            .unwrap()
            .clone()
            .send_eip712(
                &zk_wallet.l2_wallet,
                contract_address,
                "_transfer(address, address, uint256)",
                Some(
                    [
                        "CD2a3d9F938E13CD947Ec05AbC7FE734Df8DD826".into(),
                        "bBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB".into(),
                        value.into(),
                    ]
                    .into(),
                ),
                None,
            )
            .await
            .unwrap()
            .await
            .unwrap()
            .unwrap();

        let transaction_hash_transfer = receipt_transfer.transaction_hash;
        let transaction_hash_formatted_transfer =
            format!("{:#?}", receipt_transfer.transaction_hash);
        println!("Transaction hash {}", transaction_hash_formatted_transfer);
        let transaction_gas_used_formatted_transfer =
            format!("{:#?}", receipt_transfer.gas_used.unwrap());
        println!(
            "Transaction gas used {}",
            transaction_gas_used_formatted_transfer.cyan()
        );
        let l2_transaction_transfer = {
            loop {
                let l2_transaction = l2_rpc_client
                    .get_transaction_details(transaction_hash_transfer)
                    .await
                    .unwrap()
                    .unwrap();

                if l2_transaction.eth_commit_tx_hash.is_some() {
                    break l2_transaction.clone();
                }

                sleep(Duration::from_secs(1)).await; // Adjust the duration as needed
            }
        };

        let l2_tx_fee_formatted_transfer = format!("{:#?}", l2_transaction_transfer.fee);
        println!("L2 fee: {}", l2_tx_fee_formatted_transfer.green());

        let l1_transaction_transfer = l1_rpc_client
            .get_transaction_by_hash(l2_transaction_transfer.eth_commit_tx_hash.unwrap())
            .await
            .unwrap()
            .unwrap();

        let l1_max_fee_per_gas_transfer = l1_transaction_transfer.max_fee_per_gas.unwrap();
        let l1_max_fee_per_gas_formatted_transfer = format!("{:#?}", l1_max_fee_per_gas_transfer);
        println!(
            "L1 max fee per gas: {}",
            l1_max_fee_per_gas_formatted_transfer.cyan()
        );
        println!();
    }
}
