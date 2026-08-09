use journal_sdk::Word;
use journal_sdk::test_support::{ScenarioEnvironment, ScenarioTransport};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Delivery {
    After(u64),
    Drop,
}

#[derive(Clone, Debug)]
pub struct ScenarioAction {
    pub url: String,
    pub expression: String,
    pub schedule: Vec<Delivery>,
    pub tick: u64,
}

#[derive(Clone, Debug)]
pub enum ScenarioEvent {
    Message {
        sequence: u64,
        time: u64,
        action: usize,
        message: usize,
        kind: &'static str,
        source: Option<String>,
        target: Option<String>,
        latency: Option<u64>,
        dropped: bool,
    },
    Result {
        sequence: u64,
        time: u64,
        action: usize,
        value: String,
    },
}

#[derive(Debug)]
pub struct ScenarioError(pub String);

impl std::fmt::Display for ScenarioError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ScenarioError {}

type RemoteResult = Result<Vec<u8>, String>;

enum WorkerEvent {
    Remote {
        action: usize,
        source: Word,
        url: String,
        body: String,
        reply: mpsc::SyncSender<RemoteResult>,
    },
    RemoteComplete {
        action: usize,
        source: Word,
        target: Word,
        reply: mpsc::SyncSender<RemoteResult>,
        result: String,
    },
    TopComplete {
        action: usize,
        result: String,
    },
    Failed(String),
}

struct SchedulerTransport {
    events: Mutex<Option<mpsc::Sender<WorkerEvent>>>,
}

impl SchedulerTransport {
    fn set_events(&self, events: mpsc::Sender<WorkerEvent>) {
        *self
            .events
            .lock()
            .expect("scenario transport lock was poisoned") = Some(events);
    }
}

impl ScenarioTransport for SchedulerTransport {
    fn remote(&self, action: usize, source: Word, url: String, body: String) -> RemoteResult {
        let (reply, response) = mpsc::sync_channel(1);
        self.events
            .lock()
            .expect("scenario transport lock was poisoned")
            .as_ref()
            .ok_or_else(|| "No scenario scheduler is accepting remote requests".to_string())?
            .send(WorkerEvent::Remote {
                action,
                source,
                url,
                body,
                reply,
            })
            .map_err(|_| {
                "Scenario scheduler stopped before accepting remote request".to_string()
            })?;
        response.recv().map_err(|_| {
            "Scenario scheduler stopped before returning remote response".to_string()
        })?
    }
}

enum ScheduledKind {
    Start {
        record: Word,
        expression: String,
    },
    Remote {
        source: Word,
        target: Word,
        body: String,
        reply: mpsc::SyncSender<RemoteResult>,
    },
    Response {
        source: Word,
        target: Word,
        result: String,
        reply: mpsc::SyncSender<RemoteResult>,
    },
    Drop {
        source: Word,
        target: Word,
        reply: mpsc::SyncSender<RemoteResult>,
        response: bool,
    },
    Result(String),
}

struct Scheduled {
    time: u64,
    order: u64,
    action: usize,
    message: usize,
    latency: Option<u64>,
    kind: ScheduledKind,
}

struct ActionState {
    url: String,
    schedule: Vec<Delivery>,
    schedule_index: usize,
    message_index: usize,
    result: Option<String>,
}

pub struct ScenarioWorld {
    environment: Arc<ScenarioEnvironment>,
    records: HashMap<String, Word>,
    reverse: HashMap<Word, String>,
    worker_tx: mpsc::Sender<WorkerEvent>,
    worker_rx: mpsc::Receiver<WorkerEvent>,
    actions: HashMap<usize, ActionState>,
    collection: VecDeque<usize>,
    scheduled: Vec<Scheduled>,
    next_action: usize,
    current_time: u64,
    last_start: Option<u64>,
    order: u64,
    sequence: u64,
    running: bool,
}

impl ScenarioWorld {
    pub fn new() -> Self {
        let transport = Arc::new(SchedulerTransport {
            events: Mutex::new(None),
        });
        let (worker_tx, worker_rx) = mpsc::channel();
        transport.set_events(worker_tx.clone());
        Self {
            environment: Arc::new(ScenarioEnvironment::new(transport)),
            records: HashMap::new(),
            reverse: HashMap::new(),
            worker_tx,
            worker_rx,
            actions: HashMap::new(),
            collection: VecDeque::new(),
            scheduled: Vec::new(),
            next_action: 0,
            current_time: 0,
            last_start: None,
            order: 0,
            sequence: 0,
            running: false,
        }
    }

    fn record_for(&mut self, url: &str) -> Result<Word, ScenarioError> {
        if let Some(record) = self.records.get(url) {
            return Ok(*record);
        }
        let record: Word = Sha256::digest(url.as_bytes()).into();
        self.environment
            .create_record(record)
            .map_err(ScenarioError)?;
        self.records.insert(url.to_string(), record);
        self.reverse.insert(record, url.to_string());
        Ok(record)
    }

