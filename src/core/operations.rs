use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::thread;

use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

use super::error::{AppError, ErrorCode, Result};
use super::model::{
    OperationEvent, OperationEventKind, OperationKind, OperationLabel, OperationProgress,
    OperationSnapshot, OperationState,
};

const EVENT_CAPACITY: usize = 256;

type OperationJob = Box<dyn FnOnce(OperationContext) -> Result<Value> + Send + 'static>;

struct QueuedOperation {
    id: String,
    control: Arc<AtomicBool>,
    job: OperationJob,
}

#[derive(Default)]
struct State {
    snapshots: HashMap<String, OperationSnapshot>,
    controls: HashMap<String, Arc<AtomicBool>>,
    queued: VecDeque<String>,
    running: [Option<String>; 2],
}

struct Shared {
    state: Mutex<State>,
    events: broadcast::Sender<OperationEvent>,
}

#[derive(Clone)]
pub struct OperationQueue {
    transfer_sender: mpsc::Sender<QueuedOperation>,
    maintenance_sender: mpsc::Sender<QueuedOperation>,
    shared: Arc<Shared>,
}

#[derive(Clone)]
pub struct OperationContext {
    id: String,
    control: Arc<AtomicBool>,
    shared: Arc<Shared>,
}

impl OperationQueue {
    pub fn new() -> Self {
        let (transfer_sender, transfer_receiver) = mpsc::channel::<QueuedOperation>();
        let (maintenance_sender, maintenance_receiver) = mpsc::channel::<QueuedOperation>();
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            events,
        });
        spawn_worker(
            "skills-manager-transfers",
            transfer_receiver,
            shared.clone(),
            0,
        );
        spawn_worker(
            "skills-manager-maintenance",
            maintenance_receiver,
            shared.clone(),
            1,
        );
        Self {
            transfer_sender,
            maintenance_sender,
            shared,
        }
    }

    pub fn enqueue<T>(
        &self,
        kind: OperationKind,
        job: impl FnOnce(OperationContext) -> Result<T> + Send + 'static,
    ) -> Result<String>
    where
        T: Serialize + Send + 'static,
    {
        self.enqueue_with_label(kind, None, job)
    }

    /// Enqueues an operation whose progress the UI shows by name before its
    /// result exists, such as a marketplace install.
    pub fn enqueue_with_label<T>(
        &self,
        kind: OperationKind,
        label: Option<OperationLabel>,
        job: impl FnOnce(OperationContext) -> Result<T> + Send + 'static,
    ) -> Result<String>
    where
        T: Serialize + Send + 'static,
    {
        self.enqueue_value_with_label(kind, label, move |context| {
            let value = job(context)?;
            serde_json::to_value(value).map_err(AppError::from)
        })
    }

    pub fn enqueue_value(
        &self,
        kind: OperationKind,
        job: impl FnOnce(OperationContext) -> Result<Value> + Send + 'static,
    ) -> Result<String> {
        self.enqueue_value_with_label(kind, None, job)
    }

    pub fn enqueue_value_with_label(
        &self,
        kind: OperationKind,
        label: Option<OperationLabel>,
        job: impl FnOnce(OperationContext) -> Result<Value> + Send + 'static,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let control = Arc::new(AtomicBool::new(false));
        let now = now_millis();
        let snapshot = OperationSnapshot {
            id: id.clone(),
            kind,
            state: OperationState::Queued,
            cancel_requested: false,
            label,
            progress: None,
            result: None,
            error: None,
            created_at: now,
            updated_at: now,
        };
        let operation = QueuedOperation {
            id: id.clone(),
            control: control.clone(),
            job: Box::new(job),
        };

        let mut state = lock_state(&self.shared);
        state.snapshots.insert(id.clone(), snapshot.clone());
        state.controls.insert(id.clone(), control);
        state.queued.push_back(id.clone());
        emit(&self.shared, OperationEventKind::OperationState, snapshot);
        let sender = if matches!(kind, OperationKind::Delete | OperationKind::Rename) {
            &self.maintenance_sender
        } else {
            &self.transfer_sender
        };
        if sender.send(operation).is_err() {
            state.snapshots.remove(&id);
            state.controls.remove(&id);
            state.queued.retain(|queued| queued != &id);
            return Err(AppError::internal("operation worker is unavailable"));
        }
        drop(state);
        Ok(id)
    }

    pub fn snapshot(&self, id: &str) -> Option<OperationSnapshot> {
        lock_state(&self.shared).snapshots.get(id).cloned()
    }

    pub fn active_operation(&self) -> Option<OperationSnapshot> {
        let state = lock_state(&self.shared);
        state
            .running
            .iter()
            .flatten()
            .next()
            .or_else(|| state.queued.front())
            .and_then(|id| state.snapshots.get(id))
            .cloned()
    }

    pub fn cancel(&self, id: &str) -> Result<OperationSnapshot> {
        let mut state = lock_state(&self.shared);
        let control = state
            .controls
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, "operation was not found"))?;
        let current = state
            .snapshots
            .get(id)
            .expect("an operation control has a matching snapshot");
        if is_terminal(current.state) {
            return Ok(current.clone());
        }
        let was_queued = current.state == OperationState::Queued;
        if was_queued {
            state.queued.retain(|queued| queued != id);
        }
        control.store(true, Ordering::Release);
        let snapshot = state
            .snapshots
            .get_mut(id)
            .expect("the operation snapshot remains present");
        snapshot.cancel_requested = true;
        snapshot.updated_at = now_millis();
        if was_queued {
            snapshot.state = OperationState::Cancelled;
            snapshot.error = Some(cancelled_error());
        }
        let snapshot = snapshot.clone();
        emit(
            &self.shared,
            OperationEventKind::OperationState,
            snapshot.clone(),
        );
        Ok(snapshot)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<OperationEvent> {
        self.shared.events.subscribe()
    }
}

