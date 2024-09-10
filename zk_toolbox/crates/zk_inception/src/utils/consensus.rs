use anyhow::Context;
use url::Url;
use zksync_config::ApiConfig;

use crate::messages::MSG_PUBLIC_ADDR_ERR;

pub fn parse_public_addr(api_config: &ApiConfig) -> anyhow::Result<String> {
    let public_addr = api_config.web3_json_rpc.http_url.clone();
    let public_addr_url = Url::parse(&public_addr)?;
    let public_addr_ip = public_addr_url.host_str().context(MSG_PUBLIC_ADDR_ERR)?;
    let public_addr_port = public_addr_url.port().context(MSG_PUBLIC_ADDR_ERR)?;
    Ok(format!("{}:{}", public_addr_ip, public_addr_port))
}
