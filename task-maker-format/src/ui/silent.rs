use crate::ui::*;

/// This UI will never print anything.
#[derive(Default)]
pub struct SilentUI;

impl SilentUI {
    /// Make a new SilentUI.
    pub fn new() -> SilentUI {
        SilentUI {}
    }
}

impl UI for SilentUI {
    fn on_message(&mut self, _message: UIMessage) {}

    fn finish(&mut self) {}
}
