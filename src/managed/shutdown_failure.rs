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
///     phase: ShutdownPhase::Abort,
///     error: CleanupError::new(std::io::Error::other("stop failed")),
/// };
/// assert_eq!(failure.phase, ShutdownPhase::Abort);
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
