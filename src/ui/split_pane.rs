//! Terminal renderer using ANSI scroll regions.
//!
//! Two modes are supported:
//!
//! - **Status-bar mode** (default): reserves ONLY the last terminal row for a
//!   fixed status bar. The log occupies rows 1..N-1 as a scroll region. The
//!   terminal's native scrollback buffer continues to work for the log region
//!   — users can scroll up with PgUp / mouse wheel to see earlier lines.
//!
//! - **Split-pane mode** (opt-in via `--split-pane`): carves the terminal into
//!   three regions — log (top ~80%), AI pane (~20%), status bar (last row).
//!   This mode disables terminal scrollback for the log region because the
//!   scroll region excludes the AI pane rows.
//!
//! All output goes through the renderer so sequences interleave correctly:
//! the status bar never gets mangled by fast log output at high baud rates.

use std::io::{self, Write};

use crate::ai::pane::AiPaneState;

const MIN_AI_PANE_HEIGHT: u16 = 6;
const MIN_TERMINAL_HEIGHT_FOR_SPLIT: u16 = 12;

/// Which rendering layout is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Only the status bar is pinned. No AI pane.
    StatusBarOnly,
    /// Full split: log + AI pane + status bar.
    SplitPane,
    /// Terminal too small for any fancy layout — everything inline.
    Fallback,
}

pub struct SplitPaneRenderer {
    pub term_width: u16,
    pub term_height: u16,
    pub log_region_height: u16,
    pub ai_pane_height: u16,
    pub ai_pane_top_row: u16,
    pub status_bar_row: u16,
    pub mode: Mode,
    /// True when the renderer is managing a scroll region (mode != Fallback).
    pub active: bool,
}

impl SplitPaneRenderer {
    /// Build the default status-bar-only renderer. This is what you want when
    /// you just need a pinned status bar and want to keep native scrollback.
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

