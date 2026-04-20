//! Error pattern scanner with 30-second debounce.
//!
//! Checks each log line against a set of error keywords. When a match is
//! found and the debounce window has elapsed, signals for AI analysis.

use regex::Regex;
use std::time::{Duration, Instant};

const DEBOUNCE_SECS: u64 = 30;

pub struct ErrorScanner {
    patterns: Vec<Regex>,
    last_trigger: Option<Instant>,
    debounce: Duration,
}

impl ErrorScanner {
    pub fn new() -> Self {
        let keywords = [
            r" E ",       // Log level marker with spaces
            r"ERROR",
            r"FAIL",
            r"FAILED",
            r"PANIC",
            r"EXCEPTION",
            r"TIMEOUT",
        ];
        let patterns = keywords
            .iter()
            .map(|k| Regex::new(k).unwrap())
            .collect();

        Self {
            patterns,
            last_trigger: None,
            debounce: Duration::from_secs(DEBOUNCE_SECS),
        }
    }

    /// Check a line. Returns true if an error pattern matched AND debounce allows.
    pub fn check(&mut self, line: &str) -> bool {
        let matched = self.patterns.iter().any(|p| p.is_match(line));
        if !matched {
            return false;
        }

        // Debounce: only trigger if enough time has passed
        if let Some(last) = self.last_trigger {
            if last.elapsed() < self.debounce {
                return false;
            }
        }

        self.last_trigger = Some(Instant::now());
        true
    }
}

impl Default for ErrorScanner {
    fn default() -> Self {
        Self::new()
    }
}
