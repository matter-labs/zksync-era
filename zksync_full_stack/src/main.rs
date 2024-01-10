use std::{str::FromStr, time::Duration};

use ethers::{
    abi::Abi,
    core::k256::WideBytes,
    providers::{Http, JsonRpcClient},
    utils::parse_units,
};
use tokio::time::sleep;
use zksync_web3_decl::{
    jsonrpsee::http_client::HttpClientBuilder,
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};
use zksync_web3_rs::{
    contracts::main_contract::L2CanonicalTransaction,
    providers::{Middleware, Provider},
    signers::{LocalWallet, Signer},
    zks_provider::{self, ZKSProvider},
    zks_wallet::{CallRequest, DeployRequest, DepositRequest},
    ZKSWallet,
};

use loadnext::{
    command::TxType,
    config::{ExecutionConfig, LoadtestConfig},
    executor::Executor,
    report_collector::LoadtestResult,
};

static ERA_PROVIDER_URL: &str = "http://127.0.0.1:3050";
static PRIVATE_KEY: &str = "7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110";

static CONTRACT_BIN: &str = include_str!("../BytesWriter.bin");
static CONTRACT_ABI: &str = include_str!("../BytesWriter.abi");

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

    println!("Deposit transaction hash: {:?}", deposit_transaction_hash);

    // Deploy contract:
    let contract_address = {
        // Read both files from disk:
        let abi = Abi::load(CONTRACT_ABI.as_bytes()).unwrap();
        let contract_bin = hex::decode(CONTRACT_BIN).unwrap().to_vec();

        // DeployRequest sets the parameters for the constructor call and the deployment transaction.
        let request = DeployRequest::with(abi, contract_bin, vec!["0x0016".to_owned()])
            .from(zk_wallet.l2_address());

        // Send the deployment transaction and wait until we receive the contract address.
        let address = zk_wallet.deploy(&request).await.unwrap();

        println!("Contract address: {:#?}", address);

        address
    };

    // Call the greet view method:
    {
        let era_provider = zk_wallet.get_era_provider().unwrap();
        let call_request = CallRequest::new(contract_address, "readBytes()(bytes)".to_owned());

        let bytes_message = ZKSProvider::call(era_provider.as_ref(), &call_request)
            .await
            .unwrap();

        println!("bytes message: {}", bytes_message[0]);
    }

    // Perform a signed transaction calling the setGreeting method
    let values = vec![1, 10, 100, 1000];

    for &value in &values {
        let hex_value = format!("{:0width$X}", value, width = 6);
        println!("Writing hex value: {}", hex_value);
        let receipt = zk_wallet
            .get_era_provider()
            .unwrap()
            .clone()
            .send_eip712(
                &zk_wallet.l2_wallet,
                contract_address,
                "writeBytes(bytes)",
                Some([hex_value].into()),
                None,
            )
            .await
            .unwrap()
            .await
            .unwrap()
            .unwrap();

        println!(
            "writeBytes transaction hash {:#?}",
            receipt.transaction_hash
        );

        let l2_transaction = {
            loop {
                println!("Getting l2 transaction details with rpc...");
                let l2_transaction = l2_rpc_client
                    .get_transaction_details(receipt.transaction_hash)
                    .await
                    .unwrap()
                    .unwrap();

                if l2_transaction.eth_commit_tx_hash.is_some() {
                    dbg!(l2_transaction.clone());
                    break l2_transaction.clone();
                }

                sleep(Duration::from_secs(1)).await; // Adjust the duration as needed
            }
        };

        let l1_transaction = l1_rpc_client
            .get_transaction_by_hash(l2_transaction.eth_commit_tx_hash.unwrap())
            .await
            .unwrap()
            .unwrap();

        dbg!(l1_transaction.clone());
    }

    {
        let era_provider = zk_wallet.get_era_provider().unwrap();
        let call_request = CallRequest::new(contract_address, "readBytes()(bytes)".to_owned());

        let bytes_message = ZKSProvider::call(era_provider.as_ref(), &call_request)
            .await
            .unwrap();

        println!("bytes message: {}", bytes_message[0]);
    }
}
