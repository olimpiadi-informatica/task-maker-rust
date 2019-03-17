use crate::ui::*;

/// This UI will print to stdout all the raw information it gets, it's very
/// verbose and useful only for debug purpuses.
pub struct RawUI;

impl RawUI {
    /// Make a new RawUI.
    pub fn new() -> RawUI {
        RawUI {}
    }
}

impl UI for RawUI {
    fn on_message(&mut self, message: UIMessage) {
        println!("{:?}", message);
    }
}
