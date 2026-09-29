// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Errors raised when a factory accesses an undeclared dependency.

use thiserror::Error;

use crate::dependency::Dependency;
use crate::options::DefinitionSource;

/// Errors from a factory's access to its declared dependencies.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::BuildAccessError;
/// use qubit_ioc::ContainerBuilder;
///
/// let mut builder = ContainerBuilder::new();
/// builder.register_factory::<u64, _>(&[], |context| {
///     assert!(matches!(context.get::<u32>(), Err(BuildAccessError::UndeclaredDependency { .. })));
///     Ok(Arc::new(1))
/// })?;
/// builder.build_all()?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Error)]
#[must_use = "dependency access errors must be handled or explicitly discarded"]
pub enum BuildAccessError {
    /// The factory requested a dependency absent from its declaration.
    #[error("factory at {definition} did not declare dependency {dependency:?}")]
    UndeclaredDependency {
        /// Factory definition.
        definition: DefinitionSource,
        /// Requested dependency.
        dependency: Dependency,
    },
}
