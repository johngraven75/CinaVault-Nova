//! Background enrichment jobs — the UI must never block on metadata.
//!
//! A job is claimed in a single registry slot per kind, runs on a worker
//! thread, and returns immediately with a job id and `started`/`busy`
//! status. Callers poll `status(job_id)`. If a job of the same kind is
//! already running, the new call returns `busy` with the running job's
//! id — it never stacks a second run on top of the first, and it never
//! awaits the whole library inline. A 6-hour backstop marks a stuck job
//! failed; individual provider chains are bounded separately in
//! `enrichment`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Started,
    Busy,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStart {
    pub job_id: String,
    pub kind: String,
    pub status: JobStatus,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInfo {
    pub job_id: String,
    pub kind: String,
    pub status: JobStatus,
    pub message: String,
    /// Full old-style report, present only on completion.
    pub result: Option<String>,
}

struct Entry {
    kind: String,
    status: JobStatus,
    message: String,
    result: Option<String>,
    started: Instant,
}

#[derive(Default)]
pub struct JobRegistry {
    entries: Mutex<HashMap<String, Entry>>,
    counter: Mutex<u64>,
}

impl JobRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Claim a slot for `kind`, or return `busy` with the running id.
    pub fn claim(&self, kind: &str) -> JobStart {
        let mut entries = self.entries.lock().unwrap();
        for (id, e) in entries.iter() {
            if e.kind == kind && e.status == JobStatus::Running {
                return JobStart {
                    job_id: id.clone(),
                    kind: kind.into(),
                    status: JobStatus::Busy,
                    message: format!("a {kind} job is already running"),
                };
            }
        }
        let mut counter = self.counter.lock().unwrap();
        *counter += 1;
        let id = format!("{kind}-{}", *counter);
        entries.insert(
            id.clone(),
            Entry {
                kind: kind.into(),
                status: JobStatus::Running,
                message: "started".into(),
                result: None,
                started: Instant::now(),
            },
        );
        JobStart {
            job_id: id,
            kind: kind.into(),
            status: JobStatus::Started,
            message: "started".into(),
        }
    }

    pub fn complete(&self, job_id: &str, result: String) {
        if let Some(e) = self.entries.lock().unwrap().get_mut(job_id) {
            e.status = JobStatus::Completed;
            e.message = "completed".into();
            e.result = Some(result);
        }
    }

    pub fn fail(&self, job_id: &str, message: String) {
        if let Some(e) = self.entries.lock().unwrap().get_mut(job_id) {
            e.status = JobStatus::Failed;
            e.message = message;
        }
    }

    pub fn status(&self, job_id: Option<&str>) -> Option<JobInfo> {
        let entries = self.entries.lock().unwrap();
        let (id, e) = match job_id {
            Some(id) => entries.get(id).map(|e| (id.to_string(), e))?,
            None => entries.iter().last().map(|(id, e)| (id.clone(), e))?,
        };
        let mut status = e.status;
        if status == JobStatus::Running && e.started.elapsed() > Duration::from_secs(6 * 3600) {
            status = JobStatus::Failed;
        }
        Some(JobInfo {
            job_id: id,
            kind: e.kind.clone(),
            status,
            message: e.message.clone(),
            result: e.result.clone(),
        })
    }
}

pub type SharedRegistry = Arc<JobRegistry>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_claim_is_busy_with_running_id() {
        let reg = JobRegistry::new();
        let first = reg.claim("enrichment");
        assert_eq!(first.status, JobStatus::Started);
        let second = reg.claim("enrichment");
        assert_eq!(second.status, JobStatus::Busy);
        assert_eq!(second.job_id, first.job_id);
        reg.complete(&first.job_id, "ok".into());
        let third = reg.claim("enrichment");
        assert_eq!(third.status, JobStatus::Started);
        assert_eq!(reg.status(Some(&first.job_id)).unwrap().status, JobStatus::Completed);
    }
}
