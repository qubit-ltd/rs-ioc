// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow multiple-public-types
//! Opt-in component shutdown actions owned by a built application context.

use std::any::Any;
use std::error::Error;
use std::future::Future;
use std::future::poll_fn;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;

use thiserror::Error;

use crate::key::BindingKey;
use crate::options::DefinitionSource;
use crate::store::ErasedInstance;
use crate::store::InstanceStore;

/// A future used to wait for a managed component to finish shutting down.
///
/// The future is `Send` and owns everything needed to finish after the
/// component context begins shutdown.
pub type CleanupFuture = Pin<Box<dyn Future<Output = Result<(), CleanupError>> + Send + 'static>>;

/// A sendable future that constructs one managed component.
///
/// The runtime polls the future only while executing an asynchronous build.
///
/// # Type Parameters
///
/// `T` is the thread-safe component value returned by the future.
pub type ManagedFactoryFuture<T> =
    Pin<Box<dyn Future<Output = Result<Managed<T>, crate::error::FactoryError>> + Send + 'static>>;

/// Converts a cleanup callback or future panic into a structured cleanup error.
///
/// # Parameters
///
/// * `action` - Lifecycle operation whose unwind was caught.
/// * `payload` - Panic payload returned by `catch_unwind`.
///
/// # Returns
///
/// A cleanup error retaining the panic message in its source.
fn panic_cleanup_error(action: &'static str, payload: Box<dyn Any + Send>) -> CleanupError {
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic payload");
    CleanupError::new(std::io::Error::other(format!("{action} panicked: {message}")))
}

/// Starts one wait callback while converting any callback panic to an error.
///
/// The returned future is still polled separately so panics during polling are
/// attributed to the wait future.
///
/// # Parameters
///
/// * `wait` - One-shot callback that creates the component's wait future.
///
/// # Returns
///
/// The created cleanup future.
///
/// # Errors
///
/// Returns a cleanup error if the callback panics while creating the future.
fn start_wait(wait: ErasedWait) -> Result<CleanupFuture, CleanupError> {
    catch_unwind(AssertUnwindSafe(wait)).map_err(|payload| panic_cleanup_error("wait callback", payload))
}

/// Polls a retained wait future and converts an unwind panic into a cleanup
/// error.
///
/// The future is borrowed so a `Pending` result preserves its state if the
/// caller cancels the surrounding wait operation.
///
/// # Parameters
///
/// * `future` - Retained future for one component's wait action.
///
/// # Returns
///
/// `Ok(())` when the wait future succeeds.
///
/// # Errors
///
/// Returns its cleanup error, or a cleanup error containing a caught panic.
async fn poll_wait(future: &mut CleanupFuture) -> Result<(), CleanupError> {
    poll_fn(
        |context| match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(context))) {
            Ok(Poll::Ready(result)) => Poll::Ready(result),
            Ok(Poll::Pending) => Poll::Pending,
            Err(payload) => Poll::Ready(Err(panic_cleanup_error("wait future", payload))),
        },
    )
    .await
}

/// An error returned by a component stop or wait action.
///
/// # Examples
///
/// ```
/// use qubit_ioc::CleanupError;
///
/// let error = CleanupError::new(std::io::Error::other("stop failed"));
/// assert!(std::error::Error::source(&error).is_some());
/// ```
#[derive(Debug, Error)]
#[error("component cleanup failed: {source}")]
pub struct CleanupError {
    /// The original cleanup error.
    #[source]
    source: Box<dyn Error + Send + Sync + 'static>,
}

impl CleanupError {
    /// Wraps a cleanup error while retaining its source chain.
    ///
    /// # Parameters
    ///
    /// `source` is the original error produced by a stop or wait action.
    ///
    /// # Returns
    ///
    /// A cleanup error whose source chain contains `source`.
    pub fn new<E: Error + Send + Sync + 'static>(source: E) -> Self {
        Self {
            source: Box::new(source),
        }
    }
}

/// Identifies which lifecycle phase failed.
///
/// # Examples
///
/// ```
/// use qubit_ioc::ShutdownPhase;
///
/// assert_ne!(ShutdownPhase::Stop, ShutdownPhase::Wait);
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownPhase {
    /// The component's synchronous stop action failed.
    Stop,
    /// The component's asynchronous wait action failed.
    Wait,
}

