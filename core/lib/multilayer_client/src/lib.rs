use async_trait::async_trait;
use std::fmt::Debug;
use std::sync::{Arc, RwLock};

use zksync_basic_types::ethabi::Contract;
use zksync_basic_types::settlement::SettlementMode;
use zksync_basic_types::web3::{
    Block, BlockId, BlockNumber, Bytes, CallRequest, Filter, Log, Transaction, TransactionReceipt,
};
use zksync_basic_types::{Address, SLChainId, H160, H256, U256, U64};
use zksync_contracts::getters_facet_contract;
use zksync_eth_client::{BoundEthInterface, CallFunctionArgs, EthInterface, Options};
use zksync_eth_client::{
    ContractCallError, EnrichedClientResult, ExecutedTxStatus, FailureInfo, RawTransactionBytes,
    SignedCallResult, SigningError,
};

#[derive(Debug)]
struct Contracts {
    diamond_proxy_addr: Address,
}

#[derive(Debug)]
struct SLClient {
    client: Box<dyn BoundEthInterface>,
    contracts: Contracts,
}

#[derive(Debug)]
struct MultilayerSLClient {
    l1_client: SLClient,
    l2_client: Option<SLClient>,
    state: Arc<RwLock<SettlementMode>>,
}

impl MultilayerSLClient {
    // TODO replace with loading from l1
    async fn new_with_contracts(
        l1_client: (Box<dyn BoundEthInterface>, Contracts),
        l2_client: Option<(Box<dyn BoundEthInterface>, Contracts)>,
    ) -> (Self, Arc<RwLock<SettlementMode>>) {
        let l2_client = l2_client.map(|(client, contracts)| SLClient { client, contracts });
        let (client, contracts) = (l1_client.0, l1_client.1);
        let settlment_address = get_settlement_layer(
            client.as_ref(),
            contracts.diamond_proxy_addr,
            &getters_facet_contract(),
        )
        .await
        .unwrap();
        let settlment_mode = Arc::new(RwLock::new(settlement_mode_from_address(settlment_address)));
        (
            Self {
                l1_client: SLClient { client, contracts },
                l2_client,
                state: settlment_mode.clone(),
            },
            settlment_mode,
        )
    }

    fn client(&self) -> &SLClient {
        while let Ok(state) = self.state.read() {
            if state.is_gateway() {
                return self.l2_client.as_ref().unwrap();
            } else {
                return &self.l1_client;
            }
        }
        unreachable!();
    }

    fn l1_client(&self) -> &SLClient {
        &self.l1_client
    }
}

impl AsRef<dyn EthInterface> for MultilayerSLClient {
    fn as_ref(&self) -> &(dyn EthInterface + 'static) {
        (*self.client().client).as_ref()
    }
}

#[async_trait]
impl BoundEthInterface for MultilayerSLClient {
    fn clone_boxed(&self) -> Box<dyn BoundEthInterface> {
        self.client().client.clone_boxed()
    }

    fn for_component(self: Box<Self>, component_name: &'static str) -> Box<dyn BoundEthInterface> {
        // TODO check that clone is appropriate here (seems not)
        self.client().client.clone().for_component(component_name)
    }

    fn contract(&self) -> &Contract {
        self.client().client.contract()
    }

    fn contract_addr(&self) -> H160 {
        self.client().client.contract_addr()
    }

    fn chain_id(&self) -> SLChainId {
        self.client().client.chain_id()
    }

    fn sender_account(&self) -> Address {
        self.client().client.sender_account()
    }

    async fn allowance_on_account(
        &self,
        token_address: Address,
        address: Address,
        erc20_abi: &Contract,
    ) -> Result<U256, ContractCallError> {
        self.client()
            .client
            .allowance_on_account(token_address, address, erc20_abi)
            .await
    }

    async fn sign_prepared_tx_for_addr(
        &self,
        data: Vec<u8>,
        contract_addr: H160,
        options: Options,
    ) -> Result<SignedCallResult, SigningError> {
        self.client()
            .client
            .sign_prepared_tx_for_addr(data, contract_addr, options)
            .await
    }
}

