// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Errors raised while validating and constructing a component graph.

use thiserror::Error;

use super::FactoryError;
use crate::dependency::Dependency;
use crate::key::BindingKey;
use crate::managed::ShutdownFailure;
use crate::options::DefinitionSource;

/// Errors detected before or during container construction.
///
/// # Examples
///
/// ```
/// use qubit_ioc::BuildError;
/// use qubit_ioc::ContainerBuilder;
///
/// let error = match ContainerBuilder::new().build() {
///     Ok(_) => panic!("a root is required"),
///     Err(error) => error,
/// };
/// assert!(matches!(error, BuildError::NoRootsSelected));
/// ```
#[derive(Debug, Error)]
pub enum BuildError {
    /// A factory failed and one or more managed components also failed to clean
    /// up.
    #[error("build failed: {cause}; cleanup failures: {failures:?}")]
    CleanupFailed {
        /// Original graph or factory construction failure.
        #[source]
        cause: Box<BuildError>,
        /// Stop or wait failures collected while unwinding the build.
        failures: Vec<ShutdownFailure>,
    },
    /// A root-scoped build was requested without selecting a root component.
    #[error("no build roots were selected; register at least one root or call build_all")]
    NoRootsSelected,
    /// No active binding satisfies a required root request.
    #[error("missing root {request:?}; available bindings: {available:?}")]
    MissingRoot {
        /// Requested root binding.
        request: Dependency,
        /// Active bindings of the requested type.
        available: Vec<BindingKey>,
    },
    /// Several active bindings satisfy a root request without a unique primary.
    #[error("ambiguous root {request:?}; candidates: {candidates:?}")]
    AmbiguousRoot {
        /// Requested root binding.
        request: Dependency,
        /// Active matching bindings.
        candidates: Vec<BindingKey>,
    },
    /// Two active definitions own the same exact key.
    #[error("duplicate binding {key:?} from {first} and {second}")]
    DuplicateBinding {
        /// Conflicting key.
        key: BindingKey,
        /// First definition.
        first: DefinitionSource,
        /// Second definition.
        second: DefinitionSource,
    },
    /// A replacement has no earlier active binding for its exact key.
    #[error("replacement {replacement} has no active original for {key:?}")]
    ReplacementOriginalMissing {
        /// Exact key requested by the replacement.
        key: BindingKey,
        /// Definition that supplies the replacement binding.
        replacement: DefinitionSource,
    },
    /// A replacement has multiple earlier active bindings for its exact key.
    #[error("replacement {replacement} has multiple active originals for {key:?}: {originals:?}")]
    ReplacementOriginalAmbiguous {
        /// Exact key requested by the replacement.
        key: BindingKey,
        /// Earlier active definitions in registration order.
        originals: Vec<DefinitionSource>,
        /// Definition that supplies the replacement binding.
        replacement: DefinitionSource,
    },
    /// An alias's concrete target was removed or is otherwise absent after
    /// filtering.
    #[error("alias {alias:?} from {definition} has missing target {target:?}; path: {path:?}")]
    MissingAliasTarget {
        /// Alias whose target cannot be resolved.
        alias: BindingKey,
        /// Missing concrete target key.
        target: BindingKey,
        /// Definition that declared the alias.
        definition: DefinitionSource,
        /// Complete binding path to the alias.
        path: Vec<BindingKey>,
    },
    /// No active binding satisfies a required request.
    #[error("missing dependency {dependency:?} for {definition}; path: {path:?}")]
    MissingDependency {
        /// Unsatisfied request.
        dependency: Dependency,
        /// Requesting definition.
        definition: DefinitionSource,
        /// Complete binding path to the request.
        path: Vec<BindingKey>,
    },
    /// Several bindings satisfy a single request without a unique primary.
    #[error("ambiguous dependency {dependency:?} for {definition}; candidates: {candidates:?}; path: {path:?}")]
    AmbiguousBinding {
        /// Ambiguous request.
        dependency: Dependency,
        /// Requesting definition.
        definition: DefinitionSource,
        /// Matching keys.
        candidates: Vec<BindingKey>,
        /// Complete binding path to the request.
        path: Vec<BindingKey>,
    },
    /// Several active primary bindings exist for one Rust type.
    #[error("multiple primary bindings: {candidates:?}")]
    MultiplePrimaryBindings {
        /// Conflicting primary keys and their sources.
        candidates: Vec<(BindingKey, DefinitionSource)>,
    },
    /// The dependency graph contains a cycle.
    #[error("dependency cycle: {path:?}")]
    DependencyCycle {
        /// Complete cycle path, including the repeated first key.
        path: Vec<BindingKey>,
    },
    /// A synchronous build encountered an active asynchronous factory.
    #[error("asynchronous factory at {definition} requires build_async")]
    AsyncRequired {
        /// Factory definition.
        definition: DefinitionSource,
        /// Factory binding key.
        key: BindingKey,
    },
    /// A user factory failed during construction.
    #[error("factory at {definition} failed for {key:?}; path: {path:?}: {error}")]
    FactoryFailed {
        /// Factory definition.
        definition: DefinitionSource,
        /// Factory binding key.
        key: BindingKey,
        /// Complete dependency path.
        path: Vec<BindingKey>,
        /// Original user factory failure.
        #[source]
        error: FactoryError,
    },
    /// A configuration read failed during construction.
    #[error("configuration read at {definition} for {target} from `{path_key}`; path: {path:?}: {error}")]
    ConfigReadFailed {
        /// Definition containing the read.
        definition: DefinitionSource,
        /// Configuration path.
        path_key: String,
        /// Field or parameter being populated.
        target: String,
        /// Complete dependency path.
        path: Vec<BindingKey>,
        /// Original configuration failure.
        #[source]
        error: FactoryError,
    },
}
