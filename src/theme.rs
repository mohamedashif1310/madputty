//! Amazon-palette visual aesthetics: black / white / smile-yellow.
//!
//! Colors:
//!   - Amazon smile yellow (#FF9900) → rendered as ANSI bright yellow
//!   - White → ANSI bright white
//!   - Black/dim gray for secondary text
//!
//! Includes an ASCII madputty wordmark with an Amazon-style smile/arrow
//! underline, and a tiny Amazon box mascot glyph.

use console::{Style, Term};
use std::io::Write;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// madputty wordmark in solid filled blocks — reliable across Windows terminals.
pub const WORDMARK_LINES: &[&str] = &[
    "  ███╗   ███╗ █████╗ ██████╗ ██████╗ ██╗   ██╗████████╗████████╗██╗   ██╗",
    "  ████╗ ████║██╔══██╗██╔══██╗██╔══██╗██║   ██║╚══██╔══╝╚══██╔══╝╚██╗ ██╔╝",
    "  ██╔████╔██║███████║██║  ██║██████╔╝██║   ██║   ██║      ██║    ╚████╔╝ ",
    "  ██║╚██╔╝██║██╔══██║██║  ██║██╔═══╝ ██║   ██║   ██║      ██║     ╚██╔╝  ",
    "  ██║ ╚═╝ ██║██║  ██║██████╔╝██║     ╚██████╔╝   ██║      ██║      ██║   ",
    "  ╚═╝     ╚═╝╚═╝  ╚═╝╚═════╝ ╚═╝      ╚═════╝    ╚═╝      ╚═╝      ╚═╝   ",
];

/// The Amazon-style smile arrow underneath — yellow, curved from M to Y.
pub const SMILE_LINES: &[&str] = &[
    "          ╲___________________________________________________╱",
    "           ╲_________________________________________________╱ ",
];

pub const TAGLINE: &str = "serial terminal · amazon edition";

/// Tiny Amazon box mascot (stylized) printed next to the wordmark.
pub const BOX_MASCOT: &[&str] = &[
    "  ┌─────────┐  ",
    "  │  a͞mazon │  ",
    "  │   ╲___╱  │  ",
    "  └─────────┘  ",
];

/// Color styles. Amazon palette: yellow accents on white text, dim gray for
/// metadata, red for errors.
pub struct Palette {
    pub logo_white: Style,
    pub logo_yellow: Style,
    pub tagline: Style,
    pub border: Style,
    pub label: Style,
    pub value: Style,
    pub key_hint: Style,
    pub success: Style,
    pub error: Style,
    pub dim: Style,

    // Log-line colorization
    pub log_timestamp: Style,
    pub log_task_id: Style,
    pub log_level_info: Style,
    pub log_level_warn: Style,
    pub log_level_error: Style,
    pub log_module: Style,
    pub log_number: Style,
    pub log_prompt: Style,
    pub log_keyword_error: Style,
    pub log_keyword_warn: Style,
    pub log_keyword_ok: Style,
}

impl Palette {
    /// Amazon black/white/yellow palette.
    pub fn amazon() -> Self {
        Self {
            logo_white: Style::new().white().bright().bold(),
            logo_yellow: Style::new().yellow().bright().bold(),
            tagline: Style::new().white().dim(),
            border: Style::new().yellow().bright(),
            label: Style::new().yellow().bright(),
            value: Style::new().white().bright().bold(),
            key_hint: Style::new().yellow().bright().bold(),
            success: Style::new().yellow().bright().bold(),
            error: Style::new().red().bright().bold(),
            dim: Style::new().white().dim(),

            log_timestamp: Style::new().white().dim(),
            log_task_id: Style::new().white().dim(),
            log_level_info: Style::new().white().bright(),
            log_level_warn: Style::new().yellow().bright().bold(),
            log_level_error: Style::new().red().bright().bold(),
            log_module: Style::new().yellow().bright(),
            log_number: Style::new().white().bright().bold(),
            log_prompt: Style::new().yellow().bright().bold(),
            log_keyword_error: Style::new().red().bright().bold(),
            log_keyword_warn: Style::new().yellow().bright().bold(),
            log_keyword_ok: Style::new().yellow().bright().bold(),
        }
    }

    /// No-color palette for --plain.
    pub fn plain() -> Self {
        let s = Style::new();
        Self {
            logo_white: s.clone(),
            logo_yellow: s.clone(),
            tagline: s.clone(),
            border: s.clone(),
            label: s.clone(),
            value: s.clone(),
            key_hint: s.clone(),
            success: s.clone(),
            error: s.clone(),
            dim: s.clone(),
            log_timestamp: s.clone(),
            log_task_id: s.clone(),
            log_level_info: s.clone(),
            log_level_warn: s.clone(),
            log_level_error: s.clone(),
            log_module: s.clone(),
            log_number: s.clone(),
            log_prompt: s.clone(),
            log_keyword_error: s.clone(),
            log_keyword_warn: s.clone(),
            log_keyword_ok: s,
        }
    }
}

