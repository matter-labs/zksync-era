use std::time::Duration;

// TODO: Add docs
#[derive(Debug, Clone)]
pub struct Backoff {
    base_delay: Duration,
    current_delay: Duration,
    max_delay: Duration,
}

impl Backoff {
    pub fn new(base_delay: Duration, max_delay: Duration) -> Self {
        Backoff {
            base_delay,
            current_delay: base_delay,
            max_delay,
        }
    }

    // TODO: Let's add some jitter as well

    // Determine the next delay (exponential backoff with jitter)
    pub fn delay(&mut self) -> Duration {
        let delay = self.current_delay * 2;
        delay.min(self.max_delay)
        // delay = delay *
        // Calculate exponential backoff with jitter
        // let exp_delay = self.base_delay * (1 << (self.attempt - 1));
        // let jitter: u64 = rand::thread_rng().gen_range(0..100);
        // let delay = exp_delay + Duration::from_millis(jitter);

        // Cap the delay at max_delay
        // Some(delay.min(self.max_delay))
    }

    // Reset the backoff for reuse
    pub fn reset(&mut self) {
        self.current_delay = self.base_delay;
    }
}

// TODO -- add tests
