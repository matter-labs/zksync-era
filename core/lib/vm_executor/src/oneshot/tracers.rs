use std::{sync::Arc, thread};

use once_cell::sync::OnceCell;
use zksync_multivm::{
    interface::{storage::WriteStorage, tracer::ViolatedValidationRule, Call, OneshotTracers},
    tracers::{CallTracer, ValidationTracer},
    vm_latest::HistoryDisabled,
    MultiVMTracer, MultiVmTracerPointer,
};
use zksync_types::ProtocolVersionId;

/// Custom tracers supported by the API sandbox.
#[derive(Debug)]
pub(super) struct TracersAdapter<'a> {
    inner: OneshotTracers<'a>,
    calls_cell: Arc<OnceCell<Vec<Call>>>,
    validation_cell: Arc<OnceCell<ViolatedValidationRule>>,
}

impl<'a> TracersAdapter<'a> {
    pub fn new(inner: OneshotTracers<'a>) -> Self {
        Self {
            inner,
            calls_cell: Arc::default(),
            validation_cell: Arc::default(),
        }
    }

    pub fn to_boxed<S>(
        &self,
        protocol_version: ProtocolVersionId,
    ) -> Vec<MultiVmTracerPointer<S, HistoryDisabled>>
    where
        S: WriteStorage,
    {
        match self.inner {
            OneshotTracers::None => vec![],
            OneshotTracers::Calls(_) => {
                vec![CallTracer::new(self.calls_cell.clone()).into_tracer_pointer()]
            }
            OneshotTracers::Validation { params, .. } => {
                let (mut tracer, _) = ValidationTracer::<HistoryDisabled>::new(
                    params.clone(),
                    protocol_version.into(),
                );
                tracer.result = self.validation_cell.clone();
                vec![tracer.into_tracer_pointer()]
            }
        }
    }
}

impl Drop for TracersAdapter<'_> {
    fn drop(&mut self) {
        if thread::panicking() {
            return;
        }

        match &mut self.inner {
            OneshotTracers::None => { /* do nothing */ }
            OneshotTracers::Calls(calls) => {
                **calls = self.calls_cell.get().cloned().unwrap_or_default();
            }
            OneshotTracers::Validation { result, .. } => {
                **result = match self.validation_cell.get() {
                    Some(rule) => Err(rule.clone()),
                    None => Ok(()),
                };
            }
        }
    }
}
