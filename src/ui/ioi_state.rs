use crate::ui::*;

/// The state of a IOI task, all the information for the UI are stored here.
pub struct IOIUIState {
    /// DELETEME: the number of events received.
    pub num_events: u32,
}

impl IOIUIState {
    /// Make a new IOIUIState.
    pub fn new() -> IOIUIState {
        IOIUIState { num_events: 0 }
    }

    /// Apply a UIMessage to this state.
    pub fn apply(&mut self, message: UIMessage) {
        self.num_events += 1;
    }
}
