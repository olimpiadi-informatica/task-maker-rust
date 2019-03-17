use crate::ui::*;

/// A simple UI that will print to stdout the human readable messages. Useful
/// for debugging or for when curses is not available.
pub struct PrintUI;

impl PrintUI {
    /// Make a new PrintUI.
    pub fn new() -> PrintUI {
        PrintUI {}
    }
}

impl UI for PrintUI {
    fn on_message(&mut self, message: UIMessage) {
        println!("Message {:?}", message);
    }
}
