use super::Rollback;

#[derive(Debug, Default)]
pub(crate) struct Snapshotted<T> {
    pub(crate) value: T,
    old_values: Vec<T>,
}

impl<T: Copy> Rollback for Snapshotted<T> {
    fn snapshot(&mut self) {
        self.old_values.push(self.value);
    }

    fn rollback(&mut self) {
        self.value = self.old_values.pop().unwrap()
    }

    fn forget_snapshot(&mut self) {
        self.old_values.pop().unwrap();
    }
}
