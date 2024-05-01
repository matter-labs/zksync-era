use zksync_web3_decl::client::BoxedL2Client;

use crate::resource::Resource;

#[derive(Debug, Clone)]
pub struct MainNodeClientResource(pub BoxedL2Client);

impl Resource for MainNodeClientResource {
    fn name() -> String {
        "external_node/main_node_client".into()
    }
}
