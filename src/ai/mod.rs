//! AI subsystem orchestrator.
//!
//! Detects kiro-cli on PATH, probes login state, and provides the entry
//! point for AI analysis tasks. When AI is disabled, all methods are no-ops.
//!
//! Orchestration follows a "latest wins" pattern: when a new AI task starts,
//! any previously running task is cancelled via `JoinHandle::abort()`.

pub mod error_scanner;
pub mod kiro_invoker;
pub mod pane;
pub mod redactor;
pub mod response_log;
pub mod rolling_buffer;

use std::path::PathBuf;
use std::time::Duration;

use tokio::task::JoinHandle;

use kiro_invoker::KiroInvoker;
use pane::AiPaneState;
use redactor::Redactor;
use response_log::ResponseLog;
use rolling_buffer::RollingBuffer;

#[allow(dead_code)]
const SYSTEM_PROMPT: &str = "You are a serial log analyst helping a firmware engineer. \
    Analyze these live serial logs and explain what is happening in plain English. \
    Call out errors, state transitions, and likely root causes. Be concise — 3 to 5 sentences. \
    If you see WiFi connection attempts, identify the security mode, SSID if present, \
    and whether the attempt succeeded or failed.";

/// Result of a completed AI task pipeline.
#[allow(dead_code)]
pub enum AiTaskResult {
    /// Successful response from kiro-cli.
    Success(String),
    /// An error occurred (timeout, kiro error, spawn failure).
    Error(String),
    /// AI is not logged in — cannot invoke.
    NotLoggedIn,
}

pub struct AiSubsystem {
    #[allow(dead_code)]
    pub kiro_path: Option<PathBuf>,
    pub logged_in: bool,
    pub enabled: bool,
    pub invoker: Option<KiroInvoker>,
    pub redactor: Redactor,
    /// Handle to the currently running AI task (latest wins — previous is aborted).
    #[allow(dead_code)]
    current_task: Option<JoinHandle<()>>,
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
            current_task: None,
        }
    }

    fn disabled() -> Self {
        Self {
            kiro_path: None,
            logged_in: false,
            enabled: false,
            invoker: None,
            redactor: Redactor::new(),
            current_task: None,
        }
    }

    /// Cancel any currently running AI task (latest wins).
    #[allow(dead_code)]
    fn cancel_current_task(&mut self) {
        if let Some(handle) = self.current_task.take() {
            handle.abort();
        }
    }

    /// Trigger a manual analysis (Ctrl+A A or auto-watch).
    ///
    /// Pipeline: snapshot → redact → build prompt → invoke kiro-cli → update pane → append log.
    ///
    /// If a previous AI task is running, it is cancelled (latest wins).
    /// Returns immediately after spawning the task. The pane and log are updated
    /// asynchronously when the task completes.
    #[allow(dead_code)]
    pub fn analyze(
        &mut self,
        buffer: &RollingBuffer,
        pane: std::sync::Arc<std::sync::Mutex<AiPaneState>>,
        log: std::sync::Arc<std::sync::Mutex<ResponseLog>>,
    ) {
        if !self.enabled {
            return;
        }

        if !self.logged_in {
            let mut p = pane.lock().unwrap();
            p.set_error("⚠ Please run `madputty kiro-login` first".to_string());
            return;
        }

        // Cancel any previous task (latest wins).
        self.cancel_current_task();

        // 1. Snapshot the rolling buffer.
        let snapshot = buffer.snapshot();
        let snapshot_text = snapshot.join("\n");

        // 2. Redact the snapshot.
        let redacted = self.redactor.redact(&snapshot_text);

        // 3. Build the prompt.
        let prompt = self.build_analysis_prompt(&redacted);

        // 4. Set spinner active.
        {
            let mut p = pane.lock().unwrap();
            p.set_spinner(true);
        }

        // 5. Spawn the AI task.
        let invoker = self.invoker.as_ref().unwrap().clone();
        let handle = tokio::spawn(async move {
            let result = invoker.invoke(&prompt).await;
            let now = format_time_now();

            match result {
                Ok(response) => {
                    {
                        let mut p = pane.lock().unwrap();
                        p.set_response(response.clone(), now);
                    }
                    // Append to response log (best-effort).
                    if let Ok(mut l) = log.lock() {
                        let _ = l.append("Ctrl+A A", None, &response);
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    let mut p = pane.lock().unwrap();
                    p.set_error(msg);
                }
            }
        });

        self.current_task = Some(handle);
    }

    /// Trigger a custom question analysis (Ctrl+A Q).
    ///
    /// Pipeline: snapshot → redact → build question prompt → invoke kiro-cli → update pane → append log.
    ///
    /// If a previous AI task is running, it is cancelled (latest wins).
    #[allow(dead_code)]
    pub fn ask_question(
        &mut self,
        question: &str,
        buffer: &RollingBuffer,
        pane: std::sync::Arc<std::sync::Mutex<AiPaneState>>,
        log: std::sync::Arc<std::sync::Mutex<ResponseLog>>,
    ) {
        if !self.enabled {
            return;
        }

        if !self.logged_in {
            let mut p = pane.lock().unwrap();
            p.set_error("⚠ Please run `madputty kiro-login` first".to_string());
            return;
        }

        // Cancel any previous task (latest wins).
        self.cancel_current_task();

        // 1. Snapshot the rolling buffer.
        let snapshot = buffer.snapshot();
        let snapshot_text = snapshot.join("\n");

        // 2. Redact the snapshot.
        let redacted = self.redactor.redact(&snapshot_text);

        // 3. Build the question prompt.
        let prompt = self.build_question_prompt(question, &redacted);

        // 4. Set spinner active.
        {
            let mut p = pane.lock().unwrap();
            p.set_spinner(true);
        }

        // 5. Spawn the AI task.
        let invoker = self.invoker.as_ref().unwrap().clone();
        let question_owned = question.to_string();
        let handle = tokio::spawn(async move {
            let result = invoker.invoke(&prompt).await;
            let now = format_time_now();

            match result {
                Ok(response) => {
                    {
                        let mut p = pane.lock().unwrap();
                        p.set_response(response.clone(), now);
                    }
                    // Append to response log (best-effort).
                    if let Ok(mut l) = log.lock() {
                        let _ = l.append("Ctrl+A Q", Some(&question_owned), &response);
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    let mut p = pane.lock().unwrap();
                    p.set_error(msg);
                }
            }
        });

        self.current_task = Some(handle);
    }

    /// Build the full prompt for a default analysis request.
    #[allow(dead_code)]
    pub fn build_analysis_prompt(&self, redacted_snapshot: &str) -> String {
        format!("{SYSTEM_PROMPT}\n\nLogs:\n{redacted_snapshot}")
    }

    /// Build the prompt for a custom question.
    #[allow(dead_code)]
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

/// Public helper for subcommand dispatch — returns error if kiro-cli not found.
pub fn find_kiro_cli_or_error() -> Result<PathBuf, crate::errors::MadPuttyError> {
    find_kiro_cli().ok_or_else(|| {
        crate::errors::MadPuttyError::AiError("kiro-cli not found on PATH".to_string())
    })
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

/// Format the current time as `HH:MM:SS` for the AI pane header.
fn format_time_now() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let hour = (secs / 3600) % 24;
    let min = (secs / 60) % 60;
    let sec = secs % 60;
    format!("{hour:02}:{min:02}:{sec:02}")
}