    fn enqueue(&mut self, time: u64, action: usize, latency: Option<u64>, kind: ScheduledKind) {
        let state = self
            .actions
            .get_mut(&action)
            .expect("scheduled action has no state");
        let message = state.message_index;
        state.message_index += 1;
        self.scheduled.push(Scheduled {
            time,
            order: self.order,
            action,
            message,
            latency,
            kind,
        });
        self.order += 1;
    }

    fn enqueue_network(
        &mut self,
        action: usize,
        source: Word,
        target: Word,
        reply: mpsc::SyncSender<RemoteResult>,
        event: NetworkEvent,
    ) {
        let delivery = {
            let state = self
                .actions
                .get_mut(&action)
                .expect("network action has no state");
            let delivery = state
                .schedule
                .get(state.schedule_index)
                .cloned()
                .unwrap_or(Delivery::After(0));
            state.schedule_index += 1;
            delivery
        };
        match delivery {
            Delivery::After(latency) => {
                let kind = match event {
                    NetworkEvent::Request(body) => ScheduledKind::Remote {
                        source,
                        target,
                        body,
                        reply,
                    },
                    NetworkEvent::Response(result) => ScheduledKind::Response {
                        source,
                        target,
                        result,
                        reply,
                    },
                };
                self.enqueue(
                    self.current_time.saturating_add(latency),
                    action,
                    Some(latency),
                    kind,
                );
            }
            Delivery::Drop => self.enqueue(
                self.current_time,
                action,
                None,
                ScheduledKind::Drop {
                    source,
                    target,
                    reply,
                    response: matches!(event, NetworkEvent::Response(_)),
                },
            ),
        }
    }

    pub fn submit(&mut self, action: ScenarioAction) -> Result<usize, ScenarioError> {
        let id = self.next_action;
        self.next_action += 1;
        let baseline = self.last_start.unwrap_or(self.current_time);
        let requested = baseline.saturating_add(action.tick);
        let start = requested.max(self.current_time);
        self.last_start = Some(start);
        let record = self.record_for(&action.url)?;
        self.actions.insert(
            id,
            ActionState {
                url: action.url,
                schedule: action.schedule,
                schedule_index: 0,
                message_index: 0,
                result: None,
            },
        );
        self.collection.push_back(id);
        self.enqueue(
            start,
            id,
            None,
            ScheduledKind::Start {
                record,
                expression: action.expression,
            },
        );
        Ok(id)
    }

    fn pop_next(&mut self) -> Option<Scheduled> {
        let position = self
            .scheduled
            .iter()
            .enumerate()
            .min_by_key(|(_, event)| (event.time, event.order))
            .map(|(position, _)| position)?;
        Some(self.scheduled.remove(position))
    }

    fn receive_worker(&mut self) -> Result<(), ScenarioError> {
        let event = self
            .worker_rx
            .recv()
            .map_err(|_| ScenarioError("Scenario worker stopped without an event".to_string()))?;
        self.running = false;
        match event {
            WorkerEvent::Remote {
                action,
                source,
                url,
                body,
                reply,
            } => {
                let target = self.record_for(&url)?;
                self.enqueue_network(action, source, target, reply, NetworkEvent::Request(body));
            }
            WorkerEvent::RemoteComplete {
                action,
                source,
                target,
                reply,
                result,
            } => self.enqueue_network(
                action,
                source,
                target,
                reply,
                NetworkEvent::Response(result),
            ),
            WorkerEvent::TopComplete { action, result } => {
                self.enqueue(
                    self.current_time,
                    action,
                    None,
                    ScheduledKind::Result(result),
                );
            }
            WorkerEvent::Failed(message) => return Err(ScenarioError(message)),
        }
        Ok(())
    }

    fn describe(&self, event: &Scheduled) -> (&'static str, Option<String>, Option<String>, bool) {
        match &event.kind {
            ScheduledKind::Start { record, .. } => {
                ("request", None, self.reverse.get(record).cloned(), false)
            }
            ScheduledKind::Remote { source, target, .. } => (
                "remote-request",
                self.reverse.get(source).cloned(),
                self.reverse.get(target).cloned(),
                false,
            ),
            ScheduledKind::Response { source, target, .. } => (
                "remote-response",
                self.reverse.get(target).cloned(),
                self.reverse.get(source).cloned(),
                false,
            ),
            ScheduledKind::Drop {
                source,
                target,
                response,
                ..
            } => (
                if *response {
                    "remote-response"
                } else {
                    "remote-request"
                },
                if *response {
                    self.reverse.get(target).cloned()
                } else {
                    self.reverse.get(source).cloned()
                },
                if *response {
                    self.reverse.get(source).cloned()
                } else {
                    self.reverse.get(target).cloned()
                },
                true,
            ),
            ScheduledKind::Result(_) => {
                let source = self
                    .actions
                    .get(&event.action)
                    .map(|state| state.url.clone());
                ("response", source, None, false)
            }
        }
    }

