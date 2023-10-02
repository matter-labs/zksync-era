// Built-in uses.
use std::{
    collections::HashMap,
    env,
    ffi::{OsStr, OsString},
    mem,
    sync::{Mutex, MutexGuard, PoisonError},
};
// Workspace uses
use zksync_basic_types::{Address, H256};

/// Mutex that allows to modify certain env variables and roll them back to initial values when
/// the corresponding [`EnvMutexGuard`] is dropped. This is useful for having multiple tests
/// that parse the same config from the environment.
#[derive(Debug)]
pub(crate) struct EnvMutex(Mutex<()>);

impl EnvMutex {
    /// Creates a new mutex. Separate mutexes can be used for changing env vars that do not intersect
    /// (e.g., env vars for different configs).
    pub const fn new() -> Self {
        Self(Mutex::new(()))
    }

    pub fn lock(&self) -> EnvMutexGuard<'_> {
        let guard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        EnvMutexGuard {
            _inner: guard,
            redefined_vars: HashMap::new(),
        }
    }
}

/// Guard provided by [`EnvMutex`] that allows mutating env variables. All changes are rolled back
/// when the guard is dropped.
#[must_use = "Environment will be reset when the guard is dropped"]
#[derive(Debug)]
pub(crate) struct EnvMutexGuard<'a> {
    _inner: MutexGuard<'a, ()>,
    redefined_vars: HashMap<OsString, Option<OsString>>,
}

impl Drop for EnvMutexGuard<'_> {
    fn drop(&mut self) {
        for (env_name, value) in mem::take(&mut self.redefined_vars) {
            if let Some(value) = value {
                env::set_var(env_name, value);
            } else {
                env::remove_var(env_name);
            }
        }
    }
}

impl EnvMutexGuard<'_> {
    /// Sets env vars specified in `.env`-like format.
    pub fn set_env(&mut self, fixture: &str) {
        for line in fixture.split('\n').map(str::trim) {
            if line.is_empty() {
                // Skip empty lines.
                continue;
            }

            let elements: Vec<_> = line.split('=').collect();
            assert_eq!(
                elements.len(),
                2,
                "Incorrect line for setting environment variable: {}",
                line
            );

            let variable_name: &OsStr = elements[0].as_ref();
            let variable_value: &OsStr = elements[1].trim_matches('"').as_ref();

            if !self.redefined_vars.contains_key(variable_name) {
                let prev_value = env::var_os(variable_name);
                self.redefined_vars
                    .insert(variable_name.to_os_string(), prev_value);
            }
            env::set_var(variable_name, variable_value);
        }
    }

    /// Removes the specified env vars.
    pub fn remove_env(&mut self, var_names: &[&str]) {
        for &var_name in var_names {
            let variable_name: &OsStr = var_name.as_ref();
            if !self.redefined_vars.contains_key(variable_name) {
                let prev_value = env::var_os(variable_name);
                self.redefined_vars
                    .insert(variable_name.to_os_string(), prev_value);
            }
            env::remove_var(variable_name);
        }
    }
}

/// Parses the address panicking upon deserialization failure.
pub fn addr(addr_str: &str) -> Address {
    addr_str.parse().expect("Incorrect address string")
}

/// Parses the H256 panicking upon deserialization failure.
pub fn hash(addr_str: &str) -> H256 {
    addr_str.parse().expect("Incorrect hash string")
}

#[test]
fn env_mutex_basics() {
    const TEST_VARIABLE_NAME: &str = "TEST_VARIABLE_THAT_WILL_CERTAINLY_NOT_BE_SET";
    const REDEFINED_VARIABLE_NAME: &str = "REDEFINED_VARIABLE_THAT_WILL_CERTAINLY_NOT_BE_SET";

    assert!(env::var_os(TEST_VARIABLE_NAME).is_none());
    assert!(env::var_os(REDEFINED_VARIABLE_NAME).is_none());
    env::set_var(REDEFINED_VARIABLE_NAME, "initial");

    let mutex = EnvMutex::new();
    let mut lock = mutex.lock();
    lock.set_env(&format!("{TEST_VARIABLE_NAME}=test"));
    assert!(lock.redefined_vars[OsStr::new(TEST_VARIABLE_NAME)].is_none());
    assert_eq!(env::var_os(TEST_VARIABLE_NAME).unwrap(), "test");
    lock.set_env(&format!("{REDEFINED_VARIABLE_NAME}=redefined"));
    assert_eq!(
        lock.redefined_vars[OsStr::new(REDEFINED_VARIABLE_NAME)]
            .as_ref()
            .unwrap(),
        "initial"
    );
    assert_eq!(env::var_os(REDEFINED_VARIABLE_NAME).unwrap(), "redefined");

    lock.remove_env(&[REDEFINED_VARIABLE_NAME]);
    assert!(env::var_os(REDEFINED_VARIABLE_NAME).is_none());
    assert_eq!(
        lock.redefined_vars[OsStr::new(REDEFINED_VARIABLE_NAME)]
            .as_ref()
            .unwrap(),
        "initial"
    );

    drop(lock);
    assert!(env::var_os(TEST_VARIABLE_NAME).is_none());
    assert_eq!(env::var_os(REDEFINED_VARIABLE_NAME).unwrap(), "initial");
}