/// One failed component cleanup action with its registered binding metadata.
///
/// # Examples
///
/// ```
/// use qubit_ioc::BindingKey;
/// use qubit_ioc::CleanupError;
/// use qubit_ioc::DefinitionSource;
/// use qubit_ioc::ShutdownFailure;
/// use qubit_ioc::ShutdownPhase;
///
/// let failure = ShutdownFailure {
///     key: BindingKey::of::<String>(None),
///     definition: DefinitionSource::new("app", "app", "src/main.rs", 1, 1, "Worker"),
///     phase: ShutdownPhase::Stop,
///     error: CleanupError::new(std::io::Error::other("stop failed")),
/// };
/// assert_eq!(failure.phase, ShutdownPhase::Stop);
/// ```
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
///
/// # Examples
///
/// ```
/// use qubit_ioc::ShutdownError;
///
/// let error = ShutdownError::new(Vec::new());
/// assert!(error.failures().is_empty());
/// ```
#[derive(Clone, Debug, Error)]
#[error("{} component cleanup action(s) failed", failures.len())]
pub struct ShutdownError {
    /// Cleanup failures in the order their actions were attempted.
    failures: Arc<[ShutdownFailure]>,
}

impl ShutdownError {
    /// Creates a shutdown error from all observed stop and wait failures.
    ///
    /// # Parameters
    ///
    /// `failures` contains cleanup failures in the order their actions ran.
    ///
    /// # Returns
    ///
    /// An error that can be cloned without cloning its original error sources.
    pub fn new(failures: Vec<ShutdownFailure>) -> Self {
        Self {
            failures: Arc::from(failures),
        }
    }

    /// Returns every recorded cleanup failure in action order.
    #[must_use]
    pub fn failures(&self) -> &[ShutdownFailure] {
        &self.failures
    }
}

/// Owns cleanup after stop requests have been sent and resumes waits safely.
///
/// The handle retains a currently polled wait future. If a caller cancels the
/// future returned by [`Self::wait`], calling `wait` again resumes that same
/// future. Dropping the handle drops unfinished waits; stop actions have
/// already run.
pub struct ShutdownHandle {
    /// Keeps managed values alive until shutdown waiting finishes or is
    /// dropped.
    _store: InstanceStore,
    /// Cleanup actions retained until their waits complete.
    cleanup: CleanupJournal,
    /// Number of entries whose wait callback has not started.
    next_wait: usize,
    /// Wait future retained across cancellation of a `wait` call.
    active_wait: Option<CleanupFuture>,
    /// Binding metadata corresponding to `active_wait`.
    active_binding: Option<(BindingKey, DefinitionSource)>,
    /// Stop and wait failures accumulated so far.
    failures: Vec<ShutdownFailure>,
    /// Final result, set once every wait has completed.
    result: Option<ShutdownError>,
}

impl ShutdownHandle {
    /// Takes ownership of cleanup after all stop callbacks were attempted.
    pub(crate) fn new(store: InstanceStore, mut cleanup: CleanupJournal, failures: Vec<ShutdownFailure>) -> Self {
        cleanup.disarm_abort();
        let next_wait = cleanup.entries.len();
        Self {
            _store: store,
            cleanup,
            next_wait,
            active_wait: None,
            active_binding: None,
            failures,
            result: None,
        }
    }

    /// Waits for managed components in reverse construction order.
    ///
    /// The future borrows this handle, which must remain available if the
    /// caller cancels the wait. Repeated calls resume pending work and return
    /// the same completed result without rerunning callbacks.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` when every stop and wait action succeeded.
    ///
    /// # Errors
    ///
    /// Returns a cloneable [`ShutdownError`] containing all stop and wait
    /// failures observed after the final wait completes. Panics while creating
    /// or polling a wait future are recorded as wait failures and do not
    /// prevent later wait actions from running.
    pub async fn wait(&mut self) -> Result<(), ShutdownError> {
        if let Some(result) = &self.result {
            return Self::clone_result(result);
        }
        loop {
            if self.active_wait.is_none() {
                while self.next_wait > 0 {
                    self.next_wait -= 1;
                    let entry = &mut self.cleanup.entries[self.next_wait];
                    if let Some(wait) = entry.action.wait.take() {
                        match start_wait(wait) {
                            Ok(future) => {
                                self.active_binding = Some((entry.key.clone(), entry.definition));
                                self.active_wait = Some(future);
                                break;
                            }
                            Err(error) => self.failures.push(ShutdownFailure {
                                key: entry.key.clone(),
                                definition: entry.definition,
                                phase: ShutdownPhase::Wait,
                                error,
                            }),
                        }
                    }
                }
                if self.active_wait.is_none() {
                    let result = ShutdownError::new(std::mem::take(&mut self.failures));
                    self.result = Some(result);
                    return Self::clone_result(self.result.as_ref().expect("shutdown result was set"));
                }
            }

            let result = poll_wait(self.active_wait.as_mut().expect("active wait was initialized")).await;
            self.active_wait = None;
            let (key, definition) = self.active_binding.take().expect("active binding was set");
            if let Err(error) = result {
                self.failures.push(ShutdownFailure {
                    key,
                    definition,
                    phase: ShutdownPhase::Wait,
                    error,
                });
            }
        }
    }

