use std::sync::{Arc, Mutex};

use crate::executor::*;

pub fn schedule(data: Arc<Mutex<ExecutorData>>) {
    // if data.lock().unwrap().waiting_workers.len() == 0 {
    //     return;
    // }
    // let (lock, cv) = &*data
    //     .lock()
    //     .unwrap()
    //     .waiting_workers
    //     .values()
    //     .nth(0)
    //     .unwrap()
    //     .clone();
    // let mut lock = lock.lock().unwrap();
    // *lock = Some("ciao".to_owned());
    // cv.notify_one();
    // warn!(
    //     "Call to schedule: {:#?}",
    //     data.lock().unwrap().waiting_workers
    // );
}
