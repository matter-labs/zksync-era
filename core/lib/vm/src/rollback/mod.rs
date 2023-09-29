pub(crate) mod history_recorder;
pub(crate) mod reversable_log;
pub(crate) mod snapshotted;

/// Trait for anything within the VM that can be snapshotted and rolled back.
pub(crate) trait Rollback {
    fn snapshot(&mut self);
    fn rollback(&mut self);
    fn forget_snapshot(&mut self);
}

/// Helper for implementing Rollback on a struct that contains multiple things
/// implementing Rollback.
#[macro_export]
macro_rules! implement_rollback {
    {$($member:ident),+} => {
        fn snapshot(&mut self) {
            $(self.$member.snapshot();)+
        }
        fn rollback(&mut self) {
            $(self.$member.rollback();)+
        }
        fn forget_snapshot(&mut self) {
            $(self.$member.forget_snapshot();)+
        }
    };
}

/// Helper for rolling back when the sequencer orders it (external snapshot)
/// or when a call fails (internal snapshot).
///
/// As long as there are any internal snapshots, external snapshots cannot be made. This is
/// ok, as external snapshots are only made when execution is in the bootloader and
/// during the bootloader, no internal snapshots are made because when the bootloader fails
/// an external snapshot is rolled back anyway.
#[derive(Debug)]
pub(crate) struct LayeredRollback<T> {
    pub(crate) inner: T,
    callstack_height: snapshotted::Snapshotted<usize>,
}

/// The bootloader currently pushes two frames before executing transactions.
/// The bootloader frame and one near call.
const BOOTLOADER_FRAMES: usize = 2;

impl<T: Rollback> LayeredRollback<T> {
    pub(crate) fn new(inner: T) -> Self {
        Self {
            inner,
            callstack_height: Default::default(),
        }
    }

    pub(crate) fn internal_snapshot(&mut self) {
        // The first two calls are part of the bootloader
        if self.callstack_height.value >= BOOTLOADER_FRAMES {
            self.inner.snapshot();
        }
        self.callstack_height.value += 1;
    }

    pub(crate) fn internal_rollback(&mut self) {
        if self.callstack_height.value > BOOTLOADER_FRAMES {
            self.inner.rollback();
        }
        self.callstack_height.value -= 1;
    }

    pub(crate) fn internal_forget_snapshot(&mut self) {
        if self.callstack_height.value > BOOTLOADER_FRAMES {
            self.inner.forget_snapshot();
        }
        self.callstack_height.value -= 1;
    }
}

impl<T: Rollback> Rollback for LayeredRollback<T> {
    fn snapshot(&mut self) {
        assert!(
            self.callstack_height.value <= BOOTLOADER_FRAMES,
            "Snapshots must be made in the bootloader"
        );
        self.inner.snapshot();
        self.callstack_height.snapshot();
    }

    fn rollback(&mut self) {
        assert!(
            self.callstack_height.value <= BOOTLOADER_FRAMES,
            "Cannot access history while a transaction is running"
        );
        self.inner.rollback();
        self.callstack_height.rollback()
    }

    fn forget_snapshot(&mut self) {
        assert!(
            self.callstack_height.value <= BOOTLOADER_FRAMES,
            "Cannot access history while a transaction is running"
        );
        self.inner.forget_snapshot();
        self.callstack_height.forget_snapshot();
    }
}
