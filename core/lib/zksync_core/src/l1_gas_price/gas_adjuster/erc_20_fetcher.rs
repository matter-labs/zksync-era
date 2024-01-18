use serde::Deserialize;
use serde::Serialize;
use zksync_eth_client::types::Error;
#[derive(Deserialize, Serialize, Debug)]
struct EthValue {
    eth: serde_json::value::Number,
}
#[derive(Deserialize, Serialize, Debug)]
struct Request {
    dai: EthValue,
}
/// TODO: This is for an easy refactor to test things,
/// and have a POC.
/// Let's discuss where this should actually be.
async fn fetch_it() -> Result<String, Error> {
    let url =
        "https://api.coingecko.com/api/v3/simple/price?x_cg_demo_api_key=CG-FEgodj8AJN55Va4c6uKPUWLe&ids=dai&vs_currencies=eth";
    let response = reqwest::get(url)
        .await
        .expect("Failed request for ERC-20")
        .json::<Request>()
        .await
        .unwrap();
    Ok(response.dai.eth.to_string())
}

fn erc20_value_from_eth_to_wei(value_in_eth: &str) -> Result<u64, String> {
    let splitted_value: Vec<&str> = value_in_eth.split(".").collect();
    let whole_part = u64::from_str_radix(
        splitted_value
            .first()
            .ok_or("Expected decimal value separated by coma")?,
        10,
    )
    .map_err(|_| "Expected decimal value separated by coma")?;
    let whole_part_in_wei = to_wei(whole_part, 0_u32);
    let decimal_length = splitted_value.last().unwrap().len() as u32;
    let decimal_part = u64::from_str_radix(
        splitted_value
            .last()
            .ok_or("Expected decimal value separated by coma")?,
        10,
    )
    .map_err(|_| "Expected decimal value separated by coma")?;
    let decimal_part_in_wei = to_wei(decimal_part, decimal_length);
    Ok(whole_part_in_wei + decimal_part_in_wei)
}

pub fn to_wei(in_eth: u64, modifier: u32) -> u64 {
    in_eth * 10_u64.pow(18_u32 - modifier)
}

pub async fn get_erc_20_value_in_wei() -> u64 {
    // let erc_20_value_in_eth = fetch_it().await.unwrap();
    // erc20_value_from_eth_to_wei(&erc_20_value_in_eth).unwrap()
    11
}