    /// Clones the final result while sharing its retained source errors.
    fn clone_result(result: &ShutdownError) -> Result<(), ShutdownError> {
        if result.failures().is_empty() {
            Ok(())
        } else {
            Err(result.clone())
        }
    }
}

/// A component value paired with explicit application shutdown actions.
///
/// `stop` is synchronous so an in-progress asynchronous build can request
/// shutdown if its future is cancelled. `wait` is optional and runs during an
/// explicit asynchronous shutdown or a returned asynchronous build failure.
///
/// # Type Parameters
///
/// `T` is the thread-safe component value managed by the cleanup actions.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::Managed;
///
/// let _worker = Managed::new(Arc::new("worker"), |_| Ok(()));
/// ```
pub struct Managed<T: ?Sized + Send + Sync + 'static> {
    value: Arc<T>,
    stop: Box<dyn FnOnce(Arc<T>) -> Result<(), CleanupError> + Send + 'static>,
    wait: Option<Box<dyn FnOnce(Arc<T>) -> CleanupFuture + Send + 'static>>,
}

impl<T: ?Sized + Send + Sync + 'static> Managed<T> {
    /// Creates a managed value with a synchronous stop action.
    ///
    /// The stop closure runs once during explicit shutdown or when an
    /// asynchronous build future is cancelled after construction.
    ///
    /// # Type Parameters
    ///
    /// `F` is a sendable one-shot cleanup action that consumes a shared
    /// component handle.
    ///
    /// # Parameters
    ///
    /// `value` is the shared component exposed to lookups. `stop` releases its
    /// external resources and may return a [`CleanupError`].
    ///
    /// # Returns
    ///
    /// A managed component with no asynchronous wait action.
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
    ///
    /// # Type Parameters
    ///
    /// `F` is a sendable one-shot callback that returns the owned wait future.
    ///
    /// # Parameters
    ///
    /// `wait` runs after stop actions during asynchronous shutdown.
    ///
    /// # Returns
    ///
    /// This managed component with the wait action installed, replacing any
    /// previously configured wait action.
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

    /// Adds one constructed component's cleanup actions in construction order.
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
            if let Some(stop) = entry.action.stop.take() {
                let error = match catch_unwind(AssertUnwindSafe(stop)) {
                    Ok(Ok(())) => None,
                    Ok(Err(error)) => Some(error),
                    Err(payload) => Some(panic_cleanup_error("stop callback", payload)),
                };
                if let Some(error) = error {
                    failures.push(ShutdownFailure {
                        key: entry.key.clone(),
                        definition: entry.definition,
                        phase: ShutdownPhase::Stop,
                        error,
                    });
                }
            }
        }
        failures
    }

    /// Waits in reverse construction order after all stop actions have run.
    pub(crate) async fn wait_reverse(&mut self) -> Vec<ShutdownFailure> {
        let mut failures = Vec::new();
        for entry in self.entries.iter_mut().rev() {
            if let Some(wait) = entry.action.wait.take() {
                let result = match start_wait(wait) {
                    Ok(mut future) => poll_wait(&mut future).await,
                    Err(error) => Err(error),
                };
                if let Err(error) = result {
                    failures.push(ShutdownFailure {
                        key: entry.key.clone(),
                        definition: entry.definition,
                        phase: ShutdownPhase::Wait,
                        error,
                    });
                }
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
