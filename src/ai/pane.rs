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
    pub modal_open: bool,
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

    pub fn open_modal(&mut self) {
        self.modal_open = true;
        self.modal_scroll_offset = 0;
    }

    pub fn close_modal(&mut self) {
        self.modal_open = false;
    }

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
