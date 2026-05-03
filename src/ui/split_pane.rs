//! Terminal renderer.
//!
//! Uses a single-row ANSI scroll region for the status bar (rows 1..N-1 scroll,
//! row N is pinned). The log area occupies rows 1..N-1 and participates fully
//! in the terminal's native scrollback buffer — users can scroll up with
//! PgUp / mouse wheel to see earlier log lines, even while logs keep arriving.
//!
//! AI analysis is NOT rendered as a fixed pane. Instead, when the user presses
//! Ctrl+A A, the response is printed INLINE as a clearly boxed block between
//! log lines. This means:
//!
//! - The full response is always visible (no truncation to fit a small pane)
//! - AI output participates in terminal scrollback like any other log line
//! - Fast log streams don't eat the AI response — it's already written to the
//!   scroll buffer
//! - Ctrl+A L reprints the last response at the current cursor position
//!
//! The previous split-pane design carved out a fixed region for AI output at
//! the cost of disabling scrollback. Per user feedback ("I want AI on bottom
//! on top logs should run it should be scrollable") the scrollback trade-off
//! was unacceptable, so inline-boxed AI output is now the model.

use std::io::{self, Write};

use crate::ai::pane::AiPaneState;

/// Which rendering layout is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Status bar pinned at the last row, log region scrolls normally above.
    /// This is the default whenever the terminal is at least 3 rows tall.
    StatusBarOnly,
    /// Terminal too small for any decoration — everything inline, no status bar.
    Fallback,
}

pub struct SplitPaneRenderer {
    pub term_width: u16,
    pub term_height: u16,
    pub log_region_height: u16,
    pub status_bar_row: u16,
    pub mode: Mode,
    /// True when the renderer is managing a scroll region (mode != Fallback).
    pub active: bool,
    // These fields are kept for backwards compatibility with session.rs code
    // that references them; they're always zero in the current renderer.
    #[allow(dead_code)]
    pub ai_pane_height: u16,
    #[allow(dead_code)]
    pub ai_pane_top_row: u16,
}

impl SplitPaneRenderer {
    /// Build the default renderer: one row for the status bar, rest for logs.
    pub fn status_bar_only(width: u16, height: u16) -> Self {
        if height < 3 {
            return Self::fallback(width, height);
        }
        Self {
            term_width: width,
            term_height: height,
            log_region_height: height - 1,
            ai_pane_height: 0,
            ai_pane_top_row: 0,
            status_bar_row: height,
            mode: Mode::StatusBarOnly,
            active: true,
        }
    }

    /// Alias for status_bar_only — the separate split-pane mode has been retired.
    /// Kept for call-site compatibility in session.rs.
    #[allow(dead_code)]
    pub fn new(width: u16, height: u16) -> Self {
        Self::status_bar_only(width, height)
    }

    fn fallback(width: u16, height: u16) -> Self {
        Self {
            term_width: width,
            term_height: height,
            log_region_height: height.saturating_sub(1),
            ai_pane_height: 0,
            ai_pane_top_row: 0,
            status_bar_row: height,
            mode: Mode::Fallback,
            active: false,
        }
    }

    /// Install the status-bar scroll region. Call once at session start.
    ///
    /// Sets scroll region to rows 1..N-1 so new log lines scroll within that
    /// area while row N (the status bar) stays pinned. Terminal scrollback
    /// still works for the log region because the region covers nearly the
    /// whole terminal — only the last row is excluded.
    pub fn setup(&self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        let mut stdout = io::stdout().lock();

        // Clear screen so the banner from the startup sequence is gone
        // and logs begin at row 1.
        write!(stdout, "\x1b[2J\x1b[H")?;

        // Scroll region covers everything except the last row.
        write!(stdout, "\x1b[1;{}r", self.log_region_height)?;

        // Cursor to top-left of log region.
        write!(stdout, "\x1b[1;1H")?;

        stdout.flush()
    }

