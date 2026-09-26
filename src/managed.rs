// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Opt-in component shutdown actions owned by a built application context.

use std::any::Any;
use std::error::Error;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::pin::Pin;
use std::sync::Arc;

use thiserror::Error;

use crate::key::BindingKey;
use crate::options::DefinitionSource;
use crate::store::ErasedInstance;

/// A future used to wait for a managed component to finish shutting down.
pub type CleanupFuture = Pin<Box<dyn Future<Output = Result<(), CleanupError>> + Send + 'static>>;

/// A sendable future that constructs one managed component.
pub type ManagedFactoryFuture<T> =
    Pin<Box<dyn Future<Output = Result<Managed<T>, crate::error::FactoryError>> + Send + 'static>>;

/// An error returned by a component stop or wait action.
#[derive(Debug, Error)]
#[error("component cleanup failed: {source}")]
pub struct CleanupError {
    /// The original cleanup error.
    #[source]
    source: Box<dyn Error + Send + Sync + 'static>,
}

impl CleanupError {
    /// Wraps a cleanup error while retaining its source chain.
    pub fn new<E: Error + Send + Sync + 'static>(source: E) -> Self {
        Self {
            source: Box::new(source),
        }
    }
}

/// Identifies which lifecycle phase failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownPhase {
    /// The component's synchronous stop action failed.
    Stop,
    /// The component's asynchronous wait action failed.
    Wait,
}

/// One failed component cleanup action with its registered binding metadata.
#[derive(Debug, Error)]
#[error("{phase:?} cleanup for {key:?} from {definition} failed: {error}")]
pub struct ShutdownFailure {
    /// Binding whose cleanup failed.
    pub key: BindingKey,
    /// Definition that registered the binding.
    pub definition: DefinitionSource,
    /// Lifecycle phase that failed.
    pub phase: ShutdownPhase,
    /// Original cleanup failure.
    #[source]
    pub error: CleanupError,
}

/// All failures observed while stopping and waiting for managed components.
#[derive(Debug, Error)]
#[error("{} component cleanup action(s) failed", failures.len())]
pub struct ShutdownError {
    /// Cleanup failures in the order their actions were attempted.
    pub failures: Vec<ShutdownFailure>,
}

/// A component value paired with explicit application shutdown actions.
///
/// `stop` is synchronous so an in-progress asynchronous build can request
/// shutdown if its future is cancelled. `wait` is optional and runs during an
/// explicit asynchronous shutdown or a returned asynchronous build failure.
pub struct Managed<T: ?Sized + Send + Sync + 'static> {
    value: Arc<T>,
    stop: Box<dyn FnOnce(Arc<T>) -> Result<(), CleanupError> + Send + 'static>,
    wait: Option<Box<dyn FnOnce(Arc<T>) -> CleanupFuture + Send + 'static>>,
}

impl<T: ?Sized + Send + Sync + 'static> Managed<T> {
    /// Creates a managed value with a synchronous stop action.
    pub fn new<F>(value: Arc<T>, stop: F) -> Self
    where
        F: FnOnce(Arc<T>) -> Result<(), CleanupError> + Send + 'static,
    {
        Self {
            value,
            stop: Box::new(stop),
            wait: None,
        }
    }

    /// Adds an asynchronous action that waits for this component to terminate.
    pub fn with_wait<F>(mut self, wait: F) -> Self
    where
        F: FnOnce(Arc<T>) -> CleanupFuture + Send + 'static,
    {
        self.wait = Some(Box::new(wait));
        self
    }

    /// Converts the value and cleanup actions to type-erased internal storage.
    pub(crate) fn into_parts(self) -> (ErasedInstance, CleanupAction) {
        let value = Arc::clone(&self.value);
        let stop_value = Arc::clone(&value);
        let stop = self.stop;
        let erased_stop = Box::new(move || stop(stop_value));
        let wait = self.wait.map(|wait| {
            let wait_value = value;
            Box::new(move || wait(wait_value)) as ErasedWait
        });
        (
            Arc::new(self.value) as Arc<dyn Any + Send + Sync>,
            CleanupAction {
                stop: Some(erased_stop),
                wait,
            },
        )
    }
}

pub(crate) type ErasedStop = Box<dyn FnOnce() -> Result<(), CleanupError> + Send + 'static>;
pub(crate) type ErasedWait = Box<dyn FnOnce() -> CleanupFuture + Send + 'static>;

/// Type-erased lifecycle actions for one concrete binding.
pub(crate) struct CleanupAction {
    pub(crate) stop: Option<ErasedStop>,
    pub(crate) wait: Option<ErasedWait>,
}

/// Lifecycle action paired with the public binding metadata used in errors.
pub(crate) struct CleanupEntry {
    pub(crate) key: BindingKey,
    pub(crate) definition: DefinitionSource,
    pub(crate) action: CleanupAction,
}

/// Tracks successfully constructed managed components until publication.
pub(crate) struct CleanupJournal {
    entries: Vec<CleanupEntry>,
    abort_on_drop: bool,
}

impl Default for CleanupJournal {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            abort_on_drop: true,
        }
    }
}

impl CleanupJournal {
    /// Disables build-cancellation cleanup after ownership moves to a context.
    pub(crate) fn disarm_abort(&mut self) {
        self.abort_on_drop = false;
    }

    pub(crate) fn push(&mut self, key: BindingKey, definition: DefinitionSource, action: CleanupAction) {
        self.entries.push(CleanupEntry {
            key,
            definition,
            action,
        });
    }

    /// Calls every stop action in reverse construction order.
    pub(crate) fn stop_reverse(&mut self) -> Vec<ShutdownFailure> {
        let mut failures = Vec::new();
        for entry in self.entries.iter_mut().rev() {
            if let Some(stop) = entry.action.stop.take()
                && let Err(error) = stop()
            {
                failures.push(ShutdownFailure {
                    key: entry.key.clone(),
                    definition: entry.definition,
                    phase: ShutdownPhase::Stop,
                    error,
                });
            }
        }
        failures
    }

    /// Waits in reverse construction order after all stop actions have run.
    pub(crate) async fn wait_reverse(&mut self) -> Vec<ShutdownFailure> {
        let mut failures = Vec::new();
        for entry in self.entries.iter_mut().rev() {
            if let Some(wait) = entry.action.wait.take()
                && let Err(error) = wait().await
            {
                failures.push(ShutdownFailure {
                    key: entry.key.clone(),
                    definition: entry.definition,
                    phase: ShutdownPhase::Wait,
                    error,
                });
            }
        }
        self.entries.clear();
        failures
    }

    /// Runs stop actions during cancellation without blocking or awaiting.
    fn abort(&mut self) {
        for entry in self.entries.iter_mut().rev() {
            if let Some(stop) = entry.action.stop.take() {
                let _ = catch_unwind(AssertUnwindSafe(stop));
            }
        }
        self.entries.clear();
    }
}

impl Drop for CleanupJournal {
    fn drop(&mut self) {
        if self.abort_on_drop {
            self.abort();
        }
    }
}
