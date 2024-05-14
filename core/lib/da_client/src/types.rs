pub(crate) type Error = String;

#[derive(Default)]
pub struct DispatchResponse {
    pub(crate) blob_id: Vec<u8>,
}

#[derive(Default)]
pub struct InclusionData {
    data: Vec<u8>,
}