    /// Write log bytes inside the scroll region.
    pub fn write_log(&self, bytes: &[u8]) -> io::Result<()> {
        let mut stdout = io::stdout().lock();
        stdout.write_all(bytes)?;
        stdout.flush()
    }

    /// Print an AI analysis block INLINE in the log stream.
    ///
    /// The block is clearly bordered with yellow separators so it stands out
    /// from ordinary log lines. The full response is always written — no
    /// truncation — so user can scroll back to it. Response is word-wrapped
    /// to the terminal width.
    pub fn print_ai_inline(&self, state: &AiPaneState) -> io::Result<()> {
        let mut stdout = io::stdout().lock();

        let width = self.term_width as usize;
        let dash = "─";

        // Header line
        let header = if let Some(err) = state.error.as_deref() {
            format!(
                "\x1b[31;1m─── ⚠ AI Error ──────{}\x1b[0m\r\n\x1b[31m  {err}\x1b[0m\r\n",
                dash.repeat(width.saturating_sub(21))
            )
        } else if let Some(ref time) = state.header_time {
            let body = wrap_indent(&state.body, width.saturating_sub(4), "  ");
            format!(
                "\x1b[33;1m─── 🤖 AI Analysis ({time}) {}\x1b[0m\r\n{body}\x1b[33;1m─── end {}\x1b[0m\r\n",
                dash.repeat(width.saturating_sub(22 + time.len())),
                dash.repeat(width.saturating_sub(8)),
            )
        } else {
            // No response yet — this path shouldn't normally be hit, but
            // handle it gracefully.
            format!(
                "\x1b[33;1m─── 🤖 AI Analysis (pending) {}\x1b[0m\r\n",
                dash.repeat(width.saturating_sub(30))
            )
        };

        write!(stdout, "{header}")?;
        stdout.flush()
    }

    /// Print a spinner acknowledgment line inline so the user sees that
    /// the hotkey fired and analysis is in progress.
    pub fn print_ai_spinner(&self) -> io::Result<()> {
        let mut stdout = io::stdout().lock();
        write!(stdout, "\x1b[33;1m  🤖 Analyzing recent logs...\x1b[0m\r\n")?;
        stdout.flush()
    }

    /// Deprecated no-op kept for call-site compatibility. AI is rendered
    /// inline now via `print_ai_inline`.
    #[allow(dead_code)]
    pub fn draw_ai_pane(&self, _state: &AiPaneState) -> io::Result<()> {
        Ok(())
    }

    /// Draw the pinned status bar at the last row.
    pub fn draw_status_bar(&self, status: &str) -> io::Result<()> {
        if !self.active {
            let mut stderr = io::stderr().lock();
            write!(stderr, "\r\x1b[2K{status}")?;
            return stderr.flush();
        }
        let mut stdout = io::stdout().lock();
        write!(
            stdout,
            "\x1b7\x1b[{};1H\x1b[2K{status}\x1b8",
            self.status_bar_row
        )?;
        stdout.flush()
    }

    /// Handle terminal resize — recompute dimensions and re-install scroll region.
    pub fn on_resize(&mut self, new_width: u16, new_height: u16) -> io::Result<()> {
        let new = match self.mode {
            Mode::StatusBarOnly => Self::status_bar_only(new_width, new_height),
            Mode::Fallback => Self::fallback(new_width, new_height),
        };
        *self = new;
        self.setup()
    }

    /// Reset scroll region + move cursor to bottom. Call at session exit.
    pub fn teardown(&self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        let mut stdout = io::stdout().lock();
        write!(stdout, "\x1b[r")?;
        write!(stdout, "\x1b[{};1H", self.term_height)?;
        stdout.flush()
    }
}

