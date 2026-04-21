//! Shell out to `kiro-cli chat --no-interactive` with a bounded timeout.
//!
//! Spawns kiro-cli as a child process via `tokio::process::Command`.
//! On timeout, kills the child and reaps it to avoid zombies.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

/// Errors from kiro-cli invocation. Non-fatal during sessions.
#[derive(Debug)]
pub enum AiError {
    Timeout(Duration),
    KiroError(String),
    SpawnFailed(String),
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiError::Timeout(d) => write!(f, "AI timed out after {}s", d.as_secs()),
            AiError::KiroError(msg) => write!(f, "kiro-cli error: {}", msg),
            AiError::SpawnFailed(msg) => write!(f, "failed to spawn kiro-cli: {}", msg),
        }
    }
}

#[derive(Clone)]
pub struct KiroInvoker {
    kiro_path: PathBuf,
    timeout_duration: Duration,
}

impl KiroInvoker {
    pub fn new(kiro_path: PathBuf, timeout_seconds: u32) -> Self {
        Self {
            kiro_path,
            timeout_duration: Duration::from_secs(timeout_seconds as u64),
        }
    }

    /// Invoke `kiro-cli chat --no-interactive "<prompt>"`.
    /// Returns the AI response text or an error.
    ///
    /// On timeout, kills the child process and reaps it to avoid zombies.
    /// On non-zero exit, returns the first line of stderr as the error message.
    /// Invoke `kiro-cli chat --no-interactive "<prompt>"`.
    /// Returns the AI response text or an error.
    ///
    /// On timeout, kills the child process and reaps it to avoid zombies.
    /// On non-zero exit, returns the first line of stderr as the error message.
    pub async fn invoke(&self, prompt: &str) -> Result<String, AiError> {
        let mut child = Command::new(&self.kiro_path)
            .args(["chat", "--no-interactive", prompt])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AiError::SpawnFailed(e.to_string()))?;

        // Take ownership of stdout/stderr handles so we can read them
        // while still retaining the child handle for kill on timeout.
        let mut stdout_handle = child.stdout.take().expect("stdout piped");
        let mut stderr_handle = child.stderr.take().expect("stderr piped");

        let wait_and_read = async {
            let mut stdout_buf = Vec::new();
            let mut stderr_buf = Vec::new();

            // Read stdout and stderr concurrently, then wait for exit.
            let (stdout_res, stderr_res, status) = tokio::join!(
                stdout_handle.read_to_end(&mut stdout_buf),
                stderr_handle.read_to_end(&mut stderr_buf),
                child.wait(),
            );

            stdout_res.map_err(|e| e.to_string())?;
            stderr_res.map_err(|e| e.to_string())?;
            let status = status.map_err(|e| e.to_string())?;

            Ok::<_, String>((status, stdout_buf, stderr_buf))
        };

