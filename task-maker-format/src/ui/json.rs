use crate::ui::*;

/// This UI will print to stdout the UI messages as json.
#[derive(Default)]
pub struct JsonUI;

impl JsonUI {
    /// Make a new `JsonUI`.
    pub fn new() -> JsonUI {
        JsonUI {}
    }
}

impl UI for JsonUI {
    fn on_message(&mut self, message: UIMessage) {
        let message = serde_json::to_string(&message).expect("Failed to serialize message");
        println!("{}", message);
    }

    fn finish(&mut self) {}
}
