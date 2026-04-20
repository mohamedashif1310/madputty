//! Append-only Markdown log of AI responses per session.
//!
//! Creates `~/.madputty/ai-responses/<session_id>.md` on first write.
//! Each entry has a timestamp header, trigger type, optional question, and response body.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct ResponseLog {
    path: PathBuf,
    has_entries: bool,
}

impl ResponseLog {
    pub fn new(session_id: &str) -> Self {
        let dir = dirs_next_or_home().join(".madputty").join("ai-responses");
        let path = dir.join(format!("{session_id}.md"));
        Self {
            path,
            has_entries: false,
        }
    }

    /// Append a timestamped entry. Creates directory + file on first write.
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

        let now = chrono_local_now();
        writeln!(file, "## {now} — {trigger}")?;
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

    pub fn has_entries(&self) -> bool {
        self.has_entries
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn dirs_next_or_home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn chrono_local_now() -> String {
    // Use a simple format without pulling in chrono — just system time
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Simple UTC timestamp (good enough for log headers)
    format!("{secs}")
}