/// Print the Amazon-style banner: wordmark (white) + smile arrow (yellow)
/// + tagline + session info box.
pub fn print_banner(port: &str, baud: u32, framing: &str, palette: &Palette) {
    let term = Term::stdout();
    let _ = term.write_line("");

    // White wordmark
    for line in WORDMARK_LINES {
        let _ = term.write_line(&palette.logo_white.apply_to(*line).to_string());
    }

    // Yellow smile arrow
    for line in SMILE_LINES {
        let _ = term.write_line(&palette.logo_yellow.apply_to(*line).to_string());
    }

    let _ = term.write_line(&format!("  {}", palette.tagline.apply_to(TAGLINE)));
    let _ = term.write_line("");

    // Info box in yellow border, white values.
    let top = "  ╭─────────────────────────────────────────────────────────╮";
    let bot = "  ╰─────────────────────────────────────────────────────────╯";
    let _ = term.write_line(&palette.border.apply_to(top).to_string());

    let edge_l = palette.border.apply_to("  │");
    let edge_r = palette.border.apply_to("│");

    let mut row = |label: &str, value: &str| {
        let body = format!(
            " {} {} ",
            palette.label.apply_to(format!("{:<9}", label)),
            palette.value.apply_to(format!("{:<47}", value))
        );
        let _ = term.write_line(&format!("{}{}{}", edge_l, body, edge_r));
    };

    row("PORT", port);
    row("BAUD", &format!("{}", baud));
    row("FRAMING", framing);

    let exit_hint = format!(
        "Press {} then {} to exit",
        palette.key_hint.apply_to("Ctrl+A"),
        palette.key_hint.apply_to("Ctrl+X"),
    );
    let _ = term.write_line(&format!("{} {:<57} {}", edge_l, exit_hint, edge_r));

    let _ = term.write_line(&palette.border.apply_to(bot).to_string());
    let _ = term.write_line("");
}

/// Typewriter-effect print. Writes each character with a short delay.
pub fn typewriter(text: &str, style: &Style, per_char: Duration, plain: bool) {
    let mut stdout = std::io::stdout().lock();
    if plain {
        let _ = writeln!(stdout, "{}", style.apply_to(text));
        return;
    }
    for ch in text.chars() {
        let _ = write!(stdout, "{}", style.apply_to(ch));
        let _ = stdout.flush();
        sleep(per_char);
    }
    let _ = writeln!(stdout);
}

/// Boot sequence: four amber bullets appearing with slight delays.
pub fn boot_sequence(palette: &Palette, plain: bool) {
    if plain {
        return;
    }
    let steps = [
        ("opening port", 120),
        ("negotiating framing", 80),
        ("engaging raw mode", 80),
        ("streaming", 80),
    ];
    let mut stdout = std::io::stdout().lock();
    for (msg, ms) in steps {
        let _ = writeln!(
            stdout,
            "  {} {}",
            palette.logo_yellow.apply_to("▸"),
            palette.value.apply_to(msg)
        );
        let _ = stdout.flush();
        sleep(Duration::from_millis(ms));
    }
}

/// Format a byte count in human-friendly units.
pub fn humanize_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * 1024 * 1024;
    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format an elapsed duration as `H:MM:SS`.
pub fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h}:{m:02}:{s:02}")
}

/// Final session summary card — yellow border, white values.
pub fn print_session_summary(
    port: &str,
    baud: u32,
    bytes_rx: u64,
    bytes_tx: u64,
    started: Instant,
    palette: &Palette,
) {
    let elapsed = started.elapsed();
    let top = "  ╭─────────────────────────────────────────────────────────╮";
    let bot = "  ╰─────────────────────────────────────────────────────────╯";
    let edge_l = palette.border.apply_to("  │");
    let edge_r = palette.border.apply_to("│");

    println!();
    println!("{}", palette.border.apply_to(top));
    let title = palette
        .logo_yellow
        .apply_to("                 SESSION CLOSED                ");
    println!("{} {} {}", edge_l, title, edge_r);
    println!("{} {:<57} {}", edge_l, "", edge_r);

    let rows = [
        ("PORT", port.to_string()),
        ("BAUD", baud.to_string()),
        ("DURATION", format_elapsed(elapsed)),
        ("RECEIVED", humanize_bytes(bytes_rx)),
        ("SENT", humanize_bytes(bytes_tx)),
    ];
    for (label, value) in rows {
        let body = format!(
            " {} {} ",
            palette.label.apply_to(format!("{:<9}", label)),
            palette.value.apply_to(format!("{:<47}", value))
        );
        println!("{}{}{}", edge_l, body, edge_r);
    }
    println!("{}", palette.border.apply_to(bot));
    println!();
}
