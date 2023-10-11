use std::{collections::VecDeque, convert::TryFrom, str::FromStr, sync::Arc, time::Duration};

use once_cell::sync::OnceCell;
use rand::Rng;
use tokio::time::timeout;

use zksync::{signer::Signer, HttpClient, HttpClientBuilder, Wallet, ZksNamespaceClient};
use zksync_eth_signer::PrivateKeySigner;
use zksync_types::{tx::primitives::PackedEthSignature, Address, L2ChainId, H256};

use crate::{
    config::LoadtestConfig,
    corrupted_tx::CorruptedSigner,
    fs_utils::{loadnext_contract, TestContract},
    rng::{LoadtestRng, Random},
};

/// An alias to [`zksync::Wallet`] with HTTP client. Wrapped in `Arc` since
/// the client cannot be cloned due to limitations in jsonrpsee.
pub type SyncWallet = Arc<Wallet<PrivateKeySigner, HttpClient>>;
pub type CorruptedSyncWallet = Arc<Wallet<CorruptedSigner, HttpClient>>;

/// Thread-safe pool of the addresses of accounts used in the loadtest.
#[derive(Debug, Clone)]
pub struct AddressPool {
    addresses: Arc<Vec<Address>>,
}

impl AddressPool {
    pub fn new(addresses: Vec<Address>) -> Self {
        Self {
            addresses: Arc::new(addresses),
        }
    }

    /// Randomly chooses one of the addresses stored in the pool.
    pub fn random_address(&self, rng: &mut LoadtestRng) -> Address {
        let index = rng.gen_range(0..self.addresses.len());
        self.addresses[index]
    }
}

/// Credentials for a test account.
/// Currently we support only EOA accounts.
#[derive(Debug, Clone)]
pub struct AccountCredentials {
    /// Ethereum private key.
    pub eth_pk: H256,
    /// Ethereum address derived from the private key.
    pub address: Address,
}

impl Random for AccountCredentials {
    fn random(rng: &mut LoadtestRng) -> Self {
        let eth_pk = H256::random_using(rng);
        let address = pk_to_address(&eth_pk);

        Self { eth_pk, address }
    }
}

/// Type that contains the data required for the test wallet to operate.
#[derive(Debug, Clone)]
pub struct TestWallet {
    /// Pre-initialized wallet object.
    pub wallet: SyncWallet,
    /// Wallet with corrupted signer.
    pub corrupted_wallet: CorruptedSyncWallet,
    /// Contract bytecode and calldata to be used for sending `Execute` transactions.
    pub test_contract: TestContract,
    /// Address of the deployed contract to be used for sending
    /// `Execute` transaction.
    pub deployed_contract_address: Arc<OnceCell<Address>>,
    /// RNG object derived from a common loadtest seed and the wallet private key.
    pub rng: LoadtestRng,
}

/// Pool of accounts to be used in the test.
/// Each account is represented as `zksync::Wallet` in order to provide convenient interface of interation with zkSync.
#[derive(Debug)]
pub struct AccountPool {
    /// Main wallet that will be used to initialize all the test wallets.
    pub master_wallet: SyncWallet,
    /// Collection of test wallets and their Ethereum private keys.
    pub accounts: VecDeque<TestWallet>,
    /// Pool of addresses of the test accounts.
    pub addresses: AddressPool,
}

impl AccountPool {
    /// Generates all the required test accounts and prepares `Wallet` objects.
    pub async fn new(config: &LoadtestConfig) -> anyhow::Result<Self> {
        let l2_chain_id = L2ChainId::try_from(config.l2_chain_id).unwrap();
        // Create a client for pinging the rpc.
        let client = HttpClientBuilder::default()
            .build(&config.l2_rpc_address)
            .unwrap();
        // Perform a health check: check whether zkSync server is alive.
        let mut server_alive = false;
        for _ in 0usize..3 {
            if let Ok(Ok(_)) = timeout(Duration::from_secs(3), client.get_main_contract()).await {
                server_alive = true;
                break;
            }
        }
        if !server_alive {
            anyhow::bail!("zkSync server does not respond. Please check RPC address and whether server is launched");
        }

        let test_contract = loadnext_contract(&config.test_contracts_path)?;

        let master_wallet = {
            let eth_pk = H256::from_str(&config.master_wallet_pk)
                .expect("Can't parse master wallet private key");
            let eth_signer = PrivateKeySigner::new(eth_pk);
            let address = pk_to_address(&eth_pk);
            let signer = Signer::new(eth_signer, address, l2_chain_id);

            Arc::new(Wallet::with_http_client(&config.l2_rpc_address, signer).unwrap())
        };

        let mut rng = LoadtestRng::new_generic(config.seed.clone());
        tracing::info!("Using RNG with master seed: {}", rng.seed_hex());

        let group_size = config.accounts_group_size;
        let accounts_amount = config.accounts_amount;
        anyhow::ensure!(
            group_size <= accounts_amount,
            "Accounts group size is expected to be less than or equal to accounts amount"
        );

        let mut accounts = VecDeque::with_capacity(accounts_amount);
        let mut addresses = Vec::with_capacity(accounts_amount);

        for i in (0..accounts_amount).step_by(group_size) {
            let range_end = (i + group_size).min(accounts_amount);
            // The next group shares the contract address.
            let deployed_contract_address = Arc::new(OnceCell::new());

            for _ in i..range_end {
                let eth_credentials = AccountCredentials::random(&mut rng);
                let eth_signer = PrivateKeySigner::new(eth_credentials.eth_pk);
                let address = eth_credentials.address;
                let signer = Signer::new(eth_signer, address, l2_chain_id);

                let corrupted_eth_signer = CorruptedSigner::new(address);
                let corrupted_signer = Signer::new(corrupted_eth_signer, address, l2_chain_id);

                let wallet = Wallet::with_http_client(&config.l2_rpc_address, signer).unwrap();
                let corrupted_wallet =
                    Wallet::with_http_client(&config.l2_rpc_address, corrupted_signer).unwrap();

                addresses.push(wallet.address());
                let account = TestWallet {
                    wallet: Arc::new(wallet),
                    corrupted_wallet: Arc::new(corrupted_wallet),
                    test_contract: test_contract.clone(),
                    deployed_contract_address: deployed_contract_address.clone(),
                    rng: rng.derive(eth_credentials.eth_pk),
                };
                accounts.push_back(account);
            }
        }

        Ok(Self {
            master_wallet,
            accounts,
            addresses: AddressPool::new(addresses),
        })
    }
}

fn pk_to_address(eth_pk: &H256) -> Address {
    PackedEthSignature::address_from_private_key(eth_pk)
        .expect("Can't get an address from the private key")
}
