#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex};

use crate::database::Database;
use crate::engine::mover::FailpointFn;
use crate::engine::paths::ProtectedRoots;
use crate::fsops::FsBackend;
use crate::jobs::{ActiveJobs, InFlightRegistry};
use crate::services::planner::PlanStore;

/// Shared backend state managed by Tauri and tested directly in integration tests.
pub struct AppState {
    pub db: Database,
    pub protected_roots: ProtectedRoots,
    pub fs: Arc<dyn FsBackend>,
    pub plans: Mutex<PlanStore>,
    pub in_flight: InFlightRegistry,
    pub active_jobs: ActiveJobs,
    pub failpoint: Option<FailpointFn>,
}

impl AppState {
    #[must_use]
    pub fn new(db: Database, protected_roots: ProtectedRoots, fs: Arc<dyn FsBackend>) -> Self {
        Self {
            db,
            protected_roots,
            fs,
            plans: Mutex::new(PlanStore::new()),
            in_flight: InFlightRegistry::new(),
            active_jobs: ActiveJobs::new(),
            failpoint: None,
        }
    }

    #[must_use]
    pub fn with_failpoint(mut self, failpoint: FailpointFn) -> Self {
        self.failpoint = Some(failpoint);
        self
    }
}