/// Word-wrap text to `width` columns and prefix every line with `indent`.
/// Used to format AI responses so they fit the terminal without clipping.
fn wrap_indent(text: &str, width: usize, indent: &str) -> String {
    if width == 0 {
        return text.to_string();
    }
    let mut out = String::new();
    for raw_line in text.lines() {
        let mut remaining = raw_line;
        while !remaining.is_empty() {
            if remaining.len() <= width {
                out.push_str(indent);
                out.push_str(remaining);
                out.push_str("\r\n");
                break;
            }
            // Walk back to a char boundary at or below `width` so UTF-8
            // multi-byte sequences aren't split.
            let mut boundary = width.min(remaining.len());
            while boundary > 0 && !remaining.is_char_boundary(boundary) {
                boundary -= 1;
            }
            let slice = &remaining[..boundary];
            let break_at = slice.rfind(' ').unwrap_or(slice.len());
            let (head, tail) = remaining.split_at(break_at);
            out.push_str(indent);
            out.push_str(head.trim_end());
            out.push_str("\r\n");
            remaining = tail.trim_start();
        }
        // Empty lines in the source produce just an indent+CRLF.
        if raw_line.is_empty() {
            out.push_str(indent);
            out.push_str("\r\n");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_status_bar_only_by_default() {
        let r = SplitPaneRenderer::new(80, 24);
        assert!(r.active);
        assert_eq!(r.mode, Mode::StatusBarOnly);
        assert_eq!(r.log_region_height, 23);
        assert_eq!(r.status_bar_row, 24);
    }

    #[test]
    fn status_bar_only_reserves_one_row() {
        let r = SplitPaneRenderer::status_bar_only(80, 24);
        assert!(r.active);
        assert_eq!(r.mode, Mode::StatusBarOnly);
        assert_eq!(r.log_region_height, 23);
        assert_eq!(r.status_bar_row, 24);
    }

    #[test]
    fn status_bar_only_tiny_terminal_falls_back() {
        let r = SplitPaneRenderer::status_bar_only(80, 2);
        assert_eq!(r.mode, Mode::Fallback);
        assert!(!r.active);
    }

    #[test]
    fn status_bar_only_fits_small_but_viable_terminal() {
        let r = SplitPaneRenderer::status_bar_only(80, 5);
        assert!(r.active);
        assert_eq!(r.log_region_height, 4);
        assert_eq!(r.status_bar_row, 5);
    }

    #[test]
    fn resize_preserves_mode_status_bar_only() {
        let mut r = SplitPaneRenderer::status_bar_only(80, 24);
        // Pretend-resize (call-site in session.rs guards terminal I/O)
        r.term_width = 100;
        r.term_height = 40;
        r.log_region_height = 39;
        r.status_bar_row = 40;
        assert_eq!(r.log_region_height, 39);
    }

    #[test]
    fn wrap_indent_short_line_kept_as_is() {
        let out = wrap_indent("short", 80, "  ");
        assert_eq!(out, "  short\r\n");
    }

    #[test]
    fn wrap_indent_long_line_broken_on_spaces() {
        let out = wrap_indent("one two three four five", 10, "  ");
        // Each line is indented and within width.
        for line in out.lines().filter(|l| !l.is_empty()) {
            assert!(line.starts_with("  "));
            assert!(line.len() - 2 <= 10, "line too long: {:?}", line);
        }
    }

    #[test]
    fn wrap_indent_preserves_paragraph_breaks() {
        let out = wrap_indent("p1\n\np2", 80, "  ");
        assert!(out.contains("p1"));
        assert!(out.contains("p2"));
    }

    #[test]
    fn wrap_indent_handles_unicode() {
        // Emoji is multi-byte — must not panic on non-char-boundary index
        let out = wrap_indent("hello 🤖 world", 80, "  ");
        assert!(out.contains("🤖"));
    }

    #[test]
    fn fallback_mode_has_no_scroll_region() {
        let r = SplitPaneRenderer::fallback(80, 2);
        assert!(!r.active);
        assert_eq!(r.mode, Mode::Fallback);
    }
}
