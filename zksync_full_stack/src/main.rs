use std::{
    fs::OpenOptions,
    io::{Error, Write},
    str::FromStr,
    time::Duration,
};

use colored::Colorize;
use ethers::{abi::Abi, providers::Http, utils::parse_units};
use loadnext::config::LoadtestConfig;
use tokio::time::sleep;
use zksync_web3_decl::{
    jsonrpsee::http_client::HttpClientBuilder,
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};
use zksync_web3_rs::{
    providers::{Middleware, Provider},
    signers::{LocalWallet, Signer},
    zks_provider::ZKSProvider,
    zks_wallet::{CallRequest, DeployRequest, DepositRequest},
    ZKSWallet,
};

static ERA_PROVIDER_URL: &str = "http://127.0.0.1:3050";
static PRIVATE_KEY: &str = "7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110";

static CONTRACT_BIN: &str = include_str!("../ERC20.bin");
static CONTRACT_ABI: &str = include_str!("../ERC20.abi");

static L1_URL: &str = "http://localhost:8545";

static REPORT_PATH: &str = "report.csv";

fn initialize_report() -> Result<(), Error> {
    let file = OpenOptions::new()
        .write(true)
        .truncate(true) // Trunca el archivo (borra su contenido)
        .create(true) // Crea el archivo si no existe
        .open(REPORT_PATH);

    match file {
        Ok(mut f) => writeln!(f, "operation,Value,l2_fee,l1_max_fee_per_gas"),
        Err(e) => Err(e),
    }
}

fn write_line_to_report(operation: &str, value: &str, l2_fee: &str, l1_max_fee_per_gas: &str) {
    let mut file = OpenOptions::new().append(true).open(REPORT_PATH).unwrap();

    writeln!(file, "{operation},{value},{l2_fee},{l1_max_fee_per_gas}").unwrap();
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    match initialize_report() {
        Ok(_) => println!("Report CSV created."),
        Err(e) => println!("Error initializing the CSV report: {}", e),
    }
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

        // Send the deployment transaction and wait until we receive the contract address.
        let address = zk_wallet.deploy(&request).await.unwrap();

        println!("Contract address: {:#?}", address);

        address
    };

    {
        let era_provider = zk_wallet.get_era_provider().unwrap();
        let call_request_name = CallRequest::new(contract_address, "name()(string)".to_owned());

        let name_message = ZKSProvider::call(era_provider.as_ref(), &call_request_name)
            .await
            .unwrap();

        println!("Token name: {}", name_message[0]);

        let call_request_symbol = CallRequest::new(contract_address, "symbol()(string)".to_owned());

        let symbol_message = ZKSProvider::call(era_provider.as_ref(), &call_request_symbol)
            .await
            .unwrap();

        println!("Token symbol: {}", symbol_message[0]);
    }

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
                    "bBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB".into(),
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
    println!("transaction hash {}", transaction_hash_formatted_mint);
    let l2_transaction_mint = {
        // println!("{}", "Getting l2 transaction details with rpc...");
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

    let l1_transaction_mint = l1_rpc_client
        .get_transaction_by_hash(l2_transaction_mint.eth_commit_tx_hash.unwrap())
        .await
        .unwrap()
        .unwrap();

    let l1_max_fee_per_gas_mint = l1_transaction_mint.max_fee_per_gas.unwrap();
    let l1_max_fee_per_gas_formatted_mint = format!("{:#?}", l1_max_fee_per_gas_mint);

    println!(
        "L1 max fee per gas: {}",
        l1_max_fee_per_gas_formatted_mint.cyan()
    );
    println!();

    write_line_to_report(
        "Mint",
        "nill",
        &l2_tx_fee_formatted_mint,
        &l1_max_fee_per_gas_formatted_mint,
    );

    let values: Vec<&str> = vec![
        "1000", "2000", "3000", "4000", "5000", "6000", "7000", "8000", "9000", "10000",
    ];

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
                        "bBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB".into(),
                        "CD2a3d9F938E13CD947Ec05AbC7FE734Df8DD826".into(),
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
        println!("transaction hash {}", transaction_hash_formatted_transfer);
        let l2_transaction_transfer = {
            // println!("{}", "Getting l2 transaction details with rpc...");
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

        write_line_to_report(
            "Transfer",
            &value,
            &l2_tx_fee_formatted_mint,
            &l1_max_fee_per_gas_formatted_mint,
        );
    }
}
