//! Property-based tests for the redaction engine.
//!
//! Validates idempotence and leak prevention via proptest.

use madputty::ai::redactor::Redactor;
use proptest::prelude::*;

proptest! {
    /// Redaction is idempotent: applying it twice yields the same result.
    #[test]
    fn redact_is_idempotent(input in "\\PC{0,300}") {
        let r = Redactor::new();
        let once = r.redact(&input);
        let twice = r.redact(&once);
        prop_assert_eq!(&once, &twice);
    }

    /// Passwords are never leaked in the output.
    #[test]
    fn redact_never_leaks_password(secret in "[a-zA-Z0-9_]{1,30}") {
        let r = Redactor::new();
        let input = format!("password={}", secret);
        let output = r.redact(&input);
        let leaked = format!("password={}", secret);
        prop_assert!(!output.contains(&leaked));
        prop_assert!(output.contains("[REDACTED]"));
    }

    /// Tokens are never leaked in the output.
    #[test]
    fn redact_never_leaks_token(secret in "[a-zA-Z0-9_]{1,30}") {
        let r = Redactor::new();
        let input = format!("token={}", secret);
        let output = r.redact(&input);
        let leaked = format!("token={}", secret);
        prop_assert!(!output.contains(&leaked));
        prop_assert!(output.contains("[REDACTED]"));
    }

    /// IPv4 addresses are replaced with [IP].
    #[test]
    fn redact_never_leaks_ipv4(
        a in 0u8..=255,
        b in 0u8..=255,
        c in 0u8..=255,
        d in 0u8..=255,
    ) {
        let r = Redactor::new();
        let ip = format!("{a}.{b}.{c}.{d}");
        let input = format!("connecting to {ip} now");
        let output = r.redact(&input);
        prop_assert!(!output.contains(&ip));
        prop_assert!(output.contains("[IP]"));
    }

    /// Bearer tokens are redacted.
    #[test]
    fn redact_never_leaks_bearer(token in "[a-zA-Z0-9._-]{10,40}") {
        let r = Redactor::new();
        let input = format!("Authorization: Bearer {token}");
        let output = r.redact(&input);
        prop_assert!(!output.contains(&token));
        prop_assert!(output.contains("Bearer [REDACTED]"));
    }
}
