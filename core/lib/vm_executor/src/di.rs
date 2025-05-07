use zksync_node_framework::Resource;

use crate::whitelist::SharedAllowList;

impl Resource for SharedAllowList {
    fn name() -> String {
        "shared_allow_list".to_string()
    }
}
