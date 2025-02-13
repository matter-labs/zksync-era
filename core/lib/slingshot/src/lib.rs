mod gateway_client;

use alloy::transports::http::Client;
use zksync_basic_types::ethabi::Bytes;
use zksync_basic_types::{Address, L1BatchNumber, H256};

struct SlingshotFetcher<Database, EventFetcher> {
    database: Database,
    event_fetcher: EventFetcher,
}

impl<Database, EventFetcher> SlingshotFetcher<Database, EventFetcher> {
    fn run(self) {
        let last_event = self.database.get_last_executed_event();
        loop {
            let events = self.event_fetcher.next_events(last_event.miniblock);
            for event in events {
                self.database.store_event(event);
            }
        }
    }
}

struct SlingshotSender<Database> {
    database: Database,
    chain_client: Client,
}

impl<Database> SlingshotSender<Database> {
    fn run(self) {
        let chain_id = self.chain_client.chain_id();
        loop {
            let next_event = self.database.next_event(chain_id);
            // unlimited retries
            if self.chain_client.tx_executed(next_event.tx) {
                continue;
            }

            self.chain_client.send_tx(next_event);
            self.database.mark_tx_as_executed(next_event);
        }
    }
}

struct Event {
    output: Bytes,
    raw_data: Bytes,
    l1_batch_number: L1BatchNumber,
    l2_tx_number_in_block: L1BatchNumber,
    l2_message_index: L1BatchNumber,
    full_proof: Bytes,
}

trait SlingshotDatabase {
    fn get_last_executed_event(&self) -> Result<Option<Event>, ()>;
    fn next_event(&mut self, chain_id: u32) -> Result<Option<Event>, ()>;
    fn save_event(&mut self, event: Event) -> Result<(), ()>;
    fn mark_tx_as_executed(&mut self, event: Event) -> Result<(), ()>;
}

trait SlingshotClient {
    fn fetch_events(&self, from_miniblock: Option<u32>) -> Result<Vec<Event>, ()>;
}

trait DestinationChainClient {
    fn tx_executed(&self, tx_hash: H256) -> Result<bool, ()>;
    fn send_tx(&self, event: Event) -> Result<(), ()>;
}
