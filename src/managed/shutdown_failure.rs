// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Component cleanup failure and its registered binding metadata.

use thiserror::Error;

use crate::key::BindingKey;
use crate::managed::CleanupError;
use crate::managed::ShutdownPhase;
use crate::options::DefinitionSource;

/// A cleanup failure produced while shutdown processes a registered component.
///
/// The value retains the binding identity, the source location that registered
/// it, the lifecycle phase, and the original error. Shutdown reports expose
/// these values so callers can identify which cleanup action failed and inspect
/// its cause.
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
///     phase: ShutdownPhase::Abort,
///     error: CleanupError::new(std::io::Error::other("stop failed")),
/// };
/// assert_eq!(failure.phase, ShutdownPhase::Abort);
/// ```
#[derive(Debug, Error)]
#[must_use = "inspect the failed cleanup or explicitly discard it"]
#[error("{phase:?} cleanup for {key:?} from {definition} failed: {error}")]
pub struct ShutdownFailure {
    /// Identity of the binding whose cleanup action returned this failure.
    pub key: BindingKey,
    /// Source location and item name that registered the failed binding.
    pub definition: DefinitionSource,
    /// Shutdown operation during which the cleanup action failed.
    pub phase: ShutdownPhase,
    /// Original error returned or retained by the component cleanup action.
    #[source]
    pub error: CleanupError,
}
