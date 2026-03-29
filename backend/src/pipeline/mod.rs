pub mod archive;
pub mod executors;
pub mod runner;
pub mod variables;

use std::collections::{HashMap, VecDeque};

use std::sync::Arc;

use chrono::Utc;
use dashmap::DashMap;
use uuid::Uuid;

use crate::types::*;
use crate::AppState;

const PHASE_LOG_BUFFER_SIZE: usize = 2000;

pub struct PipelineManager {
    pub active: DashMap<Uuid, Arc<PipelineHandle>>,
}

pub struct PipelineHandle {
    pub progress: parking_lot::Mutex<PhaseProgress>,
    pub log_tx: tokio::sync::broadcast::Sender<WsMessage>,
    pub task_handle: parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub phase_log_buffer: parking_lot::Mutex<VecDeque<PhaseLogLine>>,
    pub process_config: parking_lot::Mutex<ProcessConfig>,
}

impl PipelineHandle {
    pub(crate) fn broadcast_progress(&self) {
        let progress = self.progress.lock().clone();
        let _ = self.log_tx.send(WsMessage::PhaseProgress(progress));
    }

    pub(crate) fn emit_log(
        &self,
        phase: PhaseKind,
        step_index: u32,
        step_name: &str,
        line: String,
        stream: LogStream,
    ) {
        let log_line = PhaseLogLine {
            timestamp: Utc::now(),
            phase,
            step_index,
            step_name: step_name.to_string(),
            line,
            stream,
        };

        // Buffer for replay
        {
            let mut buf = self.phase_log_buffer.lock();
            if buf.len() >= PHASE_LOG_BUFFER_SIZE {
                buf.pop_front();
            }
            buf.push_back(log_line.clone());
        }

        let msg = WsMessage::PhaseLog(log_line);
        let _ = self.log_tx.send(msg);
    }
}

impl Default for PipelineManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineManager {
    pub fn new() -> Self {
        Self {
            active: DashMap::new(),
        }
    }

    pub fn get_progress(&self, server_id: &Uuid) -> Option<PhaseProgress> {
        self.active
            .get(server_id)
            .map(|h| h.progress.lock().clone())
    }

    pub fn subscribe(
        &self,
        server_id: &Uuid,
    ) -> Option<tokio::sync::broadcast::Receiver<WsMessage>> {
        self.active.get(server_id).map(|h| h.log_tx.subscribe())
    }

    pub fn is_running(&self, server_id: &Uuid) -> bool {
        self.active
            .get(server_id)
            .is_some_and(|h| h.progress.lock().status == PhaseStatus::Running)
    }

    pub fn get_phase_log_buffer(&self, server_id: &Uuid) -> Vec<PhaseLogLine> {
        match self.active.get(server_id) {
            Some(h) => h.phase_log_buffer.lock().iter().cloned().collect(),
            None => vec![],
        }
    }

    pub fn get_process_config(&self, server_id: &Uuid) -> Option<ProcessConfig> {
        self.active
            .get(server_id)
            .map(|h| h.process_config.lock().clone())
    }
}

pub fn run_phase(
    state: &Arc<AppState>,
    server_id: Uuid,
    phase: PhaseKind,
    steps: Vec<PipelineStep>,
    parameter_overrides: Option<HashMap<String, String>>,
) -> Result<(), crate::error::AppError> {
    if state.pipeline_manager.is_running(&server_id) {
        return Err(crate::error::AppError::Conflict(
            "A pipeline is already running for this server".into(),
        ));
    }

    let step_progresses: Vec<StepProgress> = steps
        .iter()
        .enumerate()
        .map(|(i, s)| StepProgress {
            step_index: i as u32,
            step_name: s.name.clone(),
            status: PhaseStatus::Pending,
            message: None,
            started_at: None,
            completed_at: None,
        })
        .collect();

    let progress = PhaseProgress {
        server_id,
        phase,
        status: PhaseStatus::Running,
        steps: step_progresses,
        started_at: Some(Utc::now()),
        completed_at: None,
    };

    let log_tx = state.process_manager.ensure_handle(server_id);

    let handle = Arc::new(PipelineHandle {
        progress: parking_lot::Mutex::new(progress),
        log_tx,
        task_handle: parking_lot::Mutex::new(None),
        phase_log_buffer: parking_lot::Mutex::new(VecDeque::with_capacity(256)),
        process_config: parking_lot::Mutex::new(ProcessConfig::default()),
    });

    state
        .pipeline_manager
        .active
        .insert(server_id, Arc::clone(&handle));

    handle.broadcast_progress();

    let state_clone = Arc::clone(state);
    let task = tokio::spawn(async move {
        runner::run_pipeline_task(
            state_clone,
            server_id,
            phase,
            steps,
            handle,
            parameter_overrides,
        )
        .await;
    });

    if let Some(h) = state.pipeline_manager.active.get(&server_id) {
        *h.task_handle.lock() = Some(task);
    }

    Ok(())
}