    fn dispatch(
        &mut self,
        event: Scheduled,
        emit: &mut impl FnMut(ScenarioEvent),
    ) -> Result<(), ScenarioError> {
        self.current_time = self.current_time.max(event.time);
        self.environment.set_time(self.current_time);
        let (kind, source, target, dropped) = self.describe(&event);
        emit(ScenarioEvent::Message {
            sequence: self.sequence,
            time: self.current_time,
            action: event.action,
            message: event.message,
            kind,
            source,
            target,
            latency: event.latency,
            dropped,
        });
        self.sequence += 1;

        match event.kind {
            ScheduledKind::Start { record, expression } => {
                spawn_top(
                    self.environment.clone(),
                    self.worker_tx.clone(),
                    event.action,
                    record,
                    expression,
                );
                self.running = true;
            }
            ScheduledKind::Remote {
                source,
                target,
                body,
                reply,
            } => {
                spawn_remote(
                    self.environment.clone(),
                    self.worker_tx.clone(),
                    event.action,
                    source,
                    target,
                    body,
                    reply,
                );
                self.running = true;
            }
            ScheduledKind::Response { result, reply, .. } => {
                reply.send(Ok(result.into_bytes())).map_err(|_| {
                    ScenarioError("Remote caller stopped before its response".to_string())
                })?;
                self.running = true;
            }
            ScheduledKind::Drop { reply, .. } => {
                reply
                    .send(Err(
                        "Scenario schedule dropped a journal message".to_string()
                    ))
                    .map_err(|_| {
                        ScenarioError("Dropped message caller stopped before failure".to_string())
                    })?;
                self.running = true;
            }
            ScheduledKind::Result(value) => {
                self.actions
                    .get_mut(&event.action)
                    .expect("completed action has no state")
                    .result = Some(value.clone());
                emit(ScenarioEvent::Result {
                    sequence: self.sequence,
                    time: self.current_time,
                    action: event.action,
                    value,
                });
                self.sequence += 1;
            }
        }
        Ok(())
    }

    pub fn await_next(
        &mut self,
        mut emit: impl FnMut(ScenarioEvent),
    ) -> Result<String, ScenarioError> {
        let action = *self
            .collection
            .front()
            .ok_or_else(|| ScenarioError("test-await has no submitted action".to_string()))?;
        loop {
            if let Some(result) = self
                .actions
                .get(&action)
                .and_then(|state| state.result.clone())
            {
                self.collection.pop_front();
                return Ok(result);
            }
            if self.running {
                self.receive_worker()?;
                continue;
            }
            let event = self.pop_next().ok_or_else(|| {
                ScenarioError("Scenario deadlocked with no runnable messages".to_string())
            })?;
            self.dispatch(event, &mut emit)?;
        }
    }

    pub fn pending(&self) -> usize {
        self.collection.len()
    }
}

enum NetworkEvent {
    Request(String),
    Response(String),
}

fn spawn_top(
    environment: Arc<ScenarioEnvironment>,
    events: mpsc::Sender<WorkerEvent>,
    action: usize,
    record: Word,
    expression: String,
) {
    thread::spawn(move || {
        let event = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            environment.evaluate(record, action, &expression)
        })) {
            Ok(result) => WorkerEvent::TopComplete { action, result },
            Err(_) => WorkerEvent::Failed(format!("Scenario action {action} panicked")),
        };
        let _ = events.send(event);
    });
}

fn spawn_remote(
    environment: Arc<ScenarioEnvironment>,
    events: mpsc::Sender<WorkerEvent>,
    action: usize,
    source: Word,
    target: Word,
    body: String,
    reply: mpsc::SyncSender<RemoteResult>,
) {
    thread::spawn(move || {
        let event = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            environment.evaluate(target, action, &body)
        })) {
            Ok(result) => WorkerEvent::RemoteComplete {
                action,
                source,
                target,
                reply,
                result,
            },
            Err(_) => WorkerEvent::Failed(format!(
                "Remote evaluation for scenario action {action} panicked"
            )),
        };
        let _ = events.send(event);
    });
}

