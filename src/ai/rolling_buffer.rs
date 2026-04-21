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

    /// Returns true if the buffer is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }
}

impl Default for RollingBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Validates: Requirements 17.1, 17.2
    #[test]
    fn push_evicts_oldest_when_over_capacity() {
        let buf = RollingBuffer::with_capacity(3);
        buf.push("a".into());
        buf.push("b".into());
        buf.push("c".into());
        buf.push("d".into());

        let snap = buf.snapshot();
        assert_eq!(snap, vec!["b", "c", "d"]);
    }

    /// Validates: Requirements 17.2, 17.3
    #[test]
    fn snapshot_returns_independent_copy() {
        let buf = RollingBuffer::with_capacity(5);
        buf.push("first".into());
        buf.push("second".into());

        let snap = buf.snapshot();
        // Mutate the buffer after snapshot
        buf.push("third".into());

        // Snapshot should be unchanged
        assert_eq!(snap, vec!["first", "second"]);
        // New snapshot reflects the mutation
        assert_eq!(buf.snapshot(), vec!["first", "second", "third"]);
    }

    /// Validates: Requirements 17.1, 17.3
    #[test]
    fn empty_buffer_snapshot_returns_empty_vec() {
        let buf = RollingBuffer::new();
        let snap = buf.snapshot();
        assert!(snap.is_empty());
    }
}