#[async_trait]
impl EthInterface for MultilayerSLClient {
    async fn fetch_chain_id(&self) -> EnrichedClientResult<SLChainId> {
        (*self.client().client).as_ref().fetch_chain_id().await
    }

    async fn nonce_at_for_account(
        &self,
        account: Address,
        block: BlockNumber,
    ) -> EnrichedClientResult<U256> {
        (*self.client().client)
            .as_ref()
            .nonce_at_for_account(account, block)
            .await
    }

    async fn get_pending_block_base_fee_per_gas(&self) -> EnrichedClientResult<U256> {
        (*self.client().client)
            .as_ref()
            .get_pending_block_base_fee_per_gas()
            .await
    }

    async fn get_gas_price(&self) -> EnrichedClientResult<U256> {
        (*self.client().client).as_ref().get_gas_price().await
    }

    async fn block_number(&self) -> EnrichedClientResult<U64> {
        (*self.client().client).as_ref().block_number().await
    }

    async fn send_raw_tx(&self, tx: RawTransactionBytes) -> EnrichedClientResult<H256> {
        (*self.client().client).as_ref().send_raw_tx(tx).await
    }

    async fn get_tx_status(&self, hash: H256) -> EnrichedClientResult<Option<ExecutedTxStatus>> {
        (*self.client().client).as_ref().get_tx_status(hash).await
    }

    async fn failure_reason(&self, tx_hash: H256) -> EnrichedClientResult<Option<FailureInfo>> {
        (*self.client().client)
            .as_ref()
            .failure_reason(tx_hash)
            .await
    }

    async fn get_tx(&self, hash: H256) -> EnrichedClientResult<Option<Transaction>> {
        (*self.client().client).as_ref().get_tx(hash).await
    }

    async fn tx_receipt(&self, tx_hash: H256) -> EnrichedClientResult<Option<TransactionReceipt>> {
        (*self.client().client).as_ref().tx_receipt(tx_hash).await
    }

    async fn eth_balance(&self, address: Address) -> EnrichedClientResult<U256> {
        (*self.client().client).as_ref().eth_balance(address).await
    }

    async fn call_contract_function(
        &self,
        request: CallRequest,
        block: Option<BlockId>,
    ) -> EnrichedClientResult<Bytes> {
        (*self.client().client)
            .as_ref()
            .call_contract_function(request, block)
            .await
    }

    async fn logs(&self, filter: &Filter) -> EnrichedClientResult<Vec<Log>> {
        (*self.client().client).as_ref().logs(filter).await
    }

    async fn block(&self, block_id: BlockId) -> EnrichedClientResult<Option<Block<H256>>> {
        (*self.client().client).as_ref().block(block_id).await
    }
}

async fn get_settlement_layer(
    eth_client: &dyn BoundEthInterface,
    diamond_proxy_addr: Address,
    abi: &Contract,
) -> anyhow::Result<Address> {
    let settlement_layer: Address = CallFunctionArgs::new("getSettlementLayer", ())
        .for_contract(diamond_proxy_addr, &abi)
        .call((*eth_client).as_ref())
        .await?;

    Ok(settlement_layer)
}

fn settlement_mode_from_address(address: Address) -> SettlementMode {
    if address.is_zero() {
        SettlementMode::SettlesToL1
    } else {
        SettlementMode::Gateway
    }
}

async fn update_settlement_layer_loop(
    eth_client: Box<dyn BoundEthInterface>,
    diamond_proxy_addr: Address,
    settlement_mode: Arc<RwLock<SettlementMode>>,
) {
    let abi = getters_facet_contract();
    loop {
        let settlement_layer = get_settlement_layer(eth_client.as_ref(), diamond_proxy_addr, &abi)
            .await
            .unwrap();

        while let Ok(mut s) = settlement_mode.write() {
            *s = settlement_mode_from_address(settlement_layer);
            break;
        }
    }
}
