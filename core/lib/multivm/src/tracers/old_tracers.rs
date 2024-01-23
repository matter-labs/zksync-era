use std::sync::Arc;

use once_cell::sync::OnceCell;
use zksync_types::vm_trace::Call;

#[derive(Debug, Clone)]
pub enum OldTracers {
    CallTracer(Arc<OnceCell<Vec<Call>>>),
    StorageInvocations(usize),
    None,
}

impl OldTracers {
    pub fn call_tracer(&self) -> Option<Arc<OnceCell<Vec<Call>>>> {
        match self {
            OldTracers::CallTracer(a) => Some(a.clone()),
            _ => None,
        }
    }
    pub fn storage_invocations(&self) -> Option<usize> {
        match self {
            OldTracers::StorageInvocations(a) => Some(*a),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct TracerDispatcher {
    pub(crate) call_tracer: Option<Arc<OnceCell<Vec<Call>>>>,
    pub(crate) storage_invocations: Option<usize>,
}

impl TracerDispatcher {
    pub fn new(tracers: Vec<OldTracers>) -> Self {
        let call_tracer = tracers.iter().find_map(|x| x.call_tracer());
        let storage_invocations = tracers.iter().find_map(|x| x.storage_invocations());

        Self {
            call_tracer,
            storage_invocations,
        }
    }
}
