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

/// Create an ErrorScanner with a custom debounce duration (for testing).
#[cfg(test)]
impl ErrorScanner {
    fn with_debounce(debounce: Duration) -> Self {
        let mut scanner = Self::new();
        scanner.debounce = debounce;
        scanner
    }
}

impl Default for ErrorScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    // --- Pattern matching tests ---

    #[test]
    fn test_pattern_error() {
        let mut scanner = ErrorScanner::new();
        assert!(scanner.check("2024-01-01 ERROR: something went wrong"));
    }

    #[test]
    fn test_pattern_fail() {
        let mut scanner = ErrorScanner::new();
        assert!(scanner.check("test FAIL at line 42"));
    }

    #[test]
    fn test_pattern_failed() {
        let mut scanner = ErrorScanner::new();
        assert!(scanner.check("connection FAILED to establish"));
    }

    #[test]
    fn test_pattern_panic() {
        let mut scanner = ErrorScanner::new();
        assert!(scanner.check("kernel PANIC - not syncing"));
    }

    #[test]
    fn test_pattern_exception() {
        let mut scanner = ErrorScanner::new();
        assert!(scanner.check("unhandled EXCEPTION at 0xDEADBEEF"));
    }

    #[test]
    fn test_pattern_timeout() {
        let mut scanner = ErrorScanner::new();
        assert!(scanner.check("DHCP TIMEOUT after 10s"));
    }

    #[test]
    fn test_pattern_log_level_e() {
        let mut scanner = ErrorScanner::new();
        assert!(scanner.check("12:34:56 E wifi: connection dropped"));
    }

    #[test]
    fn test_no_match_on_normal_line() {
        let mut scanner = ErrorScanner::new();
        assert!(!scanner.check("INFO: system booted successfully"));
    }

    #[test]
    fn test_no_match_on_empty_line() {
        let mut scanner = ErrorScanner::new();
        assert!(!scanner.check(""));
    }

    // --- Debounce tests ---

    #[test]
    fn test_debounce_suppresses_rapid_fire() {
        let mut scanner = ErrorScanner::with_debounce(Duration::from_millis(100));

        // First match should trigger
        assert!(scanner.check("ERROR: first"));
        // Immediate second match should be suppressed
        assert!(!scanner.check("ERROR: second"));
        // Third immediate match also suppressed
        assert!(!scanner.check("PANIC: third"));
    }

    #[test]
    fn test_debounce_allows_after_elapsed() {
        let mut scanner = ErrorScanner::with_debounce(Duration::from_millis(10));

        // First match triggers
        assert!(scanner.check("ERROR: first"));
        // Wait past the debounce window
        thread::sleep(Duration::from_millis(20));
        // Should trigger again
        assert!(scanner.check("ERROR: after debounce"));
    }

    #[test]
    fn test_first_check_always_triggers_on_match() {
        let mut scanner = ErrorScanner::with_debounce(Duration::from_secs(30));

        // Very first match should always trigger regardless of debounce duration
        assert!(scanner.check("FAIL: first ever"));
    }

    #[test]
    fn test_non_matching_lines_dont_affect_debounce() {
        let mut scanner = ErrorScanner::with_debounce(Duration::from_millis(100));

        // First match triggers
        assert!(scanner.check("ERROR: first"));
        // Non-matching lines don't reset debounce
        assert!(!scanner.check("INFO: normal line"));
        assert!(!scanner.check("DEBUG: another normal line"));
        // Still within debounce window, so suppressed
        assert!(!scanner.check("ERROR: still debounced"));
    }
}
