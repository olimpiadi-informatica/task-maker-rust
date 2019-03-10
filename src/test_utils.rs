#[cfg(test)]
pub mod test_utils {
    use crate::evaluation::*;
    use crate::executor::*;
    use crate::store::*;
    use std::path::Path;
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use tempdir::TempDir;

    pub fn setup_test() -> TempDir {
        let mut has_inited = LOG_INITIALIZED.lock().unwrap();
        if !*has_inited {
            env_logger::Builder::from_default_env()
                .default_format_timestamp_nanos(true)
                .init();
            *has_inited = true;
        }
        TempDir::new("tm-test").unwrap()
    }

    pub fn eval_dag_locally(eval: EvaluationData, cwd: &Path) {
        let (tx, rx_remote) = channel();
        let (tx_remote, rx) = channel();
        let store_path = cwd.join("store");
        let server = thread::spawn(move || {
            let file_store = FileStore::new(&store_path).expect("Cannot create the file store");
            let mut executor = LocalExecutor::new(Arc::new(Mutex::new(file_store)), 4);
            executor.evaluate(tx_remote, rx_remote).unwrap();
        });
        ExecutorClient::evaluate(eval, tx, rx).unwrap();
        server.join().expect("Server paniced");
    }

    lazy_static! {
        static ref LOG_INITIALIZED: Mutex<bool> = Mutex::new(false);
    }
}

#[cfg(test)]
pub use test_utils::*;
