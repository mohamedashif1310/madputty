//! Split-pane terminal renderer using ANSI scroll regions.
//!
//! Top region (~80%): live log stream in a scroll region.
//! Bottom region (~20%, min 6 rows): AI pane drawn by cursor positioning.
//! Last row: status bar.

use std::io::{self, Write};

use crate::ai::pane::AiPaneState;

const MIN_AI_PANE_HEIGHT: u16 = 6;
const MIN_TERMINAL_HEIGHT: u16 = 12;

pub struct SplitPaneRenderer {
    pub term_width: u16,
    pub term_height: u16,
    pub log_region_height: u16,
    pub ai_pane_height: u16,
    pub ai_pane_top_row: u16,
    pub status_bar_row: u16,
    pub active: bool, // false in fallback mode
}

impl SplitPaneRenderer {
    /// Create and set up the split pane. Returns None if terminal too small.
    pub fn new(width: u16, height: u16) -> Self {
        if height < MIN_TERMINAL_HEIGHT {
            return Self {
                term_width: width,
                term_height: height,
                log_region_height: height.saturating_sub(1),
                ai_pane_height: 0,
                ai_pane_top_row: 0,
                status_bar_row: height,
                active: false,
            };
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
            active: true,
        }
    }

    /// Set ANSI scroll region to confine log output to the top region.
    pub fn setup(&self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        let mut stdout = io::stdout().lock();
        // Set scroll region: rows 1 through log_region_height
        write!(stdout, "\x1b[1;{}r", self.log_region_height)?;
        // Move cursor to top-left of scroll region
        write!(stdout, "\x1b[1;1H")?;
        stdout.flush()?;

        // Draw initial AI pane separator
        self.draw_separator(&mut stdout)?;
        Ok(())
    }

    /// Write bytes to the log region (within the scroll region).
    /// The terminal handles scrolling automatically.
    pub fn write_log(&self, bytes: &[u8]) -> io::Result<()> {
        let mut stdout = io::stdout().lock();
        if self.active {
            // Save cursor, ensure we're in the scroll region, write, restore
            // Actually: since scroll region is set, normal writes go there.
            stdout.write_all(bytes)?;
            stdout.flush()?;
        } else {
            stdout.write_all(bytes)?;
            stdout.flush()?;
        }
        Ok(())
    }

    /// Redraw the AI pane content.
    pub fn draw_ai_pane(&self, state: &AiPaneState) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        let mut stdout = io::stdout().lock();

        // Save cursor position
        write!(stdout, "\x1b7")?;

        // Move to AI pane area (below scroll region)
        let content_start = self.ai_pane_top_row + 1; // +1 for separator

        // Draw header
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
                "\x1b[{};1H\x1b[2K\x1b[33;1m🤖 AI Analysis (waiting)\x1b[0m",
                content_start
            )
        };
        write!(stdout, "{header}")?;

        // Draw body or error
        let body_start = content_start + 1;
        let body_rows = self.ai_pane_height.saturating_sub(2) as usize; // -1 separator, -1 header

        if let Some(ref err) = state.error {
            write!(
                stdout,
                "\x1b[{};1H\x1b[2K\x1b[31;1m⚠ {err}\x1b[0m",
                body_start
            )?;
            // Clear remaining rows
            for row in 1..body_rows {
                write!(stdout, "\x1b[{};1H\x1b[2K", body_start + row as u16)?;
            }
        } else if !state.body.is_empty() {
            let lines: Vec<&str> = state.body.lines().collect();
            for (i, line) in lines.iter().take(body_rows).enumerate() {
                let row = body_start + i as u16;
                // Truncate line to terminal width
                let display: String = line.chars().take(self.term_width as usize - 1).collect();
                write!(stdout, "\x1b[{row};1H\x1b[2K{display}")?;
            }
            // If truncated, show hint on last row
            if lines.len() > body_rows {
                let last_row = body_start + body_rows as u16 - 1;
                write!(
                    stdout,
                    "\x1b[{last_row};1H\x1b[2K\x1b[33m... (press Ctrl+A L for full)\x1b[0m"
                )?;
            }
            // Clear unused rows
            for i in lines.len().min(body_rows)..body_rows {
                let row = body_start + i as u16;
                write!(stdout, "\x1b[{row};1H\x1b[2K")?;
            }
        } else {
            // Empty pane
            for i in 0..body_rows {
                let row = body_start + i as u16;
                write!(stdout, "\x1b[{row};1H\x1b[2K")?;
            }
        }

        // Restore cursor position (back to scroll region)
        write!(stdout, "\x1b8")?;
        stdout.flush()?;
        Ok(())
    }

    /// Draw the status bar on the last row.
    pub fn draw_status_bar(&self, status: &str) -> io::Result<()> {
        let mut stderr = io::stderr().lock();
        write!(
            stderr,
            "\x1b7\x1b[{};1H\x1b[2K{status}\x1b8",
            self.status_bar_row
        )?;
        stderr.flush()?;
        Ok(())
    }

    /// Handle terminal resize.
    pub fn on_resize(&mut self, new_width: u16, new_height: u16) -> io::Result<()> {
        let new = Self::new(new_width, new_height);
        *self = new;
        self.setup()
    }

    /// Reset scroll region to full terminal on exit.
    pub fn teardown(&self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        let mut stdout = io::stdout().lock();
        write!(stdout, "\x1b[r")?; // Reset scroll region
        write!(stdout, "\x1b[{};1H", self.term_height)?; // Move to bottom
        stdout.flush()?;
        Ok(())
    }

    fn draw_separator(&self, stdout: &mut impl Write) -> io::Result<()> {
        let sep_row = self.ai_pane_top_row;
        let line = "─".repeat(self.term_width as usize);
        write!(stdout, "\x1b[{sep_row};1H\x1b[33m{line}\x1b[0m")?;
        stdout.flush()?;
        Ok(())
    }
}
