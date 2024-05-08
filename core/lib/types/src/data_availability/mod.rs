type Error = String;

pub struct DADispatchResponse {
    tx_hash: Vec<u8>,
    inclusion_data: Vec<u8>,
}

pub trait DataAvailabilityClient {
    fn dispatch(&self, data: Vec<u8>) -> Result<DADispatchResponse, Error>;
}
