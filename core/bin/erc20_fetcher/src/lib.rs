use jsonrpsee::core::{async_trait, RpcResult};
pub use rpc::RpcApiServer;
use serde::{Deserialize, Serialize};
mod rpc;

pub struct RpcApiBackend {
    erc20_symbol: String,
    erc20_name: String,
}

impl RpcApiBackend {
    pub fn new(erc20_symbol: String, erc20_name: String) -> Self {
        Self {
            erc20_symbol,
            erc20_name,
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
struct EthValue {
    eth: serde_json::value::Number,
}
#[derive(Deserialize, Serialize, Debug)]
struct Request {
    dai: EthValue,
}

#[async_trait]
impl RpcApiServer for RpcApiBackend {
    async fn conversion_rate(&self) -> RpcResult<String> {
        let url =
        "https://api.coingecko.com/api/v3/simple/price?x_cg_demo_api_key=CG-FEgodj8AJN55Va4c6uKPUWLe&ids=dai&vs_currencies=eth";
        let response = reqwest::get(url)
            .await
            .expect("Failed request for ERC-20")
            .json::<Request>()
            .await
            .unwrap();
        RpcResult::Ok(response.dai.eth.to_string())
    }
    fn token_symbol(&self) -> RpcResult<String> {
        RpcResult::Ok(self.erc20_symbol.clone())
    }
    fn token_name(&self) -> RpcResult<String> {
        RpcResult::Ok(self.erc20_name.clone())
    }
    // async fn price(&self) -> RpcResult<String> {
    //     let url =
    //     "https://api.coingecko.com/api/v3/simple/price?x_cg_demo_api_key=CG-FEgodj8AJN55Va4c6uKPUWLe&ids=dai&vs_currencies=usd";
    //     let response = reqwest::get(url)
    //         .await
    //         .expect("Failed request for ERC-20")
    //         .json::<Request>()
    //         .await
    //         .unwrap();
    //     RpcResult::Ok(response.dai.eth.to_string())
    // }
}
