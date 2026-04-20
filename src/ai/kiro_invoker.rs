//! Shell out to `kiro-cli chat --no-interactive` with a bounded timeout.
//!
//! Spawns kiro-cli as a child process via `tokio::process::Command`.
//! On timeout, kills the child and reaps it to avoid zombies.

use std::path::PathBuf;
use std::time::Duration;

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
    pub async fn invoke(&self, prompt: &str) -> Result<String, AiError> {
        let mut child = Command::new(&self.kiro_path)
            .args(["chat", "--no-interactive", prompt])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| AiError::SpawnFailed(e.to_string()))?;

        let child_id = child.id();
        match timeout(self.timeout_duration, child.wait_with_output()).await {
            Ok(Ok(output)) => {
                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let first_line = stderr.lines().next().unwrap_or("unknown error");
                    Err(AiError::KiroError(first_line.to_string()))
                }
            }
            Ok(Err(e)) => Err(AiError::SpawnFailed(e.to_string())),
            Err(_) => {
                // Timeout — kill the child by PID (child already moved into wait_with_output)
                #[cfg(windows)]
                if let Some(pid) = child_id {
                    let _ = std::process::Command::new("taskkill")
                        .args(["/F", "/PID", &pid.to_string()])
                        .output();
                }
                #[cfg(not(windows))]
                {
                    let _ = child_id; // suppress unused warning
                }
                Err(AiError::Timeout(self.timeout_duration))
            }
        }
    }
}
