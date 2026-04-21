//! Credential redaction engine.
//!
//! Applies a fixed set of regex patterns to strip passwords, tokens, IPs,
//! MACs, SSIDs, and API keys from log text before sending to kiro-cli.
//! Idempotent: `redact(redact(x)) == redact(x)`.

use regex::Regex;

/// A compiled redaction rule. The replacement is either a static string
/// or a capture-group template (for the api/secret/access key pattern).
struct RedactionRule {
    pattern: Regex,
    replacement: String,
}

pub struct Redactor {
    rules: Vec<RedactionRule>,
}

impl Redactor {
    /// Build with the default 6-pattern ruleset. Patterns compiled once.
    pub fn new() -> Self {
        let rules = vec![
            RedactionRule {
                pattern: Regex::new(r"password=\S+").unwrap(),
                replacement: "password=[REDACTED]".to_string(),
            },
            RedactionRule {
                pattern: Regex::new(r"token=\S+").unwrap(),
                replacement: "token=[REDACTED]".to_string(),
            },
            RedactionRule {
                // Word-boundary anchored to avoid matching version strings like "1.2.3.4"
                pattern: Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap(),
                replacement: "[IP]".to_string(),
            },
            RedactionRule {
                pattern: Regex::new(r"([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}").unwrap(),
                replacement: "[MAC]".to_string(),
            },
            RedactionRule {
                pattern: Regex::new(r"SSID=\S+").unwrap(),
                replacement: "SSID=[SSID]".to_string(),
            },
            RedactionRule {
                pattern: Regex::new(
                    r"(?i)(api[_-]?key|secret[_-]?key|access[_-]?key)\s*[=:]\s*\S+",
                )
                .unwrap(),
                replacement: "${1}=[REDACTED]".to_string(),
            },
            // Bearer tokens
            RedactionRule {
                pattern: Regex::new(r"(?i)Bearer\s+\S+").unwrap(),
                replacement: "Bearer [REDACTED]".to_string(),
            },
            // AWS access key IDs (AKIA...)
            RedactionRule {
                pattern: Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
                replacement: "[AWS_KEY]".to_string(),
            },
        ];
        Self { rules }
    }

    /// Apply all redaction patterns sequentially. Idempotent.
    pub fn redact(&self, input: &str) -> String {
        let mut result = input.to_string();
        for rule in &self.rules {
            result = rule
                .pattern
                .replace_all(&result, &rule.replacement)
                .to_string();
        }
        result
    }
}

