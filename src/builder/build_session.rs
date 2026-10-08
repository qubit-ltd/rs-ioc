// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Caller-owned asynchronous construction and observable cancellation.
use crate::application::Application;
use crate::builder::ContainerBuilder;
use crate::error::BuildSessionError;
use crate::managed::ShutdownHandle;
use crate::managed::ShutdownReport;

#[path = "internal/run_guard.rs"]
mod run_guard;
#[path = "internal/session_state.rs"]
mod state;
use run_guard::RunGuard;
use state::SessionState;

/// A lazy, single-use build whose cancelled cleanup remains observable.
///
/// Create it with [`ContainerBuilder::build_async_session`] or
/// [`ContainerBuilder::build_all_async_session`]. No thread or executor is
/// created. Dropping the session requests unfinished aborts without waiting.
/// Side effects created before a factory returns its managed value remain
/// that factory's responsibility. Blocking callbacks and individual blocking
/// future polls cannot be preempted by cancellation or shutdown deadlines.
///
/// # Examples
///
/// ```
/// use qubit_ioc::ContainerBuilder;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut builder = ContainerBuilder::new();
/// builder.register_instance(std::sync::Arc::new(String::from("hello")))?;
/// builder.root::<String>();
/// let mut session = builder.build_async_session();
/// let application = session.run().await?;
/// assert_eq!(application.context().get::<String>()?.as_str(), "hello");
/// # Ok(())
/// # }
/// ```
#[must_use = "run the session and observe any cancelled cleanup"]
pub struct BuildSession {
    /// Builder retained until the first poll, then consumed exactly once.
    builder: Option<ContainerBuilder>,
    /// Whether every active definition is selected.
    all: bool,
    /// Single-use state, independent of cleanup handle transfer.
    state: SessionState,
    /// Unique rollback owner after cancellation, if any managed value exists.
    cleanup: Option<ShutdownHandle>,
}

impl BuildSession {
    /// Retains `builder` without validation; `all` selects every active
    /// binding.
    pub(super) fn new(builder: ContainerBuilder, all: bool) -> Self {
        Self {
            builder: Some(builder),
            all,
            state: SessionState::Ready,
            cleanup: None,
        }
    }

    /// Validates and constructs the selected graph on the caller's executor.
    ///
    /// Dropping this future before its first poll leaves the session ready.
    /// Dropping it after polling aborts completed managed resources and retains
    /// their cleanup handle. Success transfers ownership to the application;
    /// a construction failure transfers rollback to its `BuildFailure`.
    ///
    /// # Errors
    /// Returns [`BuildSessionError::Build`] for validation or factory failures,
    /// `Cancelled` after cancellation, and `AlreadyFinished` after any result.
    pub async fn run(&mut self) -> Result<Application, BuildSessionError> {
        match self.state {
            SessionState::Ready => {}
            SessionState::Cancelled => return Err(BuildSessionError::Cancelled),
            SessionState::Running | SessionState::Finished => return Err(BuildSessionError::AlreadyFinished),
        }
        self.state = SessionState::Running;
        let mut guard = RunGuard {
            session: self,
            construction: None,
        };
        let builder = guard.session.builder.take().expect("ready session owns its builder");
        match builder.prepare_async(guard.session.all) {
            Ok(construction) => guard.construction = Some(construction),
            Err(failure) => {
                guard.session.state = SessionState::Finished;
                return Err(failure.into());
            }
        }
        let result = guard
            .construction
            .as_mut()
            .expect("validated construction")
            .construct_async()
            .await;
        let construction = guard.construction.take().expect("running construction");
        guard.session.state = SessionState::Finished;
        match result {
            Ok(()) => Ok(construction.finish()),
            Err(error) => Err(construction.cleanup_and_wrap(error).into()),
        }
    }

    /// Waits for cancelled construction cleanup, returning its success or
    /// failure report. Returns `None` unless this session still owns cleanup.
    /// Cancelling this wait preserves its future and deadline for the next
    /// call; completed calls return the same final report.
    pub async fn wait_cancelled_cleanup(&mut self) -> Option<ShutdownReport> {
        let handle = self.cleanup.as_mut()?;
        Some(match handle.wait().await {
            Ok(report) => report,
            Err(error) => error.report().clone(),
        })
    }

    /// Transfers cancelled cleanup ownership at most once. Returns `None`
    /// before cancellation, without managed resources, or after transfer.
    #[inline]
    pub fn take_cancelled_cleanup(&mut self) -> Option<ShutdownHandle> {
        self.cleanup.take()
    }
}
