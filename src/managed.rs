// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Opt-in component shutdown actions owned by a built application context.

use std::future::Future;
use std::pin::Pin;

mod cleanup_error;
mod internal;
#[path = "managed/managed/managed.rs"]
mod managed_type;
mod shutdown_error;
mod shutdown_failure;
mod shutdown_handle;
mod shutdown_mode;
mod shutdown_phase;
mod shutdown_report;
mod wait_policy;

pub use cleanup_error::CleanupError;
pub(crate) use internal::cleanup_action::CleanupAction;
pub(crate) use internal::cleanup_action::ErasedWait;
pub(crate) use internal::cleanup_entry::CleanupEntry;
pub(crate) use internal::cleanup_journal::CleanupJournal;
pub use managed_type::Managed;
pub use shutdown_error::ShutdownError;
pub use shutdown_failure::ShutdownFailure;
pub use shutdown_handle::ShutdownHandle;
pub use shutdown_mode::ShutdownMode;
pub use shutdown_phase::ShutdownPhase;
pub use shutdown_report::ShutdownReport;
pub use wait_policy::DeadlineFuture;
pub use wait_policy::WaitPolicy;

/// A future used to wait for a managed component to finish shutting down.
///
/// The future is `Send` and owns everything needed to finish after the
/// component context begins shutdown.
///
/// # Examples
///
/// ```
/// use qubit_ioc::managed::CleanupFuture;
///
/// let wait: CleanupFuture = Box::pin(async { Ok(()) });
/// ```
pub type CleanupFuture = Pin<Box<dyn Future<Output = Result<(), CleanupError>> + Send + 'static>>;

/// A sendable future that constructs one managed component.
///
/// The runtime polls the future only while executing an asynchronous build.
///
/// # Type Parameters
///
/// `T` is the thread-safe component value returned by the future.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::managed::ManagedFactoryFuture;
/// use qubit_ioc::Managed;
///
/// let factory: ManagedFactoryFuture<u32> = Box::pin(async {
///     Ok(Managed::synchronous(Arc::new(1), |_| Ok(())))
/// });
/// ```
pub type ManagedFactoryFuture<T> =
    Pin<Box<dyn Future<Output = Result<Managed<T>, crate::error::FactoryError>> + Send + 'static>>;
