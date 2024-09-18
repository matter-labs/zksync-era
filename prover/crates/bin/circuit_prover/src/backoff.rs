use std::time::Duration;

// TODO: Add docs
#[derive(Debug, Clone)]
pub struct Backoff {
    base_delay: Duration,
    current_delay: Duration,
    max_delay: Duration,
}

impl Backoff {
    /// Create a backoff with base_delay (first delay to start from) and max_delay (maximum delay possible).
    pub fn new(base_delay: Duration, max_delay: Duration) -> Self {
        Backoff {
            base_delay,
            current_delay: base_delay,
            max_delay,
        }
    }

    /// Get current delay
    pub fn delay(&mut self) -> Duration {
        let delay = self.current_delay;
        self.current_delay *= 2;
        self.current_delay = self.current_delay.min(self.max_delay);
        delay
    }

    /// Reset the backoff for to initial value
    pub fn reset(&mut self) {
        self.current_delay = self.base_delay;
    }
}
