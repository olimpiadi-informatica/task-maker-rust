use crate::ui::*;

pub struct CursesUI;

impl CursesUI {
    /// Make a new CursesUI.
    pub fn new() -> CursesUI {
        CursesUI {}
    }
}

impl UI for CursesUI {
    fn on_message(&mut self, message: UIMessage) {}
}
