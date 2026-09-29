// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Single-owner handle that completes managed component shutdown.

use crate::key::BindingKey;
use crate::managed::CleanupFuture;
use crate::managed::ShutdownError;
use crate::managed::ShutdownFailure;
use crate::managed::ShutdownPhase;
use crate::managed::internal::cleanup_journal::CleanupJournal;
use crate::managed::internal::wait::poll_wait;
use crate::managed::internal::wait::start_wait;
use crate::options::DefinitionSource;
use crate::store::InstanceStore;

/// Owns cleanup after stop requests have been sent and resumes waits safely.
///
/// The handle retains a currently polled wait future. If a caller cancels the
/// future returned by [`Self::wait`], calling `wait` again resumes that same
/// future. Dropping the handle drops unfinished waits; stop actions have
/// already run.
///
/// Dropping an [`ApplicationContext`](crate::ApplicationContext) does not run
/// managed stop actions. Call [`crate::ApplicationContext::begin_shutdown`] to
/// request stops explicitly, then keep this handle and await [`Self::wait`].
/// Dropping the returned handle abandons unfinished waits.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::{ContainerBuilder, Managed};
///
/// let mut builder = ContainerBuilder::new();
/// builder.register_managed_factory::<String, _>(&[], |_| {
///     Ok(Managed::new(Arc::new(String::from("worker")), |_| Ok(())))
/// })?;
/// builder.root::<String>();
/// let context = builder.build()?;
/// let shutdown = context.begin_shutdown();
/// # async fn wait_for_shutdown(mut shutdown: qubit_ioc::ShutdownHandle)
/// # -> Result<(), qubit_ioc::ShutdownError> {
/// shutdown.wait().await?;
/// # Ok(()) }
/// # let _ = wait_for_shutdown(shutdown);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
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
