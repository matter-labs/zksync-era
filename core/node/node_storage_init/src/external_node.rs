use zksync_web3_decl::client::{DynClient, L2};

#[derive(Debug)]
pub struct ExternalNodeRole {
    pub client: Box<DynClient<L2>>,
}
