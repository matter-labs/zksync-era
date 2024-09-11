use std::borrow::Cow;

// Temporary re-export of `sentry::capture_message` aiming to simplify the transition from `vlog` to using
// crates directly.
pub use sentry::{capture_message, Level as AlertLevel};
use sentry::{types::Dsn, ClientInitGuard};

#[derive(Debug)]
pub struct Sentry {
    url: Dsn,
    environment: Option<String>,
}

impl Sentry {
    pub fn new(url: &str) -> Result<Self, sentry::types::ParseDsnError> {
        Ok(Self {
            url: url.parse()?,
            environment: None,
        })
    }

    pub fn with_environment(mut self, environment: Option<String>) -> Self {
        self.environment = environment;
        self
    }

    pub fn install(self) -> ClientInitGuard {
        // Initialize the Sentry.
        let options = sentry::ClientOptions {
            release: sentry::release_name!(),
            environment: self.environment.map(Cow::from),
            attach_stacktrace: true,
            ..Default::default()
        };

        sentry::init((self.url, options))
    }
}
