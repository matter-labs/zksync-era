use std::sync::Arc;

use zksync_node_framework::Resource;

use crate::{interface::TransactionFilter, whitelist::SharedAllowList};

impl Resource for SharedAllowList {
    fn name() -> String {
        "shared_allow_list".to_string()
    }
}

/// Transaction filter for API / sandboxed execution.
#[derive(Debug, Clone)]
pub struct ApiTransactionFilter(pub Arc<dyn TransactionFilter>);

impl Resource for ApiTransactionFilter {
    fn name() -> String {
        "api/transaction_filter".into()
    }
}
