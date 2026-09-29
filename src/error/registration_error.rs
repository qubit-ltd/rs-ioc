// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Errors raised while registering definitions.

use thiserror::Error;

use crate::dependency::Dependency;
use crate::error::InvalidBindingId;
use crate::key::BindingKey;
use crate::options::DefinitionSource;

/// Errors detected while adding one definition to a builder.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::BindingOptions;
/// use qubit_ioc::ContainerBuilder;
/// use qubit_ioc::RegistrationError;
///
/// let mut builder = ContainerBuilder::new();
/// let result = builder.register_instance_with(
///     Arc::new(1_u32),
///     BindingOptions { id: Some("bad-id".to_owned()), ..BindingOptions::default() },
/// );
/// assert!(matches!(result, Err(RegistrationError::InvalidBindingId { .. })));
/// ```
#[derive(Debug, Error)]
#[must_use = "registration errors must be handled or explicitly discarded"]
pub enum RegistrationError {
    /// An interface alias attempted to use a different activation profile.
    #[error("{definition}: alias profile {alias:?} differs from concrete profile {concrete:?}")]
    AliasProfileMismatch {
        /// Definition containing the alias.
        definition: DefinitionSource,
        /// Concrete binding profile.
        concrete: Option<String>,
        /// Alias binding profile.
        alias: Option<String>,
    },
    /// A replacement callback staged zero or multiple definitions.
    #[error("replacement callback must stage exactly one definition; got {count}")]
    ReplacementDefinitionCount {
        /// Number of staged definitions.
        count: usize,
    },
    /// A replacement definition declared its anchor zero or multiple times.
    #[error("replacement definition must declare anchor {anchor:?} exactly once; got {count}")]
    ReplacementAnchorCount {
        /// Exact key identifying the original definition.
        anchor: BindingKey,
        /// Number of matching bindings in the replacement definition.
        count: usize,
    },
    /// A pending definition has no concrete binding.
    #[error("{definition}: definition has no concrete binding")]
    EmptyDefinition {
        /// Definition missing its required concrete binding.
        definition: DefinitionSource,
    },
    /// The first binding is an alias rather than an instance or factory.
    #[error("{definition}: first binding {key:?} must be concrete")]
    InvalidConcreteBinding {
        /// Binding in the concrete position.
        key: BindingKey,
        /// Definition containing the binding.
        definition: DefinitionSource,
    },
    /// A binding after the concrete binding is not an alias.
    #[error("{definition}: binding {key:?} after the concrete binding must be an alias")]
    InvalidAliasBinding {
        /// Binding in an alias position.
        key: BindingKey,
        /// Definition containing the binding.
        definition: DefinitionSource,
    },
    /// An interface alias points outside its own definition.
    #[error("{definition}: alias {alias:?} targets {target:?}, expected concrete key {expected:?}")]
    InvalidAliasTarget {
        /// Alias being declared.
        alias: BindingKey,
        /// Supplied target key.
        target: BindingKey,
        /// This definition's concrete binding key.
        expected: BindingKey,
        /// Definition containing the alias.
        definition: DefinitionSource,
    },
    /// One definition declares the same exact binding key more than once.
    #[error("{definition}: duplicate key {key:?} within one definition")]
    DuplicateDefinitionKey {
        /// Repeated key within the definition.
        key: BindingKey,
        /// Definition containing both conflicting declarations.
        definition: DefinitionSource,
    },
    /// A binding or dependency ID is invalid.
    #[error("{definition}: {error}")]
    InvalidBindingId {
        /// Original parse error.
        #[source]
        error: InvalidBindingId,
        /// Definition that supplied the ID.
        definition: DefinitionSource,
    },
    /// A profile name does not match `[A-Za-z][A-Za-z0-9_-]*`.
    #[error("{definition}: invalid profile `{value}`")]
    InvalidProfile {
        /// Original profile text.
        value: String,
        /// Definition that supplied the profile.
        definition: DefinitionSource,
    },
    /// A factory declared the same type, ID, and cardinality twice.
    #[error("{definition}: duplicate dependency {dependency:?}")]
    DuplicateDependency {
        /// Repeated request.
        dependency: Dependency,
        /// Definition that declared it.
        definition: DefinitionSource,
    },
}