impl Default for OperationQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl OperationContext {
    pub fn check_cancelled(&self) -> Result<()> {
        if self.control.load(Ordering::Acquire) {
            Err(cancelled_error())
        } else {
            Ok(())
        }
    }

    pub fn is_cancel_requested(&self) -> bool {
        self.control.load(Ordering::Acquire)
    }

    pub fn progress(&self, progress: OperationProgress) -> Result<()> {
        self.check_cancelled()?;
        let mut state = lock_state(&self.shared);
        let snapshot = state
            .snapshots
            .get_mut(&self.id)
            .ok_or_else(|| AppError::internal("running operation snapshot is missing"))?;
        snapshot.progress = Some(progress);
        snapshot.updated_at = now_millis();
        let snapshot = snapshot.clone();
        emit(
            &self.shared,
            OperationEventKind::OperationProgress,
            snapshot,
        );
        Ok(())
    }

    pub fn library_changed(&self) {
        let state = lock_state(&self.shared);
        let Some(snapshot) = state.snapshots.get(&self.id).cloned() else {
            return;
        };
        emit(&self.shared, OperationEventKind::LibraryChanged, snapshot);
    }
}

impl OperationEvent {
    pub fn resync(operation: Option<OperationSnapshot>) -> Self {
        Self {
            kind: OperationEventKind::OperationsResync,
            operation,
        }
    }
}

fn spawn_worker(
    name: &'static str,
    receiver: mpsc::Receiver<QueuedOperation>,
    shared: Arc<Shared>,
    lane: usize,
) {
    thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            while let Ok(operation) = receiver.recv() {
                run_operation(&shared, operation, lane);
            }
        })
        .expect("the operation worker thread must be creatable");
}

