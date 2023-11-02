use std::str::FromStr;
use ethers::providers::Http;
use ethers::utils::parse_units;
use zksync_web3_rs::providers::{Middleware, Provider};
use zksync_web3_rs::signers::{LocalWallet, Signer};
use zksync_web3_rs::ZKSWallet;
use zksync_web3_rs::zks_provider::ZKSProvider;
use zksync_web3_rs::zks_wallet::{DeployRequest, DepositRequest};
use zksync_web3_rs::zks_wallet::CallRequest;
use ethers::abi::Abi;


static ERA_PROVIDER_URL: &str = "http://127.0.0.1:3050";
static PRIVATE_KEY: &str = "7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110";

static CONTRACT_BIN: &str = include_str!("../Greeter.bin");
static CONTRACT_ABI: &str = include_str!("../Greeter.abi");

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
        ZKSWallet::new(l2_wallet, None, Some(era_provider.clone()),Some(l1_provider.clone())).unwrap()
    };

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
        let request = DeployRequest::with(abi, contract_bin, vec!["Hey".to_owned()])
            .from(zk_wallet.l2_address());

        // Send the deployment transaction and wait until we receive the contract address.
        let address = zk_wallet.deploy(&request).await.unwrap();

        println!("Contract address: {:#?}", address);

        address
    };

    // Call the greet view method:
    {
        let era_provider = zk_wallet.get_era_provider().unwrap();
        let call_request = CallRequest::new(contract_address, "greet()(string)".to_owned());

        let greet = ZKSProvider::call(era_provider.as_ref(), &call_request)
            .await
            .unwrap();

        println!("greet: {}", greet[0]);
    }

    // Perform a signed transaction calling the setGreeting method
    {
        let receipt = zk_wallet
            .get_era_provider()
            .unwrap()
            .clone()
            .send_eip712(
                &zk_wallet.l2_wallet,
                contract_address,
                "setGreeting(string)",
                Some(["Hello".into()].into()),
                None,
            )
            .await
            .unwrap()
            .await
            .unwrap()
            .unwrap();

        println!(
            "setGreeting transaction hash {:#?}",
            receipt.transaction_hash
        );
    };

    {
        let era_provider = zk_wallet.get_era_provider().unwrap();
        let call_request = CallRequest::new(contract_address, "greet()(string)".to_owned());
    
        let greet = ZKSProvider::call(era_provider.as_ref(), &call_request)
            .await
            .unwrap();
    
        println!("greet: {}", greet[0]);
    }
}
