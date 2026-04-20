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
