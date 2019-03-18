use crate::ui::*;

pub struct IOICursesUI;

impl IOICursesUI {
    pub fn new() -> IOICursesUI {
        IOICursesUI {}
    }
}

impl UI for IOICursesUI {
    fn on_message(&mut self, message: UIMessage) {
        println!("{:?}", message);
    }
}
