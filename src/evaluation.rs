use crate::execution::*;
use crate::executor::*;
use crate::ui::*;
use failure::Error;
use std::sync::{Arc, Mutex};

/// The data for an evaluation, including the DAG and the UI channel.
pub struct EvaluationData {
    /// The DAG with the evaluation data.
    pub dag: ExecutionDAG,
    /// The sender of the UI.
    pub sender: Arc<Mutex<UIMessageSender>>,
}

impl EvaluationData {
    /// Crate a new EvaluationData returning the data and the receiving part of
    /// the UI chanel.
    pub fn new() -> (EvaluationData, ChannelReceiver) {
        let (sender, receiver) = UIMessageSender::new();
        (
            EvaluationData {
                dag: ExecutionDAG::new(),
                sender: Arc::new(Mutex::new(sender)),
            },
            receiver,
        )
    }
}

/// What can send UIMessages.
pub trait UISender {
    fn send(&self, message: UIMessage) -> Result<(), Error>;
}

/// Implement .send(message) for Mutex<UIMessageSender> in order to do
/// `EvaluationData.sender.send(message)`. This will lock the mutex and send
/// the message to the UI.
impl UISender for Mutex<UIMessageSender> {
    fn send(&self, message: UIMessage) -> Result<(), Error> {
        self.lock().unwrap().send(message)
    }
}
