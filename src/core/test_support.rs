use std::thread;
use std::time::{Duration, Instant};

use super::model::{OperationSnapshot, OperationState};
use super::operations::OperationQueue;

pub(crate) fn wait_terminal(queue: &OperationQueue, id: &str) -> OperationSnapshot {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(snapshot) = queue.snapshot(id)
            && matches!(
                snapshot.state,
                OperationState::Succeeded | OperationState::Failed | OperationState::Cancelled
            )
        {
            return snapshot;
        }
        assert!(Instant::now() < deadline, "operation {id} did not finish");
        thread::sleep(Duration::from_millis(2));
    }
}