    /// Build the full split-pane renderer (log + AI pane + status bar).
    /// Falls back to `Fallback` mode if the terminal is too small.
    pub fn new(width: u16, height: u16) -> Self {
        if height < MIN_TERMINAL_HEIGHT_FOR_SPLIT {
            return Self::fallback(width, height);
        }

        let ai_pane_height = (height * 20 / 100).max(MIN_AI_PANE_HEIGHT);
        let log_region_height = height - ai_pane_height - 1; // -1 for status bar
        let ai_pane_top_row = log_region_height + 1;
        let status_bar_row = height;

        Self {
            term_width: width,
            term_height: height,
            log_region_height,
            ai_pane_height,
            ai_pane_top_row,
            status_bar_row,
            mode: Mode::SplitPane,
            active: true,
        }
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

    /// Install the ANSI scroll region. Call once at session start.
    pub fn setup(&self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        let mut stdout = io::stdout().lock();
        // Scroll region from row 1 to `log_region_height`. The status bar row
        // (and the AI pane rows, if any) are outside this region so they don't
        // scroll when logs arrive.
        write!(stdout, "\x1b[1;{}r", self.log_region_height)?;
        // Move cursor to top-left of scroll region so logs start at the top.
        write!(stdout, "\x1b[1;1H")?;
        stdout.flush()?;

        if self.mode == Mode::SplitPane {
            self.draw_separator(&mut stdout)?;
        }
        Ok(())
    }

    /// Write log bytes inside the scroll region. Terminal handles scrolling.
    pub fn write_log(&self, bytes: &[u8]) -> io::Result<()> {
        let mut stdout = io::stdout().lock();
        stdout.write_all(bytes)?;
        stdout.flush()
    }

    /// Draw or update the AI pane. No-op outside split-pane mode.
    pub fn draw_ai_pane(&self, state: &AiPaneState) -> io::Result<()> {
        if self.mode != Mode::SplitPane {
            return Ok(());
        }
        let mut stdout = io::stdout().lock();

        // Save cursor, move to pane area, draw, restore.
        write!(stdout, "\x1b7")?;

        let content_start = self.ai_pane_top_row + 1;

        let header = if state.spinner_active {
            format!(
                "\x1b[{};1H\x1b[2K\x1b[33;1m🤖 ⠋ Analyzing...\x1b[0m",
                content_start
            )
        } else if let Some(ref time) = state.header_time {
            format!(
                "\x1b[{};1H\x1b[2K\x1b[33;1m🤖 AI Analysis (updated {time})\x1b[0m",
                content_start
            )
        } else {
            format!(
                "\x1b[{};1H\x1b[2K\x1b[33;1m🤖 AI Analysis (press Ctrl+A A to run)\x1b[0m",
                content_start
            )
        };
        write!(stdout, "{header}")?;

        let body_start = content_start + 1;
        let body_rows = self.ai_pane_height.saturating_sub(2) as usize;

        if let Some(ref err) = state.error {
            write!(
                stdout,
                "\x1b[{};1H\x1b[2K\x1b[31;1m⚠ {err}\x1b[0m",
                body_start
            )?;
            for row in 1..body_rows {
                write!(stdout, "\x1b[{};1H\x1b[2K", body_start + row as u16)?;
            }
        } else if !state.body.is_empty() {
            let lines: Vec<&str> = state.body.lines().collect();
            for (i, line) in lines.iter().take(body_rows).enumerate() {
                let row = body_start + i as u16;
                let display: String = line.chars().take(self.term_width as usize - 1).collect();
                write!(stdout, "\x1b[{row};1H\x1b[2K{display}")?;
            }
            if lines.len() > body_rows {
                let last_row = body_start + body_rows as u16 - 1;
                write!(
                    stdout,
                    "\x1b[{last_row};1H\x1b[2K\x1b[33m... (press Ctrl+A L for full)\x1b[0m"
                )?;
            }
            for i in lines.len().min(body_rows)..body_rows {
                let row = body_start + i as u16;
                write!(stdout, "\x1b[{row};1H\x1b[2K",)?;
            }
        } else {
            for i in 0..body_rows {
                let row = body_start + i as u16;
                write!(stdout, "\x1b[{row};1H\x1b[2K")?;
            }
        }

        write!(stdout, "\x1b8")?;
        stdout.flush()
    }

    /// Draw the pinned status bar at the last row. Uses save-cursor /
    /// restore-cursor so the log pump's cursor position is preserved.
    pub fn draw_status_bar(&self, status: &str) -> io::Result<()> {
        if !self.active {
            // Fallback: just carriage-return and redraw inline (pre-fix behavior).
            let mut stderr = io::stderr().lock();
            write!(stderr, "\r\x1b[2K{status}")?;
            return stderr.flush();
        }
        // Write to stdout (not stderr) so it's in the same stream as logs
        // and the terminal interleaves correctly. Save/restore keeps the
        // log pump's cursor position.
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
        let mode = self.mode;
        let new = match mode {
            Mode::SplitPane => Self::new(new_width, new_height),
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

    fn draw_separator(&self, stdout: &mut impl Write) -> io::Result<()> {
        let sep_row = self.ai_pane_top_row;
        let line = "─".repeat(self.term_width as usize);
        write!(stdout, "\x1b[{sep_row};1H\x1b[33m{line}\x1b[0m")?;
        stdout.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_terminal_80x24_dimensions() {
        let r = SplitPaneRenderer::new(80, 24);
        assert!(r.active);
        assert_eq!(r.mode, Mode::SplitPane);
        assert_eq!(r.ai_pane_height, 6);
        assert_eq!(r.log_region_height, 17);
        assert_eq!(r.status_bar_row, 24);
        assert_eq!(r.ai_pane_top_row, 18);
    }

    #[test]
    fn large_terminal_120x50_dimensions() {
        let r = SplitPaneRenderer::new(120, 50);
        assert!(r.active);
        assert_eq!(r.ai_pane_height, 10);
        assert_eq!(r.log_region_height, 39);
        assert_eq!(r.status_bar_row, 50);
        assert_eq!(r.ai_pane_top_row, 40);
        assert_eq!(r.term_width, 120);
    }

    #[test]
    fn small_terminal_80x12_minimum_ai_pane_height() {
        let r = SplitPaneRenderer::new(80, 12);
        assert!(r.active);
        assert_eq!(r.ai_pane_height, 6);
        assert_eq!(r.log_region_height, 5);
        assert_eq!(r.status_bar_row, 12);
    }

    #[test]
    fn very_small_terminal_80x10_fallback_mode() {
        let r = SplitPaneRenderer::new(80, 10);
        assert!(!r.active);
        assert_eq!(r.mode, Mode::Fallback);
        assert_eq!(r.ai_pane_height, 0);
    }

    #[test]
    fn ai_pane_height_formula() {
        for height in 12..=100u16 {
            let r = SplitPaneRenderer::new(80, height);
            let expected = (height * 20 / 100).max(MIN_AI_PANE_HEIGHT);
            assert_eq!(r.ai_pane_height, expected, "failed for height={height}");
        }
    }

    #[test]
    fn log_region_height_formula() {
        for height in 12..=100u16 {
            let r = SplitPaneRenderer::new(80, height);
            let expected = height - r.ai_pane_height - 1;
            assert_eq!(r.log_region_height, expected, "failed for height={height}");
        }
    }

    #[test]
    fn status_bar_only_reserves_one_row() {
        let r = SplitPaneRenderer::status_bar_only(80, 24);
        assert!(r.active);
        assert_eq!(r.mode, Mode::StatusBarOnly);
        assert_eq!(r.log_region_height, 23);
        assert_eq!(r.status_bar_row, 24);
        assert_eq!(r.ai_pane_height, 0);
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
}