pub fn run_scenario(
    world: &mut ScenarioWorld,
    actions: Vec<ScenarioAction>,
    mut emit: impl FnMut(ScenarioEvent),
) -> Result<Vec<String>, ScenarioError> {
    if world.pending() != 0 {
        return Err(ScenarioError(
            "run-scenario cannot start while submitted actions are outstanding".to_string(),
        ));
    }
    let count = actions.len();
    for action in actions {
        world.submit(action)?;
    }
    let mut results = Vec::with_capacity(count);
    for _ in 0..count {
        results.push(world.await_next(&mut emit)?);
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(url: &str, expression: &str, schedule: Vec<Delivery>, tick: u64) -> ScenarioAction {
        ScenarioAction {
            url: url.to_string(),
            expression: expression.to_string(),
            schedule,
            tick,
        }
    }

    #[test]
    fn applies_relative_latency_and_collects_in_submission_order() {
        let mut world = ScenarioWorld::new();
        let actions = vec![
            action(
                "http://journal-101.test/interface",
                "(sync-remote \"http://journal-102.test/interface\" '(+ 3 4))",
                vec![Delivery::After(2), Delivery::After(1)],
                0,
            ),
            action("http://journal-103.test/interface", "9", vec![], 1),
        ];
        let mut completions = Vec::new();
        let results = run_scenario(&mut world, actions, |event| {
            if let ScenarioEvent::Result { action, time, .. } = event {
                completions.push((action, time));
            }
        })
        .expect("scenario should complete");
        assert_eq!(results, vec!["7", "9"]);
        assert_eq!(completions, vec![(1, 1), (0, 3)]);
    }

    #[test]
    fn zero_latency_messages_append_behind_same_slot_events() {
        let mut world = ScenarioWorld::new();
        let actions = vec![
            action(
                "http://journal-104.test/interface",
                "(sync-remote \"http://journal-105.test/interface\" '(+ 3 4))",
                vec![],
                0,
            ),
            action("http://journal-106.test/interface", "9", vec![], 0),
        ];
        let mut completions = Vec::new();
        let results = run_scenario(&mut world, actions, |event| {
            if let ScenarioEvent::Result { action, .. } = event {
                completions.push(action);
            }
        })
        .expect("scenario should complete");
        assert_eq!(results, vec!["7", "9"]);
        assert_eq!(completions, vec![1, 0]);
    }

    #[test]
    fn exposes_global_scheduler_time_to_scheme() {
        let mut world = ScenarioWorld::new();
        let first = run_scenario(
            &mut world,
            vec![action(
                "http://journal-107.test/interface",
                "(list (system-time-unix) (system-time-unix))",
                vec![],
                5,
            )],
            |_| {},
        )
        .expect("first timed action should complete");
        let second = run_scenario(
            &mut world,
            vec![action(
                "http://journal-107.test/interface",
                "(system-time-unix)",
                vec![],
                2,
            )],
            |_| {},
        )
        .expect("second timed action should complete");
        assert_eq!(first, vec!["(5 5)"]);
        assert_eq!(second, vec!["7"]);
    }

    #[test]
    fn fixes_randomness_per_scenario_world() {
        let expression = "(random-byte-vector 8)";
        let mut first_world = ScenarioWorld::new();
        let first = run_scenario(
            &mut first_world,
            vec![action(
                "http://journal-109.test/interface",
                expression,
                vec![],
                0,
            )],
            |_| {},
        )
        .expect("first random action should complete");
        let next = run_scenario(
            &mut first_world,
            vec![action(
                "http://journal-109.test/interface",
                expression,
                vec![],
                0,
            )],
            |_| {},
        )
        .expect("second random action should complete");
        let mut second_world = ScenarioWorld::new();
        let repeated = run_scenario(
            &mut second_world,
            vec![action(
                "http://journal-110.test/interface",
                expression,
                vec![],
                0,
            )],
            |_| {},
        )
        .expect("repeated random action should complete");
        assert_eq!(first, repeated);
        assert_ne!(first, next);
    }

    #[test]
    fn reuses_lazy_journal_records_across_runs() {
        let mut world = ScenarioWorld::new();
        for expression in ["1", "2"] {
            run_scenario(
                &mut world,
                vec![action(
                    "http://journal-108.test/interface",
                    expression,
                    vec![],
                    0,
                )],
                |_| {},
            )
            .expect("scenario should complete");
        }
        assert_eq!(world.records.len(), 1);
    }

    #[test]
    fn dropped_response_returns_transport_error_without_advancing_time() {
        let mut world = ScenarioWorld::new();
        let actions = vec![action(
            "http://journal-111.test/interface",
            "(catch #t (lambda () (sync-remote \"http://journal-112.test/interface\" '(+ 1 2))) (lambda args 'dropped))",
            vec![Delivery::After(0), Delivery::Drop],
            0,
        )];
        let mut dropped = Vec::new();
        let results = run_scenario(&mut world, actions, |event| {
            if let ScenarioEvent::Message {
                time,
                dropped: true,
                ..
            } = event
            {
                dropped.push(time);
            }
        })
        .expect("dropped response should unwind");
        assert_eq!(results, vec!["dropped"]);
        assert_eq!(dropped, vec![0]);
    }
}
