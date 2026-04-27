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

    /// Invoke `kiro-cli chat --no-interactive --trust-all-tools` with the prompt on stdin.
    /// Returns the AI response text or an error.
    ///
    /// On timeout, kills the child process and reaps it to avoid zombies.
    /// On non-zero exit, returns the first line of stderr as the error message.
    ///
    /// Passes the prompt via stdin rather than argv to work around Windows
    /// command-line length limits (~8191 chars on cmd.exe) — a 50-line log
    /// context plus system prompt can easily exceed that.
    ///
    /// Note: `--trust-all-tools` is required because headless mode has no
    /// user to approve tool invocations. If the installed kiro-cli doesn't
    /// recognize it, the caller will get a KiroError and can retry without it.
    pub async fn invoke(&self, prompt: &str) -> Result<String, AiError> {
        self.invoke_inner(prompt, true).await
    }

    async fn invoke_inner(&self, prompt: &str, trust_all_tools: bool) -> Result<String, AiError> {
        let mut cmd = Command::new(&self.kiro_path);
        cmd.arg("chat").arg("--no-interactive");
        if trust_all_tools {
            cmd.arg("--trust-all-tools");
        }
        // Pass prompt as the positional argument; long prompts are supported
        // on Linux/macOS via the argv mechanism. On Windows we fall back to
        // stdin below if the prompt exceeds a safe threshold.
        let use_stdin = cfg!(windows) && prompt.len() > 4000;
        if !use_stdin {
            cmd.arg(prompt);
        } else {
            // When reading from stdin, we still need SOME positional arg
            // on older kiro-cli versions; pass a marker that tells kiro to
            // read from stdin. Newer versions accept "-" for stdin.
            cmd.arg("-");
        }
        cmd.stdin(if use_stdin {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| AiError::SpawnFailed(e.to_string()))?;

        if use_stdin {
            if let Some(mut stdin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                let prompt_bytes = prompt.as_bytes().to_vec();
                // Write prompt and close stdin so kiro-cli knows the input ended.
                let _ = stdin.write_all(&prompt_bytes).await;
                let _ = stdin.shutdown().await;
                drop(stdin);
            }
        }

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
                    // If the flag is unrecognized, retry once without it so
                    // older kiro-cli versions still work.
                    if trust_all_tools
                        && (stderr.contains("unexpected argument")
                            || stderr.contains("Found argument")
                            || stderr.contains("unrecognized"))
                        && stderr.contains("trust-all-tools")
                    {
                        tracing::warn!(
                            "kiro-cli rejected --trust-all-tools; retrying without it. \
                             Consider upgrading kiro-cli to v1.26.0+."
                        );
                        // Fall through to caller's retry via Box::pin to avoid recursion issues.
                        return Err(AiError::KiroError(
                            "RETRY_WITHOUT_TRUST_ALL_TOOLS".to_string(),
                        ));
                    }
                    // Log stderr at debug level for troubleshooting without
                    // leaking it to the UI (which has a sanitized message).
                    tracing::debug!("kiro-cli stderr: {}", stderr);
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

    /// Invoke with automatic retry if --trust-all-tools is unsupported.
    ///
    /// This is the recommended entry point for callers — it hides version
    /// skew from the consumer so behavior is consistent across kiro-cli
    /// versions.
    pub async fn invoke_with_fallback(&self, prompt: &str) -> Result<String, AiError> {
        match self.invoke_inner(prompt, true).await {
            Err(AiError::KiroError(msg)) if msg == "RETRY_WITHOUT_TRUST_ALL_TOOLS" => {
                self.invoke_inner(prompt, false).await
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Helper: create a script that blocks for a long time.
    /// Uses .bat on Windows and an executable shell script elsewhere so the
    /// kiro_invoker tests pass on any host the crate builds on.
    #[cfg(windows)]
    fn make_slow_bat() -> NamedTempFile {
        let mut f = NamedTempFile::with_suffix(".bat").unwrap();
        writeln!(f, "@echo off").unwrap();
        writeln!(f, "ping -n 31 127.0.0.1 > nul").unwrap();
        f.flush().unwrap();
        f
    }

    #[cfg(not(windows))]
    fn make_slow_bat() -> NamedTempFile {
        use std::os::unix::fs::PermissionsExt;
        let mut f = NamedTempFile::with_suffix(".sh").unwrap();
        writeln!(f, "#!/bin/sh").unwrap();
        writeln!(f, "sleep 30").unwrap();
        f.flush().unwrap();
        let mut perms = std::fs::metadata(f.path()).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(f.path(), perms).unwrap();
        f
    }

    /// Helper: create a script that writes to stderr and exits non-zero.
    #[cfg(windows)]
    fn make_failing_bat(stderr_msg: &str, exit_code: u8) -> NamedTempFile {
        let mut f = NamedTempFile::with_suffix(".bat").unwrap();
        writeln!(f, "@echo off").unwrap();
        writeln!(f, "echo {}>&2", stderr_msg).unwrap();
        writeln!(f, "exit /b {}", exit_code).unwrap();
        f.flush().unwrap();
        f
    }

    #[cfg(not(windows))]
    fn make_failing_bat(stderr_msg: &str, exit_code: u8) -> NamedTempFile {
        use std::os::unix::fs::PermissionsExt;
        let mut f = NamedTempFile::with_suffix(".sh").unwrap();
        writeln!(f, "#!/bin/sh").unwrap();
        writeln!(f, "echo '{}' 1>&2", stderr_msg).unwrap();
        writeln!(f, "exit {}", exit_code).unwrap();
        f.flush().unwrap();
        let mut perms = std::fs::metadata(f.path()).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(f.path(), perms).unwrap();
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
        #[cfg(windows)]
        let bogus = PathBuf::from("C:\\nonexistent\\path\\to\\kiro-cli.exe");
        #[cfg(not(windows))]
        let bogus = PathBuf::from("/nonexistent/path/to/kiro-cli");

        let invoker = KiroInvoker::new(bogus, 10);
        let result = invoker.invoke("test").await;

        match result {
            Err(AiError::SpawnFailed(_)) => {} // expected
            other => panic!("Expected AiError::SpawnFailed, got: {:?}", other),
        }
    }
}

#[cfg(test)]
mod fallback_tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Script that rejects --trust-all-tools as "unexpected argument" on first
    /// call, then succeeds on the retry.
    #[cfg(not(windows))]
    fn make_version_skew_script() -> NamedTempFile {
        use std::os::unix::fs::PermissionsExt;
        let mut f = NamedTempFile::with_suffix(".sh").unwrap();
        writeln!(f, "#!/bin/sh").unwrap();
        // Look for --trust-all-tools in args; if present, error out.
        writeln!(f, "for arg in \"$@\"; do").unwrap();
        writeln!(f, "  if [ \"$arg\" = \"--trust-all-tools\" ]; then").unwrap();
        writeln!(
            f,
            "    echo 'error: unexpected argument --trust-all-tools' 1>&2"
        )
        .unwrap();
        writeln!(f, "    exit 2").unwrap();
        writeln!(f, "  fi").unwrap();
        writeln!(f, "done").unwrap();
        // Otherwise succeed with a canned response.
        writeln!(f, "echo 'retry succeeded'").unwrap();
        writeln!(f, "exit 0").unwrap();
        f.flush().unwrap();
        let mut perms = std::fs::metadata(f.path()).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(f.path(), perms).unwrap();
        f
    }

    /// invoke_with_fallback retries without --trust-all-tools when the flag
    /// is rejected as unknown.
    #[cfg(not(windows))]
    #[tokio::test]
    async fn test_invoke_with_fallback_retries_on_version_skew() {
        let script = make_version_skew_script();
        let invoker = KiroInvoker::new(script.path().to_path_buf(), 10);
        let result = invoker.invoke_with_fallback("test").await;
        assert!(
            matches!(result, Ok(ref s) if s.contains("retry succeeded")),
            "fallback should retry without --trust-all-tools, got: {:?}",
            result
        );
    }

    /// invoke_with_fallback propagates other errors verbatim (no retry).
    #[cfg(not(windows))]
    #[tokio::test]
    async fn test_invoke_with_fallback_propagates_other_errors() {
        use std::os::unix::fs::PermissionsExt;
        let mut f = NamedTempFile::with_suffix(".sh").unwrap();
        writeln!(f, "#!/bin/sh").unwrap();
        writeln!(f, "echo 'not logged in' 1>&2").unwrap();
        writeln!(f, "exit 1").unwrap();
        f.flush().unwrap();
        let mut perms = std::fs::metadata(f.path()).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(f.path(), perms).unwrap();

        let invoker = KiroInvoker::new(f.path().to_path_buf(), 10);
        let result = invoker.invoke_with_fallback("test").await;
        match result {
            Err(AiError::KiroError(msg)) => {
                assert!(msg.contains("not logged in"));
            }
            other => panic!("expected KiroError, got {:?}", other),
        }
    }
}
