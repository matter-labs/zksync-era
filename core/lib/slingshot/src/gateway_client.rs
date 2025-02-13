use alloy::providers::Provider;
use alloy::transports::http::reqwest::Url;
use alloy::{providers::ProviderBuilder, sol};
use std::cmp::max;
use zksync_basic_types::Address;

struct SlingshotGatewayClient {
    gateway_url: Url,
    interop_center: Address,
}

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    InteropCenter,
    "../../../contracts/l1-contracts/zkout/InteropCenter.sol/InteropCenter.json"
);

impl SlingshotGatewayClient {
    async fn run(&mut self, from_block: u64) {
        // Create a provider.
        let provider = ProviderBuilder::new().on_http(self.gateway_url.clone());

        let center = InteropCenter::new(self.interop_center.into(), provider);

        let mut latest_block = from_block;
        loop {
            // Get all logs from the latest block that match the transfer event signature/topic.
            // You could also use the event name instead of the event signature like so:
            // .event("Transfer(address,address,uint256)")

            if let Ok(logs) = center
                .InteropBundleSent_filter()
                .from_block(latest_block)
                .query()
                .await
            {
                for (data, log) in logs {
                    latest_block = max(log.block_number.unwrap(), latest_block);
                }
            };

            if let Ok(logs) = center
                .InteropTriggerSent_filter()
                .from_block(latest_block)
                .query()
                .await
            {
                for (data, log) in logs {
                    latest_block = max(log.block_number.unwrap(), latest_block);
                }
            };
        }
    }
}
