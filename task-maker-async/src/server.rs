use crate::store::ComputationWriteHandle;
use serde::{Deserialize, Serialize};
use task_maker_dag::{ExecutionDAGData, ExecutionGroup};

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerStatus {
    pub num_workers: usize,
    pub queue_length: usize,
}

#[tarpc::service]
pub trait Server {
    /// Asks the server to evaluate the given DAG. All the input files must already be available in
    /// the Store.
    async fn evaluate(dag: ExecutionDAGData) -> Result<(), ()>;

    /// Asks the server for work to do. Returns a ComputationWriteHandle to be used to store the
    /// outputs in the Store. id is an identifier of the worker that calls the method.
    async fn get_work(id: usize) -> (ExecutionGroup, ComputationWriteHandle);

    /// Retrieves information about the status of the server.
    async fn get_status() -> ServerStatus;
}
