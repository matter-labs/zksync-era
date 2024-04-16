use zksync_config::{ContractsConfig, EthConfig};
use zksync_env_config::FromEnv;
use zksync_eth_client::{clients::QueryClient, CallFunctionArgs, EthInterface};

async fn call_contract_function(
    query_client: &QueryClient,
    contract_name: &str,
    function_name: &str,
) -> Result<Uint256, Error> {
    let args: ContractCall = CallFunctionArgs::new(function_name, ())
        .for_contract(contracts_config.diamond_proxy_addr, contract_name.into());
    query_client.call_contract_function(args).await
}

pub(crate) async fn run() -> anyhow::Result<()> {
    let contracts_config = ContractsConfig::from_env()?;
    let eth_config = EthConfig::from_env()?;
    let query_client = QueryClient::new(&eth_config.web3_url).unwrap();

    let total_batches_committed = call_contract_function(
        &query_client,
        zksync_contracts::zksync_contract(),
        "getTotalBatchesCommitted",
    )
    .await?;

    let total_batches_verified = call_contract_function(
        &query_client,
        zksync_contracts::zksync_contract(),
        "getTotalBatchesVerified",
    )
    .await?;

    Ok(())
}

// async function getL1ValidatorStatus(): Promise<[number, number]> {
//     // Setup a provider
//     let provider = new ethers.providers.JsonRpcProvider(process.env.ETH_CLIENT_WEB3_URL);

//     // Create a contract instance
//     let contract = new ethers.Contract(process.env.CONTRACTS_DIAMOND_PROXY_ADDR!, GETTER_ABI, provider);

//     try {
//         const blocksCommitted = await contract.getTotalBatchesCommitted();
//         const blocksVerified = await contract.getTotalBatchesVerified();
//         return [Number(blocksCommitted), Number(blocksVerified)];
//     } catch (error) {
//         console.error(`Error calling L1 contract: ${error}`);
//         return [-1, -1];
//     }
// }
