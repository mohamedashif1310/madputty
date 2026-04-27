//! Log-line colorizer.
//!
//! Parses common embedded-system log formats and applies color to timestamps,
//! task/thread IDs, log levels (I/W/E), module tags in brackets, numbers,
//! shell prompts (`rfw>`, `>`, `#`), and highlight keywords (`ERROR`, `FAIL`,
//! etc.). The colorizer is byte-stream aware: it buffers partial lines and
//! emits colored output only once a line is complete (or a timeout passes).

use std::io::{self, Write};
use std::time::{Duration, Instant};

use crate::theme::Palette;

/// How long to wait for a line terminator before flushing a partial line.
/// Keeps interactive prompts responsive (e.g. `rfw>` without a newline).
const PARTIAL_FLUSH_MS: u64 = 100;

pub struct Colorizer {
    palette: Palette,
    buf: String,
    last_write: Instant,
    enabled: bool,
}

impl Colorizer {
    pub fn new(palette: Palette, enabled: bool) -> Self {
        Self {
            palette,
            buf: String::new(),
            last_write: Instant::now(),
            enabled,
        }
    }

    /// Feed raw bytes from the port. Emits colored lines to `out`.
    pub fn feed<W: Write>(&mut self, bytes: &[u8], out: &mut W) -> io::Result<()> {
        if !self.enabled {
            out.write_all(bytes)?;
            out.flush()?;
            return Ok(());
        }

        // Append bytes (lossy UTF-8 decoding preserves unknown bytes).
        self.buf.push_str(&String::from_utf8_lossy(bytes));

        // Flush whole lines.
        while let Some(idx) = self.buf.find('\n') {
            let line: String = self.buf.drain(..=idx).collect();
            let line_stripped = line.trim_end_matches(['\n', '\r']);
            let colored = colorize_line(line_stripped, &self.palette);
            writeln!(out, "{}", colored)?;
        }

        // Flush partial line if it's been idle for a bit (for prompts).
        if !self.buf.is_empty()
            && self.last_write.elapsed() > Duration::from_millis(PARTIAL_FLUSH_MS)
        {
            let partial = std::mem::take(&mut self.buf);
            let colored = colorize_line(&partial, &self.palette);
            write!(out, "{}", colored)?;
        }

        self.last_write = Instant::now();
        out.flush()?;
        Ok(())
    }

    /// Flush any buffered partial line at shutdown.
    pub fn flush<W: Write>(&mut self, out: &mut W) -> io::Result<()> {
        if !self.buf.is_empty() {
            let partial = std::mem::take(&mut self.buf);
            let colored = colorize_line(&partial, &self.palette);
            write!(out, "{}", colored)?;
        }
        out.flush()
    }
}

/// Apply color to a single log line based on pattern recognition.
fn colorize_line(line: &str, p: &Palette) -> String {
    let trimmed = line.trim_start();

    // 1. Recognize shell prompts (line ends with `>`, `#`, or `$`).
    if let Some(last) = trimmed.chars().last() {
        if matches!(last, '>' | '#' | '$') && trimmed.len() <= 12 {
            return p.log_prompt.apply_to(line).to_string();
        }
    }

    // 2. Parse the common embedded format:
    //    [timestamp][task_id] LEVEL [Module:line]Message
    // Work through segments left-to-right.
    let mut out = String::new();
    let mut rest = line;

    // Timestamp segment: leading [ISO-like string]
    if let Some((ts, after)) = take_bracket_segment(rest) {
        // Heuristic: timestamps contain 'T' and 'Z' or ':'.
        if ts.contains('T') && (ts.ends_with('Z') || ts.contains(':')) {
            out.push_str(&p.log_timestamp.apply_to(format!("[{}]", ts)).to_string());
            rest = after;
        } else {
            out.push_str(&p.log_module.apply_to(format!("[{}]", ts)).to_string());
            rest = after;
        }
    }

    // Task/thread ID segment: [hex_id]
    if let Some((tid, after)) = take_bracket_segment(rest) {
        if is_hex_id(tid) {
            out.push_str(&p.log_task_id.apply_to(format!("[{}]", tid)).to_string());
            rest = after;
        } else {
            // Not a task ID; treat as module tag.
            out.push_str(&p.log_module.apply_to(format!("[{}]", tid)).to_string());
            rest = after;
        }
    }

    // Level segment: " I ", " W ", or " E "
    let rest_trim = rest.trim_start();
    let leading_space = &rest[..rest.len() - rest_trim.len()];
    out.push_str(leading_space);

    let (level_style, level_len) = match rest_trim.chars().next() {
        Some('E') if is_level_boundary(rest_trim) => (Some(&p.log_level_error), 1),
        Some('W') if is_level_boundary(rest_trim) => (Some(&p.log_level_warn), 1),
        Some('I') if is_level_boundary(rest_trim) => (Some(&p.log_level_info), 1),
        _ => (None, 0),
    };

    if let Some(style) = level_style {
        let (level_ch, after) = rest_trim.split_at(level_len);
        let styled: String = style.apply_to(level_ch).to_string();
        out.push_str(&styled);
        rest = after;
    } else {
        rest = rest_trim;
    }

    // Remaining: highlight more bracketed module tags + keywords + numbers.
    let rest_highlighted = highlight_keywords(&highlight_brackets(rest, p), p);
    out.push_str(&rest_highlighted);

    out
}

