//! Append-only Markdown log of AI responses per session.
//!
//! Creates `~/.madputty/ai-responses/<session_id>.md` on first write.
//! Each entry has a timestamp header, trigger type, optional question, and response body.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Append-only per-session AI response log in Markdown format.
pub struct ResponseLog {
    path: PathBuf,
    has_entries: bool,
}

impl ResponseLog {
    /// Create a new response log targeting `~/.madputty/ai-responses/<session_id>.md`.
    ///
    /// The session ID format is `YYYYMMDD-HHMMSS-<port>`.
    pub fn new(session_id: &str) -> Self {
        let dir = home_dir().join(".madputty").join("ai-responses");
        let path = dir.join(format!("{session_id}.md"));
        Self {
            path,
            has_entries: false,
        }
    }

    /// Append a timestamped Markdown entry. Creates directory + file on first write.
    ///
    /// # Format
    ///
    /// ```markdown
    /// ## 2026-04-21 12:34:56 — Manual analysis
    /// Trigger: Ctrl+A A
    ///
    /// <response text>
    ///
    /// ---
    /// ```
    ///
    /// For custom questions, a `Question:` line is included after the trigger.
    pub fn append(
        &mut self,
        trigger: &str,
        question: Option<&str>,
        response: &str,
    ) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        let now = format_local_timestamp(SystemTime::now());
        let label = trigger_label(trigger);

        writeln!(file, "## {now} — {label}")?;
        writeln!(file, "Trigger: {trigger}")?;
        if let Some(q) = question {
            writeln!(file, "Question: {q}")?;
        }
        writeln!(file)?;
        writeln!(file, "{response}")?;
        writeln!(file)?;
        writeln!(file, "---")?;
        writeln!(file)?;

        self.has_entries = true;
        Ok(())
    }

    /// Returns true if at least one entry was written during this session.
    pub fn has_entries(&self) -> bool {
        self.has_entries
    }

    /// Returns the absolute path to the log file (for the exit message).
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Generate a session ID in the format `YYYYMMDD-HHMMSS-<port>`.
#[allow(dead_code)]
pub fn make_session_id(port: &str) -> String {
    let now = format_local_compact(SystemTime::now());
    let sanitized = sanitize_port_name(port);
    format!("{now}-{sanitized}")
}

/// Sanitize port name for use in filenames (replace path separators, colons, etc.).
#[allow(dead_code)]
fn sanitize_port_name(port: &str) -> String {
    port.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | ' ' => '_',
            c if c.is_ascii_alphanumeric() || c == '-' || c == '.' => c,
            _ => '_',
        })
        .collect()
}

/// Derive a human-readable trigger label from the trigger key combo.
fn trigger_label(trigger: &str) -> &str {
    match trigger {
        "Ctrl+A A" => "Manual analysis",
        "Ctrl+A Q" => "Custom question",
        _ if trigger.contains("auto") || trigger.contains("Auto") => "Auto-watch",
        _ => trigger,
    }
}

