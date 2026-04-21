//! Keyboard input translation and hotkey dispatch.
//!
//! `HotkeyDispatcher` extends the original Ctrl+A Ctrl+X exit state machine
//! with AI hotkeys: Ctrl+A A (analyze), Ctrl+A Q (question), Ctrl+A L (last).
//! When AI is disabled, the AI hotkeys fall through as forwarded bytes.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

const CTRL_A: u8 = 0x01;
const CTRL_X: u8 = 0x18;

/// Actions the hotkey dispatcher can produce.
#[derive(Debug, PartialEq, Eq)]
pub enum HotkeyAction {
    /// Normal bytes to forward to the serial port.
    Forward(Vec<u8>),
    /// Ctrl+A X — terminate the session.
    Exit,
    /// Ctrl+A A — trigger AI analysis of recent logs.
    Analyze,
    /// Ctrl+A Q — open the custom question prompt.
    AskQuestion,
    /// Ctrl+A L — show the last full AI response.
    ShowLastResponse,
    /// Non-key event or empty input, ignore.
    Continue,
}

/// Prefix-dispatch state machine for Ctrl+A hotkeys.
///
/// When `ai_enabled` is false, only Ctrl+A X (exit) is recognized;
/// A/Q/L fall through as forwarded bytes.
#[derive(Debug)]
pub struct HotkeyDispatcher {
    armed: bool,
    ai_enabled: bool,
}

impl HotkeyDispatcher {
    pub fn new(ai_enabled: bool) -> Self {
        Self {
            armed: false,
            ai_enabled,
        }
    }

    /// Feed a byte slice through the state machine.
    pub fn feed(&mut self, bytes: &[u8]) -> HotkeyAction {
        if bytes.is_empty() {
            return HotkeyAction::Continue;
        }
        let mut out = Vec::with_capacity(bytes.len() + 1);
        for &b in bytes {
            if self.armed {
                self.armed = false;
                match b {
                    CTRL_X => return HotkeyAction::Exit,
                    b'a' | b'A' if self.ai_enabled => return HotkeyAction::Analyze,
                    b'q' | b'Q' if self.ai_enabled => return HotkeyAction::AskQuestion,
                    b'l' | b'L' if self.ai_enabled => return HotkeyAction::ShowLastResponse,
                    _ => {
                        // Not a recognized hotkey — forward both bytes
                        out.push(CTRL_A);
                        out.push(b);
                    }
                }
            } else if b == CTRL_A {
                self.armed = true;
            } else {
                out.push(b);
            }
        }
        if out.is_empty() {
            HotkeyAction::Continue
        } else {
            HotkeyAction::Forward(out)
        }
    }
}

// Backward compat aliases (retained for external consumers)
#[allow(dead_code)]
pub type ForwardOutcome = HotkeyAction;
#[allow(dead_code)]
pub type ExitStateMachine = HotkeyDispatcher;

/// Translate a crossterm `Event` into the bytes a serial device expects.
pub fn event_to_bytes(event: &Event) -> Vec<u8> {
    match event {
        Event::Key(key) => key_event_to_bytes(key),
        _ => Vec::new(),
    }
}

pub fn key_event_to_bytes(key: &KeyEvent) -> Vec<u8> {
    match key.code {
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => vec![0x1b, b'[', b'A'],
        KeyCode::Down => vec![0x1b, b'[', b'B'],
        KeyCode::Right => vec![0x1b, b'[', b'C'],
        KeyCode::Left => vec![0x1b, b'[', b'D'],
        KeyCode::Home => vec![0x1b, b'[', b'H'],
        KeyCode::End => vec![0x1b, b'[', b'F'],
        KeyCode::Delete => vec![0x1b, b'[', b'3', b'~'],
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let byte = (c.to_ascii_lowercase() as u8) & 0x1f;
                vec![byte]
            } else {
                let mut buf = [0u8; 4];
                c.encode_utf8(&mut buf).as_bytes().to_vec()
            }
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_bytes_pass_through() {
        let mut d = HotkeyDispatcher::new(true);
        assert_eq!(d.feed(b"hello"), HotkeyAction::Forward(b"hello".to_vec()));
    }

    #[test]
    fn ctrl_a_alone_arms_no_output() {
        let mut d = HotkeyDispatcher::new(true);
        assert_eq!(d.feed(&[CTRL_A]), HotkeyAction::Continue);
        // Subsequent non-hotkey byte flushes both bytes.
        assert_eq!(d.feed(b"z"), HotkeyAction::Forward(vec![CTRL_A, b'z']));
    }

    #[test]
    fn ctrl_a_ctrl_x_exits() {
        let mut d = HotkeyDispatcher::new(true);
        assert_eq!(d.feed(&[CTRL_A, CTRL_X]), HotkeyAction::Exit);
    }

    #[test]
    fn ctrl_a_a_analyzes_when_ai_enabled() {
        let mut d = HotkeyDispatcher::new(true);
        assert_eq!(d.feed(&[CTRL_A, b'a']), HotkeyAction::Analyze);
        let mut d = HotkeyDispatcher::new(true);
        assert_eq!(d.feed(&[CTRL_A, b'A']), HotkeyAction::Analyze);
    }

    #[test]
    fn ctrl_a_q_asks_question_when_ai_enabled() {
        let mut d = HotkeyDispatcher::new(true);
        assert_eq!(d.feed(&[CTRL_A, b'q']), HotkeyAction::AskQuestion);
    }

    #[test]
    fn ctrl_a_l_shows_last_response_when_ai_enabled() {
        let mut d = HotkeyDispatcher::new(true);
        assert_eq!(d.feed(&[CTRL_A, b'l']), HotkeyAction::ShowLastResponse);
    }

    #[test]
    fn ai_hotkeys_fall_through_when_disabled() {
        let mut d = HotkeyDispatcher::new(false);
        assert_eq!(
            d.feed(&[CTRL_A, b'a']),
            HotkeyAction::Forward(vec![CTRL_A, b'a'])
        );
    }

    #[test]
    fn ctrl_a_x_still_exits_when_ai_disabled() {
        let mut d = HotkeyDispatcher::new(false);
        assert_eq!(d.feed(&[CTRL_A, CTRL_X]), HotkeyAction::Exit);
    }

    #[test]
    fn armed_state_resets_after_non_hotkey() {
        let mut d = HotkeyDispatcher::new(true);
        // Arm with Ctrl+A, flush with 'z' — 'z' is NOT a hotkey so it emits [CTRL_A,'z'].
        assert_eq!(
            d.feed(&[CTRL_A, b'z']),
            HotkeyAction::Forward(vec![CTRL_A, b'z'])
        );
        // Now disarmed — a plain 'z' should just forward as-is.
        assert_eq!(d.feed(b"z"), HotkeyAction::Forward(b"z".to_vec()));
    }

    #[test]
    fn empty_input_is_continue() {
        let mut d = HotkeyDispatcher::new(true);
        assert_eq!(d.feed(&[]), HotkeyAction::Continue);
    }
}
