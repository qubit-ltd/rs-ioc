// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cardinality for a dependency request.

/// Number of matching bindings requested by a factory.
///
/// # Examples
///
/// ```
/// use qubit_ioc::Dependency;
/// use qubit_ioc::DependencyCardinality;
///
/// assert_eq!(Dependency::optional::<String>().cardinality(), DependencyCardinality::Optional);
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DependencyCardinality {
    /// Exactly one matching binding must be selected.
    Required,
    /// Zero or one matching binding may be selected.
    Optional,
    /// Every matching binding is selected, possibly none.
    All,
}