        match timeout(self.timeout_duration, wait_and_read).await {
            Ok(Ok((status, stdout_buf, stderr_buf))) => {
                if status.success() {
                    Ok(String::from_utf8_lossy(&stdout_buf).to_string())
                } else {
                    let stderr = String::from_utf8_lossy(&stderr_buf);
                    let first_line = stderr.lines().next().unwrap_or("unknown error");
                    Err(AiError::KiroError(first_line.to_string()))
                }
            }
            Ok(Err(e)) => Err(AiError::SpawnFailed(e)),
            Err(_) => {
                // Timeout — kill the child and reap to avoid zombies.
                let _ = child.kill().await;
                let _ = child.wait().await;
                Err(AiError::Timeout(self.timeout_duration))
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Helper: create a .bat file that blocks for a long time (pings localhost).
    fn make_slow_bat() -> NamedTempFile {
        let mut f = NamedTempFile::with_suffix(".bat").unwrap();
        writeln!(f, "@echo off").unwrap();
        writeln!(f, "ping -n 31 127.0.0.1 > nul").unwrap();
        f.flush().unwrap();
        f
    }

    /// Helper: create a .bat file that writes to stderr and exits non-zero.
    fn make_failing_bat(stderr_msg: &str, exit_code: u8) -> NamedTempFile {
        let mut f = NamedTempFile::with_suffix(".bat").unwrap();
        writeln!(f, "@echo off").unwrap();
        writeln!(f, "echo {}>&2", stderr_msg).unwrap();
        writeln!(f, "exit /b {}", exit_code).unwrap();
        f.flush().unwrap();
        f
    }

    /// Test timeout handling: a slow process is killed after the timeout expires,
    /// and the child is reaped to avoid zombies. Uses the same timeout + kill + reap
    /// pattern as invoke().
    /// Validates: Requirements 11.4
    #[tokio::test]
    async fn test_timeout_kills_slow_process() {
        let bat = make_slow_bat();

        let mut child = Command::new(bat.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("should spawn .bat file");

        let mut stdout_handle = child.stdout.take().unwrap();
        let mut stderr_handle = child.stderr.take().unwrap();

        let wait_and_read = async {
            let mut stdout_buf = Vec::new();
            let mut stderr_buf = Vec::new();
            let (stdout_res, stderr_res, status) = tokio::join!(
                stdout_handle.read_to_end(&mut stdout_buf),
                stderr_handle.read_to_end(&mut stderr_buf),
                child.wait(),
            );
            stdout_res.map_err(|e| e.to_string())?;
            stderr_res.map_err(|e| e.to_string())?;
            let status = status.map_err(|e| e.to_string())?;
            Ok::<_, String>((status, stdout_buf, stderr_buf))
        };

        let timeout_duration = Duration::from_secs(1);
        let result = timeout(timeout_duration, wait_and_read).await;

        assert!(result.is_err(), "Expected timeout but process completed");

        // Kill and reap to avoid zombie (mirrors invoke() behavior)
        let _ = child.kill().await;
        let _ = child.wait().await;
    }

    /// Test non-zero exit code: the first line of stderr is extracted as the error.
    /// Validates: Requirements 11.6
    #[tokio::test]
    async fn test_nonzero_exit_extracts_stderr_first_line() {
        let bat = make_failing_bat("something went wrong", 1);

        let mut child = Command::new(bat.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("should spawn .bat file");

        let mut stdout_handle = child.stdout.take().unwrap();
        let mut stderr_handle = child.stderr.take().unwrap();

        let wait_and_read = async {
            let mut stdout_buf = Vec::new();
            let mut stderr_buf = Vec::new();
            let (stdout_res, stderr_res, status) = tokio::join!(
                stdout_handle.read_to_end(&mut stdout_buf),
                stderr_handle.read_to_end(&mut stderr_buf),
                child.wait(),
            );
            stdout_res.map_err(|e| e.to_string())?;
            stderr_res.map_err(|e| e.to_string())?;
            let status = status.map_err(|e| e.to_string())?;
            Ok::<_, String>((status, stdout_buf, stderr_buf))
        };

        let timeout_duration = Duration::from_secs(10);
        let result = timeout(timeout_duration, wait_and_read).await;

        assert!(result.is_ok(), "Process should complete before timeout");
        let (status, _stdout_buf, stderr_buf) = result.unwrap().unwrap();
        assert!(!status.success(), "Process should exit with non-zero code");

        let stderr = String::from_utf8_lossy(&stderr_buf);
        let first_line = stderr.lines().next().unwrap_or("unknown error");
        assert!(
            first_line.contains("something went wrong"),
            "Expected first line to contain 'something went wrong', got: '{}'",
            first_line
        );
    }

    /// Test invoke() returns AiError::Timeout for a slow process via the full
    /// invoke() method using a .bat file that blocks.
    /// Validates: Requirements 11.4
    #[tokio::test]
    async fn test_invoke_timeout() {
        let bat = make_slow_bat();

        let invoker = KiroInvoker::new(bat.path().to_path_buf(), 1);
        let result = invoker.invoke("test prompt").await;

        match result {
            Err(AiError::Timeout(d)) => {
                assert_eq!(d.as_secs(), 1);
            }
            other => panic!("Expected AiError::Timeout, got: {:?}", other),
        }
    }

    /// Test invoke() returns AiError::KiroError with stderr first line when
    /// the process exits non-zero.
    /// Validates: Requirements 11.6
    #[tokio::test]
    async fn test_invoke_nonzero_exit() {
        let bat = make_failing_bat("kiro-cli: authentication required", 1);

        let invoker = KiroInvoker::new(bat.path().to_path_buf(), 10);
        let result = invoker.invoke("test prompt").await;

        match result {
            Err(AiError::KiroError(msg)) => {
                assert!(
                    msg.contains("authentication required"),
                    "Expected stderr first line to contain 'authentication required', got: {}",
                    msg
                );
            }
            other => panic!("Expected AiError::KiroError, got: {:?}", other),
        }
    }

    /// Test invoke() returns AiError::SpawnFailed for a non-existent executable.
    #[tokio::test]
    async fn test_invoke_spawn_failed() {
        let invoker = KiroInvoker::new(
            PathBuf::from("C:\\nonexistent\\path\\to\\kiro-cli.exe"),
            10,
        );
        let result = invoker.invoke("test").await;

        match result {
            Err(AiError::SpawnFailed(_)) => {} // expected
            other => panic!("Expected AiError::SpawnFailed, got: {:?}", other),
        }
    }
}