impl Default for Redactor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // Validates: Requirements 6.8
    // Redactor is idempotent: applying redact twice yields the same result as once.
    proptest! {
        #[test]
        fn redact_is_idempotent(input in "\\PC{0,200}") {
            let r = Redactor::new();
            let once = r.redact(&input);
            let twice = r.redact(&once);
            prop_assert_eq!(&once, &twice, "redact was not idempotent for input: {:?}", input);
        }
    }

    // Validates: Requirements 6.8
    // When input contains a password pattern, the output never leaks the original value.
    proptest! {
        #[test]
        fn redact_never_leaks_password(secret in "[a-zA-Z0-9_]{1,30}") {
            let r = Redactor::new();
            let input = format!("password={}", secret);
            let output = r.redact(&input);
            prop_assert!(!output.contains(&format!("password={}", secret)),
                "output leaked password value: {}", output);
            prop_assert!(output.contains("password=[REDACTED]"));
        }
    }

    // Validates: Requirements 6.8
    // When input contains a token pattern, the output never leaks the original value.
    proptest! {
        #[test]
        fn redact_never_leaks_token(secret in "[a-zA-Z0-9_]{1,30}") {
            let r = Redactor::new();
            let input = format!("token={}", secret);
            let output = r.redact(&input);
            prop_assert!(!output.contains(&format!("token={}", secret)),
                "output leaked token value: {}", output);
            prop_assert!(output.contains("token=[REDACTED]"));
        }
    }

    // Validates: Requirements 6.8
    // When input contains an IPv4 address, the output never leaks the original IP.
    proptest! {
        #[test]
        fn redact_never_leaks_ipv4(
            a in 0u8..=255,
            b in 0u8..=255,
            c in 0u8..=255,
            d in 0u8..=255,
        ) {
            let r = Redactor::new();
            let ip = format!("{}.{}.{}.{}", a, b, c, d);
            let input = format!("connecting to {}", ip);
            let output = r.redact(&input);
            prop_assert!(!output.contains(&ip),
                "output leaked IP address: {}", output);
            prop_assert!(output.contains("[IP]"));
        }
    }

    // ─── Unit tests for individual redaction patterns ───────────────────

    /// **Validates: Requirements 6.2**
    #[test]
    fn redact_password_pattern() {
        let r = Redactor::new();
        assert_eq!(r.redact("password=hunter2"), "password=[REDACTED]");
        assert_eq!(
            r.redact("login password=s3cr3t! done"),
            "login password=[REDACTED] done"
        );
    }

    /// **Validates: Requirements 6.3**
    #[test]
    fn redact_token_pattern() {
        let r = Redactor::new();
        assert_eq!(r.redact("token=abc123xyz"), "token=[REDACTED]");
        assert_eq!(
            r.redact("auth token=DEADBEEF ok"),
            "auth token=[REDACTED] ok"
        );
    }

    /// **Validates: Requirements 6.4**
    #[test]
    fn redact_ipv4_pattern() {
        let r = Redactor::new();
        assert_eq!(r.redact("connecting to 192.168.1.1"), "connecting to [IP]");
        assert_eq!(r.redact("src=10.0.0.1 dst=172.16.0.5"), "src=[IP] dst=[IP]");
    }

    /// **Validates: Requirements 6.5**
    #[test]
    fn redact_mac_pattern() {
        let r = Redactor::new();
        assert_eq!(r.redact("mac=AA:BB:CC:DD:EE:FF"), "mac=[MAC]");
        assert_eq!(
            r.redact("device 01:23:45:67:89:ab connected"),
            "device [MAC] connected"
        );
    }

    /// **Validates: Requirements 6.6**
    #[test]
    fn redact_ssid_pattern() {
        let r = Redactor::new();
        assert_eq!(r.redact("SSID=MyNetwork"), "SSID=[SSID]");
        assert_eq!(
            r.redact("joining SSID=Home_WiFi_5G now"),
            "joining SSID=[SSID] now"
        );
    }

    /// **Validates: Requirements 6.7**
    #[test]
    fn redact_api_key_patterns() {
        let r = Redactor::new();
        assert_eq!(r.redact("api_key=abcdef123"), "api_key=[REDACTED]");
        assert_eq!(r.redact("secret_key: s3cr3t"), "secret_key=[REDACTED]");
        assert_eq!(
            r.redact("access_key = AKIAIOSFODNN7"),
            "access_key=[REDACTED]"
        );
        // Case-insensitive
        assert_eq!(r.redact("API_KEY=xyz"), "API_KEY=[REDACTED]");
        assert_eq!(r.redact("Secret-Key: foo"), "Secret-Key=[REDACTED]");
    }

    /// **Validates: Requirements 6.2, 6.3, 6.4, 6.5, 6.6, 6.7**
    #[test]
    fn redact_combined_input() {
        let r = Redactor::new();
        let input = "password=abc token=xyz host=192.168.0.1 mac=AA:BB:CC:DD:EE:FF SSID=Test api_key=secret";
        let output = r.redact(input);
        assert!(output.contains("password=[REDACTED]"));
        assert!(output.contains("token=[REDACTED]"));
        assert!(output.contains("[IP]"));
        assert!(output.contains("[MAC]"));
        assert!(output.contains("SSID=[SSID]"));
        assert!(output.contains("api_key=[REDACTED]"));
        // Originals gone
        assert!(!output.contains("password=abc"));
        assert!(!output.contains("token=xyz"));
        assert!(!output.contains("192.168.0.1"));
        assert!(!output.contains("AA:BB:CC:DD:EE:FF"));
        assert!(!output.contains("SSID=Test"));
        assert!(!output.contains("api_key=secret"));
    }

    /// **Validates: Requirements 6.2, 6.3, 6.4, 6.5, 6.6, 6.7**
    #[test]
    fn redact_non_sensitive_text_unchanged() {
        let r = Redactor::new();
        let input = "INFO: system booted successfully in 1.23 seconds";
        assert_eq!(r.redact(input), input);

        let input2 = "DEBUG: no errors detected, all modules loaded";
        assert_eq!(r.redact(input2), input2);
    }
}