/// Resolve the user's home directory.
fn home_dir() -> PathBuf {
    // Prefer HOME (Unix) or USERPROFILE (Windows)
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Format a `SystemTime` as `YYYY-MM-DD HH:MM:SS` in UTC.
///
/// We avoid pulling in `chrono` to satisfy the no-new-deps constraint (Requirement 18.2).
fn format_local_timestamp(time: SystemTime) -> String {
    let secs = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let (year, month, day, hour, min, sec) = secs_to_datetime(secs);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{min:02}:{sec:02}")
}

/// Format a `SystemTime` as `YYYYMMDD-HHMMSS` for session IDs.
#[allow(dead_code)]
fn format_local_compact(time: SystemTime) -> String {
    let secs = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let (year, month, day, hour, min, sec) = secs_to_datetime(secs);
    format!("{year:04}{month:02}{day:02}-{hour:02}{min:02}{sec:02}")
}

/// Convert Unix timestamp to (year, month, day, hour, minute, second) in UTC.
fn secs_to_datetime(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let sec = secs % 60;
    let min = (secs / 60) % 60;
    let hour = (secs / 3600) % 24;

    // Days since epoch
    let mut days = secs / 86400;

    // Compute year
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    // Compute month and day
    let leap = is_leap_year(year);
    let month_days: [u64; 12] = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 0u64;
    for (i, &md) in month_days.iter().enumerate() {
        if days < md {
            month = i as u64 + 1;
            break;
        }
        days -= md;
    }
    let day = days + 1;

    (year, month, day, hour, min, sec)
}

fn is_leap_year(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn test_format_local_timestamp_epoch() {
        let time = UNIX_EPOCH;
        assert_eq!(format_local_timestamp(time), "1970-01-01 00:00:00");
    }

    #[test]
    fn test_format_local_timestamp_known_date() {
        // 2024-01-01 00:00:00 UTC = 1704067200 seconds since epoch
        let secs = 1704067200u64;
        let time = UNIX_EPOCH + Duration::from_secs(secs);
        assert_eq!(format_local_timestamp(time), "2024-01-01 00:00:00");
    }

    #[test]
    fn test_format_local_compact() {
        // 2024-01-01 00:00:00 UTC = 1704067200 seconds since epoch
        let secs = 1704067200u64;
        let time = UNIX_EPOCH + Duration::from_secs(secs);
        assert_eq!(format_local_compact(time), "20240101-000000");
    }

    #[test]
    fn test_make_session_id() {
        let id = make_session_id("COM3");
        // Should end with -COM3
        assert!(id.ends_with("-COM3"), "got: {id}");
        // Should start with YYYYMMDD-HHMMSS pattern (14 chars + dash)
        assert!(id.len() > 16);
    }

    #[test]
    fn test_sanitize_port_name() {
        assert_eq!(sanitize_port_name("COM3"), "COM3");
        assert_eq!(sanitize_port_name("/dev/ttyUSB0"), "_dev_ttyUSB0");
        assert_eq!(sanitize_port_name("COM 3"), "COM_3");
    }

    #[test]
    fn test_trigger_label() {
        assert_eq!(trigger_label("Ctrl+A A"), "Manual analysis");
        assert_eq!(trigger_label("Ctrl+A Q"), "Custom question");
        assert_eq!(trigger_label("auto-watch"), "Auto-watch");
        assert_eq!(trigger_label("something"), "something");
    }

    #[test]
    fn test_response_log_new() {
        let log = ResponseLog::new("20260421-123456-COM3");
        assert!(!log.has_entries());
        assert!(log
            .path()
            .to_str()
            .unwrap()
            .contains("20260421-123456-COM3.md"));
        assert!(log.path().to_str().unwrap().contains("ai-responses"));
    }

    #[test]
    fn test_response_log_append_manual() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test-session.md");

        let mut log = ResponseLog {
            path: file_path.clone(),
            has_entries: false,
        };

        log.append(
            "Ctrl+A A",
            None,
            "The device is rebooting due to a watchdog timeout.",
        )
        .unwrap();

        assert!(log.has_entries());
        let content = std::fs::read_to_string(&file_path).unwrap();

        // Check structure
        assert!(content.contains("## "));
        assert!(content.contains(" — Manual analysis"));
        assert!(content.contains("Trigger: Ctrl+A A"));
        assert!(!content.contains("Question:"));
        assert!(content.contains("The device is rebooting due to a watchdog timeout."));
        assert!(content.contains("---"));
    }

    #[test]
    fn test_response_log_append_custom_question() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test-session.md");

        let mut log = ResponseLog {
            path: file_path.clone(),
            has_entries: false,
        };

        log.append(
            "Ctrl+A Q",
            Some("why is WiFi failing?"),
            "The WiFi is failing because the SSID is not found.",
        )
        .unwrap();

        assert!(log.has_entries());
        let content = std::fs::read_to_string(&file_path).unwrap();

        assert!(content.contains(" — Custom question"));
        assert!(content.contains("Trigger: Ctrl+A Q"));
        assert!(content.contains("Question: why is WiFi failing?"));
        assert!(content.contains("The WiFi is failing because the SSID is not found."));
        assert!(content.contains("---"));
    }

    #[test]
    fn test_response_log_creates_directory() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("deep").join("nested").join("dir");
        let file_path = nested.join("session.md");

        let mut log = ResponseLog {
            path: file_path.clone(),
            has_entries: false,
        };

        log.append("Ctrl+A A", None, "test response").unwrap();
        assert!(file_path.exists());
    }

    #[test]
    fn test_response_log_multiple_appends() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("multi.md");

        let mut log = ResponseLog {
            path: file_path.clone(),
            has_entries: false,
        };

        log.append("Ctrl+A A", None, "First response").unwrap();
        log.append("Ctrl+A Q", Some("what happened?"), "Second response")
            .unwrap();

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("First response"));
        assert!(content.contains("Second response"));
        // Two separator lines
        assert_eq!(content.matches("---").count(), 2);
    }
}
