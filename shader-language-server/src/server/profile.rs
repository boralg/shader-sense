use std::time::Instant;

use log::debug;

#[macro_export]
macro_rules! profile_scope {
    ($($arg:tt)+) => {
        crate::server::profile::Profiler::new(format!($($arg)+))
    };
}

pub struct Profiler {
    start: Instant,
    message: String,
}
impl Profiler {
    pub fn new(message: String) -> Self {
        Self {
            start: Instant::now(),
            message: message,
        }
    }
}
impl Drop for Profiler {
    fn drop(&mut self) {
        debug!(
            "{} (duration {}ms)",
            self.message,
            self.start.elapsed().as_millis()
        );
    }
}
