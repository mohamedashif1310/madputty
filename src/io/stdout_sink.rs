//! `StdoutSink` manages the optional append-log file. Stdout writing is now
//! delegated to the colorizer, but the sink still owns the log file handle
//! and exposes it for raw-byte appending.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

use crate::errors::MadPuttyError;

pub struct StdoutSink {
    log: Option<File>,
}

impl StdoutSink {
    pub fn new() -> Self {
        Self { log: None }
    }

    pub fn with_log(path: &Path) -> Result<Self, MadPuttyError> {
        let file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .map_err(|e| MadPuttyError::LogFile {
                path: path.display().to_string(),
                source: e,
            })?;
        Ok(Self { log: Some(file) })
    }

    /// Mutable access to the log file handle, for writing raw bytes.
    pub fn log_mut(&mut self) -> Option<&mut File> {
        self.log.as_mut()
    }

    /// Flush the log file if present.
    #[allow(dead_code)]
    pub fn flush_log(&mut self) -> io::Result<()> {
        if let Some(f) = &mut self.log {
            use std::io::Write;
            f.flush()?;
        }
        Ok(())
    }
}

impl Default for StdoutSink {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use tempfile::tempdir;

    #[test]
    fn new_has_no_log_file() {
        let mut sink = StdoutSink::new();
        assert!(sink.log_mut().is_none());
    }

    #[test]
    fn default_has_no_log_file() {
        let mut sink = StdoutSink::default();
        assert!(sink.log_mut().is_none());
    }

    #[test]
    fn with_log_creates_file_if_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("session.log");
        assert!(!path.exists());
        let _sink = StdoutSink::with_log(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn with_log_appends_without_truncate() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("append.log");
        // Pre-seed with content
        std::fs::write(&path, b"existing\n").unwrap();

        let mut sink = StdoutSink::with_log(&path).unwrap();
        let f = sink.log_mut().unwrap();
        f.write_all(b"new line\n").unwrap();
        f.flush().unwrap();

        let mut contents = String::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        assert_eq!(contents, "existing\nnew line\n");
    }

    #[test]
    fn with_log_returns_error_on_invalid_path() {
        // Path in a nonexistent directory that we can't create
        let bad = std::path::PathBuf::from("/nonexistent/madputty/test/path/file.log");
        let result = StdoutSink::with_log(&bad);
        assert!(result.is_err());
        // Error should be MadPuttyError::LogFile variant
        match result {
            Err(crate::errors::MadPuttyError::LogFile { path, .. }) => {
                assert!(path.contains("madputty/test/path"));
            }
            Err(other) => panic!("expected LogFile error, got {:?}", other),
            Ok(_) => panic!("expected LogFile error, got Ok"),
        }
    }

    #[test]
    fn flush_log_when_none_is_ok() {
        let mut sink = StdoutSink::new();
        assert!(sink.flush_log().is_ok());
    }

    #[test]
    fn flush_log_when_present_succeeds() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("flush.log");
        let mut sink = StdoutSink::with_log(&path).unwrap();
        sink.log_mut().unwrap().write_all(b"data").unwrap();
        assert!(sink.flush_log().is_ok());
    }

    #[test]
    fn multiple_sinks_on_same_file_both_append() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("multi.log");

        {
            let mut s1 = StdoutSink::with_log(&path).unwrap();
            s1.log_mut().unwrap().write_all(b"first\n").unwrap();
        }
        {
            let mut s2 = StdoutSink::with_log(&path).unwrap();
            s2.log_mut().unwrap().write_all(b"second\n").unwrap();
        }

        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "first\nsecond\n");
    }
}
