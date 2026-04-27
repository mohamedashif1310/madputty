//! AI pane state management.
//!
//! Holds the current AI response, spinner state, error messages, and modal
//! overlay state. The renderer reads this to draw the AI pane region.

pub struct AiPaneState {
    pub header_time: Option<String>,
    pub body: String,
    pub body_truncated: bool,
    pub spinner_active: bool,
    pub error: Option<String>,
    #[allow(dead_code)]
    pub modal_open: bool,
    #[allow(dead_code)]
    pub modal_scroll_offset: usize,
}

impl AiPaneState {
    pub fn new() -> Self {
        Self {
            header_time: None,
            body: String::new(),
            body_truncated: false,
            spinner_active: false,
            error: None,
            modal_open: false,
            modal_scroll_offset: 0,
        }
    }

    pub fn set_response(&mut self, text: String, time: String) {
        self.body = text;
        self.header_time = Some(time);
        self.spinner_active = false;
        self.error = None;
        self.body_truncated = false;
    }

    pub fn set_error(&mut self, msg: String) {
        self.error = Some(msg);
        self.spinner_active = false;
    }

    pub fn set_spinner(&mut self, active: bool) {
        self.spinner_active = active;
    }

    #[allow(dead_code)]
    pub fn open_modal(&mut self) {
        self.modal_open = true;
        self.modal_scroll_offset = 0;
    }

    #[allow(dead_code)]
    pub fn close_modal(&mut self) {
        self.modal_open = false;
    }

    #[allow(dead_code)]
    pub fn scroll_modal(&mut self, delta: isize) {
        let new_offset = self.modal_scroll_offset as isize + delta;
        self.modal_scroll_offset = new_offset.max(0) as usize;
    }

    pub fn has_response(&self) -> bool {
        !self.body.is_empty()
    }
}

impl Default for AiPaneState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_is_empty() {
        let p = AiPaneState::new();
        assert!(p.header_time.is_none());
        assert!(p.body.is_empty());
        assert!(!p.spinner_active);
        assert!(p.error.is_none());
        assert!(!p.modal_open);
        assert_eq!(p.modal_scroll_offset, 0);
        assert!(!p.has_response());
    }

    #[test]
    fn set_response_populates_body_and_clears_spinner() {
        let mut p = AiPaneState::new();
        p.set_spinner(true);
        p.set_response("hello".to_string(), "12:34:56".to_string());
        assert_eq!(p.body, "hello");
        assert_eq!(p.header_time.as_deref(), Some("12:34:56"));
        assert!(!p.spinner_active);
        assert!(p.error.is_none());
        assert!(p.has_response());
    }

    #[test]
    fn set_response_clears_previous_error() {
        let mut p = AiPaneState::new();
        p.set_error("old error".to_string());
        p.set_response("new".to_string(), "00:00:00".to_string());
        assert!(p.error.is_none());
    }

    #[test]
    fn set_error_clears_spinner() {
        let mut p = AiPaneState::new();
        p.set_spinner(true);
        p.set_error("oops".to_string());
        assert!(!p.spinner_active);
        assert_eq!(p.error.as_deref(), Some("oops"));
    }

    #[test]
    fn set_spinner_toggles() {
        let mut p = AiPaneState::new();
        p.set_spinner(true);
        assert!(p.spinner_active);
        p.set_spinner(false);
        assert!(!p.spinner_active);
    }

    #[test]
    fn modal_open_close() {
        let mut p = AiPaneState::new();
        p.open_modal();
        assert!(p.modal_open);
        assert_eq!(p.modal_scroll_offset, 0);
        p.close_modal();
        assert!(!p.modal_open);
    }

    #[test]
    fn scroll_modal_increments_and_bounds_to_zero() {
        let mut p = AiPaneState::new();
        p.scroll_modal(3);
        assert_eq!(p.modal_scroll_offset, 3);
        p.scroll_modal(-2);
        assert_eq!(p.modal_scroll_offset, 1);
        // Scrolling below zero clamps to zero
        p.scroll_modal(-10);
        assert_eq!(p.modal_scroll_offset, 0);
    }

    #[test]
    fn has_response_false_when_empty() {
        let p = AiPaneState::new();
        assert!(!p.has_response());
    }

    #[test]
    fn has_response_true_after_set() {
        let mut p = AiPaneState::new();
        p.set_response("x".to_string(), "t".to_string());
        assert!(p.has_response());
    }

    #[test]
    fn default_is_same_as_new() {
        let p = AiPaneState::default();
        assert!(!p.has_response());
        assert!(p.header_time.is_none());
    }
}