/// Extract a `[...]` segment from the start of a string, returning (inner, rest).
fn take_bracket_segment(s: &str) -> Option<(&str, &str)> {
    if !s.starts_with('[') {
        return None;
    }
    let rest = &s[1..];
    let end = rest.find(']')?;
    Some((&rest[..end], &rest[end + 1..]))
}

/// Heuristic: matches a hex/numeric task ID (at least 4 hex digits).
fn is_hex_id(s: &str) -> bool {
    s.len() >= 4 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// True if the next char after position 0 is whitespace or end-of-string
/// (so we don't recolor `Info` or `Error` as a level letter).
fn is_level_boundary(s: &str) -> bool {
    s.chars().nth(1).is_none_or(|c| c.is_whitespace())
}

/// Color all `[...]` bracketed segments as module tags.
fn highlight_brackets(s: &str, p: &Palette) -> String {
    let mut out = String::with_capacity(s.len());
    let chars = s.chars().peekable();
    let mut in_bracket = false;
    let mut buf = String::new();

    for ch in chars {
        if ch == '[' && !in_bracket {
            in_bracket = true;
            buf.push(ch);
        } else if ch == ']' && in_bracket {
            buf.push(ch);
            out.push_str(&p.log_module.apply_to(&buf).to_string());
            buf.clear();
            in_bracket = false;
        } else if in_bracket {
            buf.push(ch);
        } else {
            out.push(ch);
        }
    }
    out.push_str(&buf); // unterminated bracket stays as-is
    out
}

/// Highlight interesting keywords in error/warn/ok colors.
fn highlight_keywords(s: &str, p: &Palette) -> String {
    // Use simple case-insensitive substring search. We apply ANSI codes by
    // rebuilding the string with styled substrings.
    let triggers: &[(&str, &console::Style)] = &[
        ("ERROR", &p.log_keyword_error),
        ("FAIL", &p.log_keyword_error),
        ("FAILED", &p.log_keyword_error),
        ("PANIC", &p.log_keyword_error),
        ("CRASH", &p.log_keyword_error),
        ("TIMEOUT", &p.log_keyword_warn),
        ("RETRY", &p.log_keyword_warn),
        ("WARN", &p.log_keyword_warn),
        ("SUCCESS", &p.log_keyword_ok),
        ("JOINED", &p.log_keyword_ok),
        ("CONNECTED", &p.log_keyword_ok),
        ("READY", &p.log_keyword_ok),
    ];

    let mut result = s.to_string();
    for (keyword, style) in triggers {
        result = replace_case_insensitive(&result, keyword, style);
    }
    result
}

/// Replace case-insensitive matches of `keyword` with a styled version.
/// Preserves the original casing of each match.
fn replace_case_insensitive(haystack: &str, keyword: &str, style: &console::Style) -> String {
    let lower = haystack.to_lowercase();
    let needle = keyword.to_lowercase();
    let mut out = String::with_capacity(haystack.len());
    let mut last = 0;
    let mut idx = 0;

    while let Some(pos) = lower[idx..].find(&needle) {
        let abs = idx + pos;
        let end = abs + keyword.len();
        out.push_str(&haystack[last..abs]);
        out.push_str(&style.apply_to(&haystack[abs..end]).to_string());
        last = end;
        idx = end;
    }
    out.push_str(&haystack[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Palette;

    /// Helper: feed bytes, collect output, assert it contains expected substring.
    fn feed_and_collect(bytes: &[u8], enabled: bool) -> String {
        let mut c = Colorizer::new(Palette::plain(), enabled);
        let mut out = Vec::new();
        c.feed(bytes, &mut out).unwrap();
        c.flush(&mut out).unwrap();
        String::from_utf8_lossy(&out).to_string()
    }

    #[test]
    fn disabled_passes_bytes_through_unchanged() {
        let out = feed_and_collect(b"raw bytes here\n", false);
        assert_eq!(out, "raw bytes here\n");
    }

    #[test]
    fn enabled_plain_palette_preserves_content() {
        let out = feed_and_collect(b"hello world\n", true);
        assert!(out.contains("hello world"));
    }

    #[test]
    fn multiple_complete_lines_are_all_emitted() {
        let out = feed_and_collect(b"line one\nline two\nline three\n", true);
        assert!(out.contains("line one"));
        assert!(out.contains("line two"));
        assert!(out.contains("line three"));
    }

    #[test]
    fn partial_line_without_newline_is_buffered_then_flushed() {
        let mut c = Colorizer::new(Palette::plain(), true);
        let mut out = Vec::new();
        c.feed(b"no newline here", &mut out).unwrap();
        // Until flush, the line may not be fully emitted if it came in fast.
        c.flush(&mut out).unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(s.contains("no newline here"));
    }

    #[test]
    fn crlf_line_endings_are_stripped() {
        let out = feed_and_collect(b"windows line\r\n", true);
        // Content preserved, CR should be trimmed before coloring
        assert!(out.contains("windows line"));
        // The emitted output ends each line with \n only
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn empty_input_produces_empty_output() {
        let out = feed_and_collect(b"", true);
        assert_eq!(out, "");
    }

    #[test]
    fn invalid_utf8_bytes_dont_panic() {
        // 0xFF is invalid UTF-8
        let bytes = b"valid \xFF\xFE then more\n";
        let out = feed_and_collect(bytes, true);
        assert!(out.contains("valid"));
        assert!(out.contains("then more"));
    }

    #[test]
    fn error_keyword_passes_through_in_plain_palette() {
        let out = feed_and_collect(b"ERROR: kernel panic detected\n", true);
        assert!(out.contains("ERROR"));
        assert!(out.contains("kernel panic"));
    }

    #[test]
    fn bracketed_timestamp_content_preserved() {
        let out = feed_and_collect(b"[2024-01-01T12:00:00Z] startup\n", true);
        assert!(out.contains("2024-01-01T12:00:00Z"));
        assert!(out.contains("startup"));
    }

    #[test]
    fn shell_prompt_recognized() {
        let out = feed_and_collect(b"rfw>\n", true);
        assert!(out.contains("rfw>"));
    }

    #[test]
    fn flush_idempotent_on_empty_buffer() {
        let mut c = Colorizer::new(Palette::plain(), true);
        let mut out = Vec::new();
        c.flush(&mut out).unwrap();
        c.flush(&mut out).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn is_hex_id_recognizes_hex_task_ids() {
        assert!(is_hex_id("DEADBEEF"));
        assert!(is_hex_id("0x1234"[2..].as_ref()));
        assert!(is_hex_id("abcd"));
    }

    #[test]
    fn is_hex_id_rejects_short_or_non_hex() {
        assert!(!is_hex_id("abc")); // too short
        assert!(!is_hex_id("xyz123"));
        assert!(!is_hex_id(""));
    }

    #[test]
    fn take_bracket_segment_extracts_inner() {
        assert_eq!(
            take_bracket_segment("[hello]world"),
            Some(("hello", "world"))
        );
        assert_eq!(take_bracket_segment("[a][b]"), Some(("a", "[b]")));
    }

    #[test]
    fn take_bracket_segment_none_without_leading_bracket() {
        assert!(take_bracket_segment("no bracket").is_none());
        assert!(take_bracket_segment("").is_none());
    }

    #[test]
    fn is_level_boundary_true_for_space_after() {
        assert!(is_level_boundary("E rest"));
        assert!(is_level_boundary("W")); // end of string
    }

    #[test]
    fn is_level_boundary_false_when_letter_follows() {
        assert!(!is_level_boundary("Error"));
        assert!(!is_level_boundary("Warn"));
    }

    #[test]
    fn replace_case_insensitive_preserves_case() {
        let style = console::Style::new();
        let out = replace_case_insensitive("an ERROR and an error", "error", &style);
        // Both casings must be present in the output
        assert!(out.contains("ERROR"));
        assert!(out.contains("error"));
    }
}
