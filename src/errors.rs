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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn port_not_found_maps_to_exit_code_2() {
        let err = MadPuttyError::PortNotFound {
            port: "COM99".to_string(),
        };
        assert_eq!(err.exit_code(), ExitCode::NotFound);
        assert_eq!(err.exit_code() as i32, 2);
    }

    #[test]
    fn port_busy_maps_to_exit_code_3() {
        let err = MadPuttyError::PortBusy {
            port: "COM3".to_string(),
        };
        assert_eq!(err.exit_code(), ExitCode::Busy);
        assert_eq!(err.exit_code() as i32, 3);
    }

    #[test]
    fn port_io_maps_to_exit_code_1() {
        let err = MadPuttyError::PortIo(io::Error::other("boom"));
        assert_eq!(err.exit_code(), ExitCode::General);
        assert_eq!(err.exit_code() as i32, 1);
    }

    #[test]
    fn log_file_maps_to_general() {
        let err = MadPuttyError::LogFile {
            path: "/tmp/x.log".to_string(),
            source: io::Error::new(io::ErrorKind::PermissionDenied, "denied"),
        };
        assert_eq!(err.exit_code(), ExitCode::General);
    }

    #[test]
    fn send_file_maps_to_general() {
        let err = MadPuttyError::SendFile {
            path: "/tmp/x.bin".to_string(),
            source: io::Error::new(io::ErrorKind::NotFound, "nope"),
        };
        assert_eq!(err.exit_code(), ExitCode::General);
    }

    #[test]
    fn invalid_arg_maps_to_general() {
        let err = MadPuttyError::InvalidArg("bad baud".to_string());
        assert_eq!(err.exit_code(), ExitCode::General);
    }

    #[test]
    fn ai_error_maps_to_general() {
        let err = MadPuttyError::AiError("kiro timeout".to_string());
        assert_eq!(err.exit_code(), ExitCode::General);
    }

    #[test]
    fn success_is_zero() {
        assert_eq!(ExitCode::Success as i32, 0);
    }

    #[test]
    fn display_contains_port_name() {
        let err = MadPuttyError::PortNotFound {
            port: "/dev/ttyUSB0".to_string(),
        };
        assert!(err.to_string().contains("/dev/ttyUSB0"));
    }

    #[test]
    fn display_contains_busy_context() {
        let err = MadPuttyError::PortBusy {
            port: "COM3".to_string(),
        };
        let s = err.to_string();
        assert!(s.contains("COM3"));
        assert!(s.to_lowercase().contains("use") || s.to_lowercase().contains("busy"));
    }
}
