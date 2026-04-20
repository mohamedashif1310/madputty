//! Credential redaction engine.
//!
//! Applies a fixed set of regex patterns to strip passwords, tokens, IPs,
//! MACs, SSIDs, and API keys from log text before sending to kiro-cli.
//! Idempotent: `redact(redact(x)) == redact(x)`.

use regex::Regex;

pub struct Redactor {
    rules: Vec<(Regex, &'static str)>,
}

impl Redactor {
    /// Build with the default 6-pattern ruleset. Patterns compiled once.
    pub fn new() -> Self {
        let rules = vec![
            (Regex::new(r"password=\S+").unwrap(), "password=[REDACTED]"),
            (Regex::new(r"token=\S+").unwrap(), "token=[REDACTED]"),
            (
                Regex::new(r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}").unwrap(),
                "[IP]",
            ),
            (
                Regex::new(r"([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}").unwrap(),
                "[MAC]",
            ),
            (Regex::new(r"SSID=\S+").unwrap(), "SSID=[SSID]"),
            (
                Regex::new(r"(?i)(api[_-]?key|secret[_-]?key|access[_-]?key)\s*[=:]\s*\S+")
                    .unwrap(),
                "[REDACTED]",
            ),
        ];
        Self { rules }
    }

    /// Apply all redaction patterns. Idempotent.
    pub fn redact(&self, input: &str) -> String {
        let mut result = input.to_string();
        for (pattern, replacement) in &self.rules {
            result = pattern.replace_all(&result, *replacement).to_string();
        }
        result
    }
}

impl Default for Redactor {
    fn default() -> Self {
        Self::new()
    }
}
