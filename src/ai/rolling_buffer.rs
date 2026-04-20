//! Rolling buffer of the most recent N log lines from the serial stream.
//!
//! Thread-safe via `Arc<Mutex<VecDeque>>`. The mutex is held only for
//! O(1) push and O(N) snapshot-clone, both sub-microsecond at N=50.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const DEFAULT_CAPACITY: usize = 50;

#[derive(Clone)]
pub struct RollingBuffer {
    inner: Arc<Mutex<VecDeque<String>>>,
    capacity: usize,
}

impl RollingBuffer {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
        }
    }

    /// Push a line. Evicts the oldest if over capacity.
    pub fn push(&self, line: String) {
        let mut buf = self.inner.lock().unwrap();
        if buf.len() >= self.capacity {
            buf.pop_front();
        }
        buf.push_back(line);
    }

    /// Take a point-in-time snapshot. Non-blocking beyond the brief mutex lock.
    pub fn snapshot(&self) -> Vec<String> {
        let buf = self.inner.lock().unwrap();
        buf.iter().cloned().collect()
    }

    /// Current number of lines in the buffer.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
}

impl Default for RollingBuffer {
    fn default() -> Self {
        Self::new()
    }
}