fn run_operation(shared: &Arc<Shared>, operation: QueuedOperation, lane: usize) {
    {
        let mut state = lock_state(shared);
        state.queued.retain(|id| id != &operation.id);
        if state
            .snapshots
            .get(&operation.id)
            .is_some_and(|snapshot| snapshot.state == OperationState::Cancelled)
        {
            return;
        }
        if !state.snapshots.contains_key(&operation.id) {
            return;
        }
        state.running[lane] = Some(operation.id.clone());
        let snapshot = state
            .snapshots
            .get_mut(&operation.id)
            .expect("the queued snapshot remains present");
        snapshot.state = OperationState::Running;
        snapshot.updated_at = now_millis();
        let snapshot = snapshot.clone();
        emit(shared, OperationEventKind::OperationState, snapshot);
    }

    let context = OperationContext {
        id: operation.id.clone(),
        control: operation.control.clone(),
        shared: shared.clone(),
    };
    let result = catch_unwind(AssertUnwindSafe(|| (operation.job)(context)))
        .unwrap_or_else(|_| Err(AppError::internal("operation panicked")));

    let mut state = lock_state(shared);
    if state.running[lane].as_deref() == Some(operation.id.as_str()) {
        state.running[lane] = None;
    }
    let Some(snapshot) = state.snapshots.get_mut(&operation.id) else {
        return;
    };
    snapshot.updated_at = now_millis();
    match result {
        Ok(value) => {
            snapshot.state = OperationState::Succeeded;
            snapshot.result = Some(value);
        }
        Err(error) => {
            snapshot.state = if error.code == ErrorCode::Cancelled {
                OperationState::Cancelled
            } else {
                OperationState::Failed
            };
            snapshot.error = Some(error);
        }
    }
    let snapshot = snapshot.clone();
    emit(shared, OperationEventKind::OperationState, snapshot);
}

fn emit(shared: &Shared, kind: OperationEventKind, operation: OperationSnapshot) {
    let _ = shared.events.send(OperationEvent {
        kind,
        operation: Some(operation),
    });
}

fn lock_state(shared: &Shared) -> MutexGuard<'_, State> {
    shared
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) fn cancelled_error() -> AppError {
    AppError::new(ErrorCode::Cancelled, "operation was cancelled")
}

#[cfg(test)]
pub(crate) fn test_context(cancelled: bool) -> OperationContext {
    let (events, _) = broadcast::channel(EVENT_CAPACITY);
    let id = Uuid::new_v4().to_string();
    let control = Arc::new(AtomicBool::new(cancelled));
    let now = now_millis();
    let snapshot = OperationSnapshot {
        id: id.clone(),
        kind: OperationKind::Rename,
        state: OperationState::Running,
        cancel_requested: cancelled,
        label: None,
        progress: None,
        result: None,
        error: None,
        created_at: now,
        updated_at: now,
    };
    let mut state = State::default();
    state.snapshots.insert(id.clone(), snapshot);
    state.controls.insert(id.clone(), control.clone());
    state.running[0] = Some(id.clone());
    OperationContext {
        id,
        control,
        shared: Arc::new(Shared {
            state: Mutex::new(state),
            events,
        }),
    }
}

fn is_terminal(state: OperationState) -> bool {
    matches!(
        state,
        OperationState::Succeeded | OperationState::Failed | OperationState::Cancelled
    )
}

