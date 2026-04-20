//! Error type and exit-code contract for madputty.
//!
//! The binary maps each error variant to one of four exit codes (see
//! `ExitCode`), matching Requirement 8 of the serial-terminal spec.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum MadPuttyError {
    #[error("COM port not found: {port}")]
    PortNotFound { port: String },

    #[error("COM port in use by another process: {port}")]
    PortBusy { port: String },

    #[error("Serial port I/O error: {0}")]
    PortIo(#[from] io::Error),

    #[error("Serial port error: {0}")]
    Serial(#[from] serialport::Error),

    #[error("Log file error ({path}): {source}")]
    LogFile {
        path: String,
        #[source]
        source: io::Error,
    },

    #[error("Send file error ({path}): {source}")]
    SendFile {
        path: String,
        #[source]
        source: io::Error,
    },

    #[error("Invalid argument: {0}")]
    InvalidArg(String),

    #[error("AI error: {0}")]
    AiError(String),
}

/// Process exit codes used by the binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    Success = 0,
    General = 1,
    NotFound = 2,
    Busy = 3,
}

impl MadPuttyError {
    /// Map an error variant to its associated exit code.
    pub fn exit_code(&self) -> ExitCode {
        match self {
            MadPuttyError::PortNotFound { .. } => ExitCode::NotFound,
            MadPuttyError::PortBusy { .. } => ExitCode::Busy,
            _ => ExitCode::General,
        }
    }
}
