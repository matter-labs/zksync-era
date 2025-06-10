use std::marker::PhantomData;

#[derive(Debug)]
pub(super) struct AllocationGuardImpl {
    operation: &'static str,
    // Imitate the real guard.
    _not_send: PhantomData<*mut ()>,
}

impl AllocationGuardImpl {
    pub(super) fn new(operation: &'static str) -> Self {
        Self {
            operation,
            _not_send: PhantomData,
        }
    }
}
