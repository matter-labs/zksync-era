use std::marker::PhantomData;

#[derive(Debug)]
pub(super) struct AllocationGuardImpl {
    // Imitate the real guard.
    _not_send: PhantomData<*mut ()>,
}

impl AllocationGuardImpl {
    pub(super) fn new(_operation: &'static str) -> Self {
        Self {
            _not_send: PhantomData,
        }
    }
}
