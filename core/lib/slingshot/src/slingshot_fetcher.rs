use std::cmp::max;

use crate::contracts::InteropCenter;
use alloy::{providers::ProviderBuilder, transports::http::reqwest::Url};
use zksync_basic_types::Address;

struct SlingshotFetcherClient {
    gateway_url: Url,
    interop_center: Address,
}

pub struct InteropMessageParsed {
    pub interop_center_sender: Address,
    // The unique global identifier of this message.
    pub msg_hash: FixedBytes<32>,
    // The address that sent this message on the source chain.
    pub sender: Address,

    // 'data' field from the Log (it contains the InteropMessage).
    pub data: Bytes,

    pub interop_message: InteropCenter::InteropMessage,
    pub chain_id: u64,
}

impl SlingshotFetcherClient {
    async fn run(&mut self, from_block: u64) {
        // Create a provider.
        let provider = ProviderBuilder::new().on_http(self.gateway_url.clone());

        let center = InteropCenter::new(self.interop_center.into(), provider);

        let mut latest_block = from_block;
        loop {
            if let Ok(logs) = center
                .InteropBundleSent_filter()
                .from_block(latest_block)
                .query()
                .await
            {
                for (data, log) in logs {
                    latest_block = max(log.block_number.unwrap(), latest_block);
                    // let msg = InteropMessageParsed::from_log(&log, chain_id);
                    //
                    // println!("Got msg {:?}", msg);
                    //
                    // handle_type_a_message(&msg, &providers_map).await;
                    //
                    // if msg.is_type_c() {
                    //     handle_type_c_message(&msg, &providers_map, shared_map.clone())
                    //         .await;
                    // }
                    // let mut map = shared_map.lock().await;
                    //
                    // map.insert(msg.msg_hash, msg);

                }
            };

        //     if let Ok(logs) = center
        //         .InteropTriggerSent_filter()
        //         .from_block(latest_block)
        //         .query()
        //         .await
        //     {
        //         for (data, log) in logs {
        //             latest_block = max(log.block_number.unwrap(), latest_block);
        //         }
        //     };
        // }
    }
}
