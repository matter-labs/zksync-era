use zksync_da_client::DataAvailabilityInterface;

use crate::resource::Resource;

#[derive(Debug, Clone)]
pub struct DAInterfaceResource(pub Box<dyn DataAvailabilityInterface>);

impl Resource for DAInterfaceResource {
    fn name() -> String {
        "common/da_interface".into()
    }
}
