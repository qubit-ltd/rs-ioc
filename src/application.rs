// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Unique lifecycle ownership of a constructed application.

use crate::application_context::ApplicationContext;
use crate::application_state::ApplicationState;
use crate::builder::ContainerBuilder;
use crate::managed::CleanupJournal;
use crate::managed::ShutdownHandle;
use crate::managed::ShutdownMode;
use crate::managed::WaitPolicy;

/// Owns a constructed application's managed lifecycle independently of queries.
///
/// Dropping this owner requests abort without creating or polling waits.
/// Use [`Self::begin_shutdown`] to observe cleanup results and await
/// termination.
#[must_use = "retain the application owner until shutdown is requested"]
pub struct Application {
    /// Shared immutable component queries and observable lifecycle state.
    context: ApplicationContext,
    /// One-shot lifecycle ownership, taken when shutdown starts.
    cleanup: Option<CleanupJournal>,
    /// Explicit wait policy retained for shutdown.
    policy: WaitPolicy,
}

impl Application {
    /// Returns an empty application builder.
    #[must_use]
    pub fn builder() -> ContainerBuilder {
        ContainerBuilder::new()
    }

    /// Attaches unique cleanup ownership to a successfully constructed context.
    pub(crate) fn new(context: ApplicationContext, cleanup: CleanupJournal, policy: WaitPolicy) -> Self {
        Self {
            context,
            cleanup: Some(cleanup),
            policy,
        }
    }

    /// Borrows the cloneable component query handle.
    #[must_use]
    pub fn context(&self) -> &ApplicationContext {
        &self.context
    }

    /// Transfers lifecycle ownership into a resumable shutdown handle.
    ///
    /// Immediate mode requests every abort before returning. Graceful mode
    /// starts the first request when the returned handle's wait is polled.
    /// Callback failures are retained in the handle's report.
    pub fn begin_shutdown(mut self, mode: ShutdownMode) -> ShutdownHandle {
        self.context.publish_state(ApplicationState::ShuttingDown);
        ShutdownHandle::new(
            self.context.clone(),
            self.cleanup
                .take()
                .expect("application owns its cleanup until shutdown"),
            self.policy.clone(),
            mode,
        )
    }
}

impl Drop for Application {
    /// Requests all remaining aborts without creating or polling wait futures.
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            self.context.publish_state(ApplicationState::ShuttingDown);
            let handle = ShutdownHandle::new(
                self.context.clone(),
                cleanup,
                self.policy.clone(),
                ShutdownMode::Immediate,
            );
            drop(handle);
        }
    }
}
