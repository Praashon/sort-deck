#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::errors::AppError;
use crate::fsops::CancelToken;

/// Guarantees at most one active operation per normalized source path across
/// manual sweeps, auto-sort, undo, and recovery (`AGENTS.md` §8.3).
#[derive(Debug, Default)]
pub struct InFlightRegistry {
    active_keys: Arc<Mutex<HashSet<String>>>,
}

impl InFlightRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Acquires exclusive in-flight ownership for `path_key` until the returned
    /// guard is dropped.
    pub fn try_acquire(&self, path_key: &str) -> Result<InFlightGuard, AppError> {
        let mut guard = self.active_keys.lock().map_err(|_| AppError::Internal {
            message: "in-flight registry mutex poisoned".to_owned(),
        })?;

        if !guard.insert(path_key.to_owned()) {
            return Err(AppError::Conflict {
                message: format!("path '{path_key}' already has an operation in flight"),
            });
        }

        Ok(InFlightGuard {
            key: path_key.to_owned(),
            registry: Arc::clone(&self.active_keys),
        })
    }
}

/// RAII guard releasing a path key from [`InFlightRegistry`] on drop.
pub struct InFlightGuard {
    key: String,
    registry: Arc<Mutex<HashSet<String>>>,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.registry.lock() {
            guard.remove(&self.key);
        }
    }
}

/// Tracks active cancellable batch jobs by `batch_id`.
#[derive(Debug, Default)]
pub struct ActiveJobs {
    tokens: Mutex<HashMap<String, CancelToken>>,
}

impl ActiveJobs {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, batch_id: &str, token: CancelToken) -> Result<(), AppError> {
        let mut guard = self.tokens.lock().map_err(|_| AppError::Internal {
            message: "active jobs mutex poisoned".to_owned(),
        })?;
        guard.insert(batch_id.to_owned(), token);
        Ok(())
    }

    pub fn cancel(&self, batch_id: &str) -> Result<bool, AppError> {
        let guard = self.tokens.lock().map_err(|_| AppError::Internal {
            message: "active jobs mutex poisoned".to_owned(),
        })?;
        if let Some(token) = guard.get(batch_id) {
            token.cancel();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn unregister(&self, batch_id: &str) {
        if let Ok(mut guard) = self.tokens.lock() {
            guard.remove(batch_id);
        }
    }
}
