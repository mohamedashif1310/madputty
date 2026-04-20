//! AI subsystem orchestrator.
//!
//! Detects kiro-cli on PATH, probes login state, and provides the entry
//! point for AI analysis tasks. When AI is disabled, all methods are no-ops.

pub mod error_scanner;
pub mod kiro_invoker;
pub mod pane;
pub mod redactor;
pub mod response_log;
pub mod rolling_buffer;

use std::path::PathBuf;
use std::time::Duration;

use kiro_invoker::KiroInvoker;
use redactor::Redactor;

const SYSTEM_PROMPT: &str = "You are a serial log analyst helping a firmware engineer. \
    Analyze these live serial logs and explain what is happening in plain English. \
    Call out errors, state transitions, and likely root causes. Be concise — 3 to 5 sentences. \
    If you see WiFi connection attempts, identify the security mode, SSID if present, \
    and whether the attempt succeeded or failed.";

pub struct AiSubsystem {
    pub kiro_path: Option<PathBuf>,
    pub logged_in: bool,
    pub enabled: bool,
    pub invoker: Option<KiroInvoker>,
    pub redactor: Redactor,
}

impl AiSubsystem {
    /// Detect kiro-cli on PATH and probe login state.
    pub async fn detect(no_ai: bool, timeout_seconds: u32) -> Self {
        if no_ai {
            return Self::disabled();
        }

        let kiro_path = find_kiro_cli();
        if kiro_path.is_none() {
            eprintln!(
                "⚠ kiro-cli not found — AI analysis disabled. Install kiro-cli for AI features."
            );
            return Self::disabled();
        }

        let path = kiro_path.unwrap();
        let logged_in = check_login(&path).await;
        if !logged_in {
            eprintln!(
                "⚠ kiro-cli found but not logged in. Run `madputty kiro-login` to enable AI."
            );
        }

        let invoker = KiroInvoker::new(path.clone(), timeout_seconds);

        Self {
            kiro_path: Some(path),
            logged_in,
            enabled: true,
            invoker: Some(invoker),
            redactor: Redactor::new(),
        }
    }

    fn disabled() -> Self {
        Self {
            kiro_path: None,
            logged_in: false,
            enabled: false,
            invoker: None,
            redactor: Redactor::new(),
        }
    }

    /// Build the full prompt for a default analysis request.
    pub fn build_analysis_prompt(&self, redacted_snapshot: &str) -> String {
        format!("{SYSTEM_PROMPT}\n\nLogs:\n{redacted_snapshot}")
    }

    /// Build the prompt for a custom question.
    pub fn build_question_prompt(&self, question: &str, redacted_snapshot: &str) -> String {
        format!("{question}\n\nLogs:\n{redacted_snapshot}")
    }
}

/// Search PATH for kiro-cli or kiro-cli.exe.
fn find_kiro_cli() -> Option<PathBuf> {
    let names = if cfg!(windows) {
        vec!["kiro-cli.exe", "kiro-cli"]
    } else {
        vec!["kiro-cli"]
    };

    if let Ok(path_var) = std::env::var("PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for dir in path_var.split(sep) {
            for name in &names {
                let candidate = PathBuf::from(dir).join(name);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// Probe login state by running `kiro-cli whoami --no-interactive` with a 5s timeout.
async fn check_login(kiro_path: &PathBuf) -> bool {
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new(kiro_path)
            .args(["whoami", "--no-interactive"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status(),
    )
    .await;

    matches!(result, Ok(Ok(status)) if status.success())
}