fn now_millis() -> i64 {
    Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::core::test_support::wait_terminal;

    fn progress(phase: &str) -> OperationProgress {
        OperationProgress {
            phase: phase.to_string(),
            current: None,
            total: None,
            bytes: None,
            bytes_total: None,
            detail: None,
            cancellable: true,
        }
    }

    #[test]
    fn executes_jobs_in_fifo_order_and_preserves_results() {
        let queue = OperationQueue::new();
        let order = Arc::new(Mutex::new(Vec::new()));
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();

        let first_order = order.clone();
        let first = queue
            .enqueue(OperationKind::Install, move |_| {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                first_order.lock().unwrap().push("first");
                Ok("one")
            })
            .unwrap();
        started_rx.recv().unwrap();
        for (kind, value) in [
            (OperationKind::ApplyUpdate, "second"),
            (OperationKind::CheckUpdates, "third"),
        ] {
            let order = order.clone();
            queue
                .enqueue_value(kind, move |_| {
                    order.lock().unwrap().push(value);
                    Ok(Value::Null)
                })
                .unwrap();
        }
        release_tx.send(()).unwrap();

        assert_eq!(wait_terminal(&queue, &first).result.unwrap(), "one");
        assert_eq!(*order.lock().unwrap(), ["first", "second", "third"]);
        assert_eq!(queue.active_operation(), None);
    }

    #[test]
    fn queued_cancellation_never_runs_the_job() {
        let queue = OperationQueue::new();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let first = queue
            .enqueue_value(OperationKind::Install, move |_| {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(Value::Null)
            })
            .unwrap();
        started_rx.recv().unwrap();

        let ran = Arc::new(AtomicBool::new(false));
        let job_ran = ran.clone();
        let second = queue
            .enqueue_value(OperationKind::CheckUpdates, move |_| {
                job_ran.store(true, Ordering::Release);
                Ok(Value::Null)
            })
            .unwrap();
        let cancelled = queue.cancel(&second).unwrap();
        assert_eq!(cancelled.state, OperationState::Cancelled);
        release_tx.send(()).unwrap();
        wait_terminal(&queue, &first);

        assert!(!ran.load(Ordering::Acquire));
        assert_eq!(queue.active_operation(), None);
        assert_eq!(
            wait_terminal(&queue, &second).error.unwrap().code,
            ErrorCode::Cancelled
        );
    }

    #[test]
    fn running_cancellation_is_observed_and_progress_is_emitted() {
        let queue = OperationQueue::new();
        let mut events = queue.subscribe();
        let (progress_tx, progress_rx) = mpsc::channel();
        let id = queue
            .enqueue_value(OperationKind::CheckUpdates, move |context| {
                context.progress(progress("checking"))?;
                progress_tx.send(()).unwrap();
                loop {
                    context.check_cancelled()?;
                    thread::sleep(Duration::from_millis(2));
                }
            })
            .unwrap();
        progress_rx.recv().unwrap();
        queue.cancel(&id).unwrap();
        let snapshot = wait_terminal(&queue, &id);
        assert_eq!(snapshot.state, OperationState::Cancelled);
        assert!(snapshot.cancel_requested);

        let mut kinds = Vec::new();
        while let Ok(event) = events.try_recv() {
            kinds.push(event.kind);
        }
        assert!(kinds.contains(&OperationEventKind::OperationProgress));
        assert_eq!(kinds.last(), Some(&OperationEventKind::OperationState));
    }

    #[test]
    fn failures_and_panics_become_failed_snapshots_without_stopping_the_worker() {
        let queue = OperationQueue::new();
        let conflict = queue
            .enqueue_value(OperationKind::ApplyUpdate, |_| {
                Err(AppError::new(ErrorCode::Conflict, "local changes"))
            })
            .unwrap();
        let panic = queue
            .enqueue_value(OperationKind::Delete, |_| -> Result<Value> {
                panic!("test panic")
            })
            .unwrap();
        let after = queue
            .enqueue(OperationKind::Rename, |_| Ok("still running"))
            .unwrap();

        let conflict = wait_terminal(&queue, &conflict);
        assert_eq!(conflict.state, OperationState::Failed);
        assert_eq!(conflict.error.unwrap().code, ErrorCode::Conflict);
        let panic = wait_terminal(&queue, &panic);
        assert_eq!(panic.state, OperationState::Failed);
        assert_eq!(panic.error.unwrap().code, ErrorCode::Internal);
        assert_eq!(
            wait_terminal(&queue, &after).result.unwrap(),
            Value::String("still running".to_string())
        );
    }

    #[test]
    fn skill_changes_have_a_distinct_event() {
        let queue = OperationQueue::new();
        let mut events = queue.subscribe();
        let id = queue
            .enqueue_value(OperationKind::Install, |context| {
                context.library_changed();
                Ok(Value::Null)
            })
            .unwrap();
        wait_terminal(&queue, &id);

        let mut found = false;
        while let Ok(event) = events.try_recv() {
            found |= event.kind == OperationEventKind::LibraryChanged;
        }
        assert!(found);
    }

    #[test]
    fn local_deletes_run_while_a_transfer_is_waiting() {
        let queue = OperationQueue::new();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let transfer = queue
            .enqueue_value(OperationKind::Install, move |_| {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(Value::Null)
            })
            .unwrap();
        started_rx.recv().unwrap();

        let deleted = queue
            .enqueue_value(OperationKind::Delete, |_| Ok(Value::Bool(true)))
            .unwrap();
        assert_eq!(
            wait_terminal(&queue, &deleted).result,
            Some(Value::Bool(true))
        );

        release_tx.send(()).unwrap();
        wait_terminal(&queue, &transfer);
    }
}
