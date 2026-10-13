// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Scope of static graph validation for root-scoped construction.

/// Selects which active definitions are checked before building requested
/// roots.
///
/// Both modes construct only the root dependency closure. `AllActive` checks
/// every active definition after profile filtering and replacement, while
/// construction remains limited to the dependency closure of the selected
/// roots. Validation does not run unselected factories or require them to
/// satisfy their async mode or wait policy.
///
/// # Examples
///
/// `AllActive` validates all active definitions, but this example constructs
/// only the dependency closure of its selected root:
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::{ContainerBuilder, ValidationScope};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut builder = ContainerBuilder::new()
///     .validation_scope(ValidationScope::AllActive);
/// builder.register_instance(Arc::new(String::from("ready")))?;
/// builder.root::<String>();
/// let application = builder.build()?;
/// assert_eq!(application.context().get::<String>()?.as_str(), "ready");
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationScope {
    /// Validate only definitions reachable from selected roots.
    #[default]
    Reachable,
    /// Validate every active definition before selecting the root closure.
    AllActive,
}
