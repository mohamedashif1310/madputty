//! Keyboard input translation and Ctrl+A Ctrl+X exit state machine.
//!
//! The state machine implements Requirement 4: Ctrl+A (0x01) alone arms, a
//! subsequent Ctrl+X (0x18) requests exit, any other subsequent byte forwards
//! both the held Ctrl+A and the new byte in order.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

const CTRL_A: u8 = 0x01;
const CTRL_X: u8 = 0x18;

#[derive(Debug, PartialEq, Eq)]
pub enum ForwardOutcome {
    Bytes(Vec<u8>),
    ExitRequested,
    Continue,
}

#[derive(Debug, Default)]
pub struct ExitStateMachine {
    armed: bool,
}

impl ExitStateMachine {
    pub fn new() -> Self {
        Self { armed: false }
    }

    /// Feed a byte slice through the state machine. Returns the bytes that
    /// should be forwarded to the port, or `ExitRequested` when the
    /// Ctrl+A Ctrl+X sequence completes.
    pub fn feed(&mut self, bytes: &[u8]) -> ForwardOutcome {
        if bytes.is_empty() {
            return ForwardOutcome::Continue;
        }
        let mut out = Vec::with_capacity(bytes.len() + 1);
        for &b in bytes {
            if self.armed {
                self.armed = false;
                if b == CTRL_X {
                    return ForwardOutcome::ExitRequested;
                }
                out.push(CTRL_A);
                out.push(b);
            } else if b == CTRL_A {
                self.armed = true;
            } else {
                out.push(b);
            }
        }
        if out.is_empty() {
            ForwardOutcome::Continue
        } else {
            ForwardOutcome::Bytes(out)
        }
    }
}

/// Translate a crossterm `Event` into the bytes a serial device expects.
/// Non-key events (resize, focus, paste, mouse) return an empty vector.
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
